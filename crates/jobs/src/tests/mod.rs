use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

mod queue_tests;
mod worker_tests;

use crate::{
    Job, JobError, Registry, Schedule, Worker, WorkerConfig, enqueue,
    queue::{self, Failed},
    schedule,
};
use akasha_db::MIGRATOR;

#[derive(Clone, Default)]
struct Ctx {
    runs: Arc<AtomicUsize>,
    /// `(number, is_last)` of each Flaky attempt.
    attempts: Arc<std::sync::Mutex<Vec<(i32, bool)>>>,
}

#[derive(Serialize, Deserialize)]
struct Echo {
    n: u32,
}

impl Job for Echo {
    const KIND: &'static str = "test_echo";
    fn dedupe_key(&self) -> Option<String> {
        Some(self.n.to_string())
    }
}

#[derive(Serialize, Deserialize)]
struct Flaky;

impl Job for Flaky {
    const KIND: &'static str = "test_flaky";
    const MAX_ATTEMPTS: i32 = 3;
}

#[derive(Serialize, Deserialize)]
struct Doomed;

impl Job for Doomed {
    const KIND: &'static str = "test_doomed";
}

#[derive(Serialize, Deserialize)]
struct Panics;

impl Job for Panics {
    const KIND: &'static str = "test_panics";
}

#[derive(Serialize, Deserialize)]
struct Slow {
    ms: u64,
}

impl Job for Slow {
    const KIND: &'static str = "test_slow";
}

#[derive(Serialize, Deserialize)]
struct Unhandled;

impl Job for Unhandled {
    const KIND: &'static str = "test_unhandled";
}

fn registry() -> Registry<Ctx> {
    Registry::new()
        .register(|ctx: Ctx, _job: Echo| async move {
            ctx.runs.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .register(|ctx: Ctx, _job: Flaky| async move {
            ctx.runs.fetch_add(1, Ordering::SeqCst);
            let attempt = crate::current_attempt().expect("inside a worker");
            ctx.attempts
                .lock()
                .expect("lock")
                .push((attempt.number, attempt.is_last()));
            Err(JobError::retry("flaky"))
        })
        .register(|_ctx: Ctx, _job: Doomed| async { Err(JobError::permanent("never")) })
        .register(|_ctx: Ctx, _job: Panics| async {
            if true {
                panic!("boom");
            }
            Ok(())
        })
        .register(|ctx: Ctx, job: Slow| async move {
            tokio::time::sleep(Duration::from_millis(job.ms)).await;
            ctx.runs.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
}

fn worker(pool: &PgPool, ctx: &Ctx) -> Worker<Ctx> {
    Worker::new(
        pool.clone(),
        ctx.clone(),
        registry(),
        WorkerConfig::default(),
    )
}

async fn put<J: Job>(pool: &PgPool, job: &J) -> Option<Uuid> {
    let mut conn = pool.acquire().await.expect("conn");
    enqueue(&mut conn, job).await.expect("enqueue")
}

async fn status(pool: &PgPool, id: Uuid) -> queue::JobInfo {
    queue::get(pool, id)
        .await
        .expect("get")
        .expect("job exists")
}

/// Make a waiting job due now (skip its backoff).
async fn make_due(pool: &PgPool, id: Uuid) {
    sqlx::query("UPDATE jobs SET run_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .expect("make due");
}
