//! Periodic jobs, coordinated through the `job_schedules` table so any number of
//! workers enqueue each tick exactly once.

use std::time::Duration;

use serde_json::Value;
use sqlx::PgPool;

use crate::{Job, QueueError};

/// "Enqueue this job every `every`."
#[derive(Debug, Clone)]
pub struct Schedule {
    pub name: &'static str,
    kind: &'static str,
    payload: Value,
    every: Duration,
}

impl Schedule {
    pub fn new<J: Job>(name: &'static str, every: Duration, job: &J) -> Result<Self, QueueError> {
        Ok(Self {
            name,
            kind: J::KIND,
            payload: serde_json::to_value(job)?,
            every,
        })
    }

    fn interval_secs(&self) -> i32 {
        i32::try_from(self.every.as_secs().max(1)).unwrap_or(i32::MAX)
    }
}

/// Create or update the schedules (keeps `next_run_at` of existing ones, so a
/// restart does not re-run everything). New schedules are due immediately.
pub async fn upsert(pool: &PgPool, schedules: &[Schedule]) -> Result<(), sqlx::Error> {
    for s in schedules {
        sqlx::query!(
            r#"INSERT INTO job_schedules (name, kind, payload, interval_secs)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (name) DO UPDATE
               SET kind = EXCLUDED.kind, payload = EXCLUDED.payload,
                   interval_secs = EXCLUDED.interval_secs,
                   next_run_at = LEAST(job_schedules.next_run_at,
                                       now() + make_interval(secs => EXCLUDED.interval_secs))"#,
            s.name,
            s.kind,
            s.payload,
            s.interval_secs(),
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Enqueue every due schedule among `names` and advance it. Atomic: the advance and
/// the enqueue commit together, and the conditional update means concurrent
/// workers cannot both fire one tick. Returns how many jobs were enqueued.
pub async fn tick(pool: &PgPool, names: &[String]) -> Result<u64, sqlx::Error> {
    let fired = sqlx::query!(
        r#"WITH due AS (
               UPDATE job_schedules
               SET next_run_at = now() + make_interval(secs => interval_secs)
               WHERE next_run_at <= now() AND name = ANY($1)
               RETURNING name, kind, payload
           )
           INSERT INTO jobs (kind, payload, dedupe_key)
           SELECT kind, payload, 'schedule:' || name FROM due
           ON CONFLICT (kind, dedupe_key) WHERE dedupe_key IS NOT NULL AND status = 'queued'
           DO NOTHING"#,
        names,
    )
    .execute(pool)
    .await?;
    Ok(fired.rows_affected())
}
