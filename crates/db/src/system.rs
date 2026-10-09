//! Server-wide facts for the system status page (no user data).

use chrono::{DateTime, Utc};
use sqlx::PgPool;

/// Job queue depth and activity.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueStats {
    /// Claimable now (queued, or failed and due for a retry).
    pub ready: i64,
    pub running: i64,
    /// Running, but without a heartbeat for longer than the visibility timeout.
    pub stale: i64,
    /// Failed and waiting for their retry time.
    pub retrying: i64,
    /// Given up on in the last 24 hours.
    pub dead_recent: i64,
    /// How long the oldest claimable job has waited, in seconds.
    pub oldest_ready_secs: Option<f64>,
    pub last_finished_at: Option<DateTime<Utc>>,
}

pub async fn queue_stats(pool: &PgPool, stale_after_secs: f64) -> Result<QueueStats, sqlx::Error> {
    sqlx::query_as!(
        QueueStats,
        r#"SELECT
             count(*) FILTER (WHERE status IN ('queued', 'failed') AND run_at <= now()) AS "ready!",
             count(*) FILTER (WHERE status = 'running') AS "running!",
             count(*) FILTER (WHERE status = 'running'
                              AND locked_at < now() - make_interval(secs => $1)) AS "stale!",
             count(*) FILTER (WHERE status = 'failed' AND run_at > now()) AS "retrying!",
             count(*) FILTER (WHERE status = 'dead'
                              AND finished_at > now() - interval '24 hours') AS "dead_recent!",
             EXTRACT(EPOCH FROM now() - min(run_at) FILTER (
                 WHERE status IN ('queued', 'failed') AND run_at <= now()))::float8
                 AS oldest_ready_secs,
             max(finished_at) AS last_finished_at
           FROM jobs"#,
        stale_after_secs
    )
    .fetch_one(pool)
    .await
}
