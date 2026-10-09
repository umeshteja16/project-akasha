//! SQL for the `jobs` table. Every state change is guarded so a worker can only move
//! a job it still owns (`status = 'running' AND locked_by = me AND attempts = n`):
//! a job reclaimed from a slow worker cannot be completed twice.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::{Job, QueueError, backoff};

/// A job a worker has claimed and now owns.
#[derive(Debug, Clone)]
pub struct ClaimedJob {
    pub id: Uuid,
    pub kind: String,
    pub payload: Value,
    /// Including this one.
    pub attempts: i32,
    pub max_attempts: i32,
}

/// A job's visible state (for status displays and tests).
#[derive(Debug, Clone, PartialEq)]
pub struct JobInfo {
    pub id: Uuid,
    pub kind: String,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub run_at: DateTime<Utc>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Where a failed job went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failed {
    /// Will be retried after the backoff delay.
    Retrying,
    Dead,
    /// The job was no longer ours (reclaimed); nothing changed.
    Lost,
}

/// Enqueue `job` on `conn`. Pass a transaction (`&mut *tx`) to make the job atomic
/// with the change that needs it. Returns `None` when an identical job (same kind
/// and [`Job::dedupe_key`]) is already queued.
pub async fn enqueue<J: Job>(conn: &mut PgConnection, job: &J) -> Result<Option<Uuid>, QueueError> {
    enqueue_delayed(conn, job, Duration::ZERO).await
}

/// [`enqueue`], not claimable until `delay` has passed.
pub async fn enqueue_delayed<J: Job>(
    conn: &mut PgConnection,
    job: &J,
    delay: Duration,
) -> Result<Option<Uuid>, QueueError> {
    let payload = serde_json::to_value(job)?;
    let id = sqlx::query_scalar!(
        r#"INSERT INTO jobs (kind, payload, max_attempts, dedupe_key, run_at)
           VALUES ($1, $2, $3, $4, now() + make_interval(secs => $5))
           ON CONFLICT (kind, dedupe_key) WHERE dedupe_key IS NOT NULL AND status = 'queued'
           DO NOTHING
           RETURNING id"#,
        J::KIND,
        payload,
        J::MAX_ATTEMPTS,
        job.dedupe_key(),
        delay.as_secs_f64(),
    )
    .fetch_optional(conn)
    .await?;
    if id.is_none() {
        tracing::debug!(kind = J::KIND, "job already queued; deduplicated");
    }
    Ok(id)
}

/// Claim up to `limit` due jobs of the given kinds for `worker`.
pub async fn claim(
    pool: &PgPool,
    worker: &str,
    kinds: &[String],
    limit: i64,
) -> Result<Vec<ClaimedJob>, sqlx::Error> {
    sqlx::query_as!(
        ClaimedJob,
        r#"WITH next AS (
               SELECT id FROM jobs
               WHERE status IN ('queued', 'failed') AND run_at <= now() AND kind = ANY($2)
               ORDER BY run_at
               LIMIT $3
               FOR UPDATE SKIP LOCKED
           )
           UPDATE jobs j
           SET status = 'running', attempts = j.attempts + 1, locked_at = now(), locked_by = $1
           FROM next WHERE j.id = next.id
           RETURNING j.id, j.kind, j.payload, j.attempts, j.max_attempts"#,
        worker,
        kinds,
        limit,
    )
    .fetch_all(pool)
    .await
}

/// Mark a claimed job done. `false` if it was no longer ours.
pub async fn complete(pool: &PgPool, worker: &str, job: &ClaimedJob) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        r#"UPDATE jobs
           SET status = 'succeeded', locked_at = NULL, locked_by = NULL,
               last_error = NULL, finished_at = now()
           WHERE id = $1 AND status = 'running' AND locked_by = $2 AND attempts = $3"#,
        job.id,
        worker,
        job.attempts,
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Record a failed attempt. Retries (with backoff) while `retry` is set and attempts
/// remain; otherwise the job is dead.
pub async fn fail(
    pool: &PgPool,
    worker: &str,
    job: &ClaimedJob,
    error: &str,
    retry: bool,
) -> Result<Failed, sqlx::Error> {
    let delay = backoff(job.attempts, backoff::random_unit());
    let status = sqlx::query_scalar!(
        r#"UPDATE jobs
           SET status = CASE WHEN $4 AND attempts < max_attempts THEN 'failed' ELSE 'dead' END,
               run_at = CASE WHEN $4 AND attempts < max_attempts
                             THEN now() + make_interval(secs => $5) ELSE run_at END,
               finished_at = CASE WHEN $4 AND attempts < max_attempts THEN NULL ELSE now() END,
               locked_at = NULL, locked_by = NULL, last_error = $6
           WHERE id = $1 AND status = 'running' AND locked_by = $2 AND attempts = $3
           RETURNING status"#,
        job.id,
        worker,
        job.attempts,
        retry,
        delay.as_secs_f64(),
        truncate(error),
    )
    .fetch_optional(pool)
    .await?;
    Ok(match status.as_deref() {
        Some("failed") => Failed::Retrying,
        Some(_) => Failed::Dead,
        None => Failed::Lost,
    })
}

/// Hand back a job that was interrupted (worker shutdown) without counting the
/// attempt. It is claimable again immediately.
pub async fn release(pool: &PgPool, worker: &str, job: &ClaimedJob) -> Result<bool, sqlx::Error> {
    let released = sqlx::query!(
        r#"UPDATE jobs
           SET status = 'failed', attempts = attempts - 1, run_at = now(),
               locked_at = NULL, locked_by = NULL,
               last_error = 'interrupted by worker shutdown'
           WHERE id = $1 AND status = 'running' AND locked_by = $2 AND attempts = $3"#,
        job.id,
        worker,
        job.attempts,
    )
    .execute(pool)
    .await?;
    Ok(released.rows_affected() == 1)
}

/// Refresh `locked_at` on the jobs `worker` is running, so they are not reclaimed.
pub async fn heartbeat(pool: &PgPool, worker: &str, ids: &[Uuid]) -> Result<u64, sqlx::Error> {
    if ids.is_empty() {
        return Ok(0);
    }
    let beat = sqlx::query!(
        r#"UPDATE jobs SET locked_at = now()
           WHERE id = ANY($1) AND status = 'running' AND locked_by = $2"#,
        ids,
        worker,
    )
    .execute(pool)
    .await?;
    Ok(beat.rows_affected())
}

/// Put back jobs whose worker stopped heartbeating for longer than `timeout`
/// (crashed or partitioned). The lost run counts as an attempt.
pub async fn reap_stale(pool: &PgPool, timeout: Duration) -> Result<u64, sqlx::Error> {
    let reaped = sqlx::query!(
        r#"UPDATE jobs
           SET status = CASE WHEN attempts < max_attempts THEN 'failed' ELSE 'dead' END,
               finished_at = CASE WHEN attempts < max_attempts THEN NULL ELSE now() END,
               run_at = now(), locked_at = NULL, locked_by = NULL,
               last_error = 'worker stopped responding (visibility timeout)'
           WHERE status = 'running' AND locked_at < now() - make_interval(secs => $1)"#,
        timeout.as_secs_f64(),
    )
    .execute(pool)
    .await?;
    Ok(reaped.rows_affected())
}

/// Delete finished jobs: succeeded ones older than `succeeded`, dead ones older than
/// `dead` (kept longer for inspection).
pub async fn prune_finished(
    pool: &PgPool,
    succeeded: Duration,
    dead: Duration,
) -> Result<u64, sqlx::Error> {
    let pruned = sqlx::query!(
        r#"DELETE FROM jobs
           WHERE (status = 'succeeded' AND finished_at < now() - make_interval(secs => $1))
              OR (status = 'dead' AND finished_at < now() - make_interval(secs => $2))"#,
        succeeded.as_secs_f64(),
        dead.as_secs_f64(),
    )
    .execute(pool)
    .await?;
    Ok(pruned.rows_affected())
}

/// The most recent job of `kind` with `dedupe_key`.
pub async fn latest_by_key(
    pool: &PgPool,
    kind: &str,
    dedupe_key: &str,
) -> Result<Option<JobInfo>, sqlx::Error> {
    sqlx::query_as!(
        JobInfo,
        r#"SELECT id, kind, status, attempts, max_attempts, run_at, last_error,
                  created_at, updated_at
           FROM jobs WHERE kind = $1 AND dedupe_key = $2
           ORDER BY created_at DESC LIMIT 1"#,
        kind,
        dedupe_key,
    )
    .fetch_optional(pool)
    .await
}

pub async fn get(pool: &PgPool, id: Uuid) -> Result<Option<JobInfo>, sqlx::Error> {
    sqlx::query_as!(
        JobInfo,
        r#"SELECT id, kind, status, attempts, max_attempts, run_at, last_error,
                  created_at, updated_at
           FROM jobs WHERE id = $1"#,
        id,
    )
    .fetch_optional(pool)
    .await
}

/// Waiting jobs whose kind is not in `known`, counted per kind.
pub async fn unhandled_kinds(
    pool: &PgPool,
    known: &[String],
) -> Result<Vec<(String, i64)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT kind, count(*) AS "count!" FROM jobs
           WHERE status IN ('queued', 'failed') AND kind <> ALL($1)
           GROUP BY kind ORDER BY kind"#,
        known,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| (r.kind, r.count)).collect())
}

/// Error messages are stored for operators; keep them bounded.
fn truncate(error: &str) -> String {
    const MAX: usize = 2000;
    match error.char_indices().nth(MAX) {
        Some((cut, _)) => format!("{}…", &error[..cut]),
        None => error.to_owned(),
    }
}
