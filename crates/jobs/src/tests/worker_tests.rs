//! The worker: retries, dead-lettering, notify wake-up, graceful shutdown.

use std::{sync::atomic::Ordering, time::Duration};

use sqlx::PgPool;
use uuid::Uuid;

use super::*;

#[sqlx::test(migrator = "MIGRATOR")]
async fn failures_back_off_then_dead_letter(pool: PgPool) {
    let ctx = Ctx::default();
    let worker = worker(&pool, &ctx);
    let id = put(&pool, &Flaky).await.expect("id");

    assert_eq!(worker.run_until_idle().await.expect("run"), 1);
    let info = status(&pool, id).await;
    assert_eq!((info.status.as_str(), info.attempts), ("failed", 1));
    assert_eq!(info.last_error.as_deref(), Some("flaky"));
    let delay = (info.run_at - chrono::Utc::now()).num_milliseconds();
    assert!(
        (3_000..=10_500).contains(&delay),
        "first backoff 5-10 s, got {delay} ms"
    );
    assert_eq!(
        worker.run_until_idle().await.expect("run"),
        0,
        "not due yet"
    );

    make_due(&pool, id).await;
    worker.run_until_idle().await.expect("run");
    let info = status(&pool, id).await;
    assert_eq!((info.status.as_str(), info.attempts), ("failed", 2));
    let delay = (info.run_at - chrono::Utc::now()).num_milliseconds();
    assert!(
        (8_000..=20_500).contains(&delay),
        "second backoff 10-20 s, got {delay} ms"
    );

    make_due(&pool, id).await;
    worker.run_until_idle().await.expect("run");
    let info = status(&pool, id).await;
    assert_eq!((info.status.as_str(), info.attempts), ("dead", 3));
    make_due(&pool, id).await;
    assert_eq!(worker.run_until_idle().await.expect("run"), 0);
    assert_eq!(ctx.runs.load(Ordering::SeqCst), 3);
    let attempts = ctx.attempts.lock().expect("lock").clone();
    assert_eq!(attempts, vec![(1, false), (2, false), (3, true)]);
    assert_eq!(
        crate::current_attempt(),
        None,
        "no attempt outside a handler"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn permanent_errors_bad_payloads_and_panics(pool: PgPool) {
    let ctx = Ctx::default();
    let worker = worker(&pool, &ctx);
    let doomed = put(&pool, &Doomed).await.expect("id");
    let panics = put(&pool, &Panics).await.expect("id");
    let bad: Uuid = sqlx::query_scalar(
        "INSERT INTO jobs (kind, payload) VALUES ('test_echo', '{\"n\": \"seven\"}') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert");

    assert_eq!(worker.run_until_idle().await.expect("run"), 3);
    assert_eq!(status(&pool, doomed).await.status, "dead");
    let bad = status(&pool, bad).await;
    assert_eq!(bad.status, "dead");
    assert!(
        bad.last_error
            .unwrap_or_default()
            .starts_with("invalid payload")
    );
    let panics = status(&pool, panics).await;
    assert_eq!(panics.status, "failed", "a panic is retried, not fatal");
    assert!(panics.last_error.unwrap_or_default().contains("boom"));
}

/// Poll until `id` reaches `want` (the worker runs in the background).
async fn wait_for(pool: &PgPool, id: Uuid, want: &str) {
    for _ in 0..200 {
        if status(pool, id).await.status == want {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("job {id} never reached {want}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn running_worker_wakes_on_notify_and_drains_on_shutdown(pool: PgPool) {
    let ctx = Ctx::default();
    let config = WorkerConfig {
        concurrency: 2,
        // Long poll: only LISTEN/NOTIFY can make the first job start quickly.
        poll_interval: Duration::from_secs(60),
        shutdown_grace: Duration::from_secs(10),
        ..WorkerConfig::default()
    };
    let worker = Worker::new(pool.clone(), ctx.clone(), registry(), config);
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(worker.run(async {
        let _ = stopped.await;
    }));
    // Let the first (empty) claim and LISTEN happen before enqueueing.
    tokio::time::sleep(Duration::from_millis(300)).await;

    let quick = put(&pool, &Echo { n: 1 }).await.expect("id");
    wait_for(&pool, quick, "succeeded").await;

    let slow = put(&pool, &Slow { ms: 500 }).await.expect("id");
    wait_for(&pool, slow, "running").await;
    let _ = stop.send(());
    handle.await.expect("join").expect("worker");
    assert_eq!(
        status(&pool, slow).await.status,
        "succeeded",
        "in-flight job finished"
    );
    assert_eq!(ctx.runs.load(Ordering::SeqCst), 2);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn jobs_past_the_grace_period_are_handed_back(pool: PgPool) {
    let ctx = Ctx::default();
    let config = WorkerConfig {
        poll_interval: Duration::from_millis(50),
        shutdown_grace: Duration::from_millis(100),
        ..WorkerConfig::default()
    };
    let worker = Worker::new(pool.clone(), ctx.clone(), registry(), config);
    let id = put(&pool, &Slow { ms: 60_000 }).await.expect("id");
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(worker.run(async {
        let _ = stopped.await;
    }));
    wait_for(&pool, id, "running").await;
    let _ = stop.send(());
    handle.await.expect("join").expect("worker");
    let info = status(&pool, id).await;
    assert_eq!((info.status.as_str(), info.attempts), ("failed", 0));
    assert!(info.run_at <= chrono::Utc::now(), "claimable again at once");
}
