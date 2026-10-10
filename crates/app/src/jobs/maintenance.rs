//! Periodic housekeeping.

use std::time::Duration;

use akasha_jobs::{JobError, queue};

use super::{
    JobContext,
    kinds::{PruneActivity, PruneJobs, PruneSessions, PruneStaging},
};

/// Staged uploads older than this are abandoned. Must exceed the upload request
/// timeout (1 h, `routes.rs`) so an upload still in progress is never pruned.
const STAGING_MAX_AGE: Duration = Duration::from_secs(2 * 60 * 60);
/// Keep succeeded jobs this long (debugging), dead ones longer (inspection).
const KEEP_SUCCEEDED: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const KEEP_DEAD: Duration = Duration::from_secs(30 * 24 * 60 * 60);

pub async fn prune_sessions(ctx: JobContext, _job: PruneSessions) -> Result<(), JobError> {
    let n = akasha_db::sessions::delete_expired(&ctx.db).await?;
    if n > 0 {
        tracing::info!(count = n, "pruned expired sessions");
    }
    Ok(())
}

pub async fn prune_staging(ctx: JobContext, _job: PruneStaging) -> Result<(), JobError> {
    let n = ctx.storage.prune_staging(STAGING_MAX_AGE).await?;
    if n > 0 {
        tracing::info!(count = n, "pruned abandoned staged uploads");
    }
    Ok(())
}

pub async fn prune_jobs(ctx: JobContext, _job: PruneJobs) -> Result<(), JobError> {
    let n = queue::prune_finished(&ctx.db, KEEP_SUCCEEDED, KEEP_DEAD).await?;
    if n > 0 {
        tracing::info!(count = n, "pruned finished jobs");
    }
    Ok(())
}

/// Idempotent: deletes whatever is older than the retention period right now.
pub async fn prune_activity(ctx: JobContext, _job: PruneActivity) -> Result<(), JobError> {
    let days = ctx.config.activity_retention_days;
    if days == 0 {
        return Ok(());
    }
    let n = akasha_db::activity::prune(&ctx.db, days).await?;
    if n > 0 {
        tracing::info!(count = n, days, "pruned old activity events");
    }
    Ok(())
}
