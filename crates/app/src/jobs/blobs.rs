//! Blob garbage collection.

use akasha_db::files;
use akasha_jobs::{JobError, enqueue};
use akasha_storage::ContentHash;
use futures_util::StreamExt;
use sqlx::{PgConnection, PgPool};

use super::{JobContext, kinds::DeleteBlobIfUnreferenced};

/// Hashes checked per database round trip in the orphan sweep.
const SWEEP_BATCH: usize = 500;

/// Enqueue a [`DeleteBlobIfUnreferenced`] for each distinct hash, on `conn` (the
/// transaction that removed the rows).
pub async fn release(
    conn: &mut PgConnection,
    hashes: &[String],
) -> Result<(), akasha_jobs::QueueError> {
    let mut seen: Vec<&String> = Vec::new();
    for hash in hashes {
        if !seen.contains(&hash) {
            seen.push(hash);
            enqueue(&mut *conn, &DeleteBlobIfUnreferenced { hash: hash.clone() }).await?;
        }
    }
    Ok(())
}

/// Delete the blob (and its thumbnails) unless a file references it. Holds the
/// per-hash lock across the check and the delete, so an upload of the same bytes either finishes first (and
/// the blob stays) or waits and then re-creates the blob from its staged copy.
pub async fn delete_if_unreferenced(
    ctx: JobContext,
    job: DeleteBlobIfUnreferenced,
) -> Result<(), JobError> {
    let hash: ContentHash = job.hash.parse().map_err(JobError::permanent)?;
    let mut tx = ctx.db.begin().await?;
    files::lock_hash(&mut tx, &job.hash).await?;
    if files::is_referenced(&mut tx, &job.hash).await? {
        tracing::debug!(hash = %job.hash, "blob still referenced; kept");
        return Ok(());
    }
    ctx.storage.delete_thumbnails(&hash).await?;
    ctx.storage.delete(&hash).await?;
    tx.commit().await?;
    tracing::info!(hash = %job.hash, "deleted unreferenced blob");
    Ok(())
}

/// List every stored blob and queue the unreferenced ones for deletion.
pub async fn sweep_orphans(
    ctx: JobContext,
    _job: super::kinds::SweepOrphanBlobs,
) -> Result<(), JobError> {
    let mut blobs = ctx.storage.list_blobs();
    let mut batch = Vec::with_capacity(SWEEP_BATCH);
    let mut queued = 0;
    while let Some(hash) = blobs.next().await {
        batch.push(hash?.to_hex());
        if batch.len() == SWEEP_BATCH {
            queued += queue_orphans(&ctx.db, &batch).await?;
            batch.clear();
        }
    }
    if !batch.is_empty() {
        queued += queue_orphans(&ctx.db, &batch).await?;
    }
    if queued > 0 {
        tracing::info!(count = queued, "queued orphan blobs for deletion");
    }
    Ok(())
}

async fn queue_orphans(db: &PgPool, hashes: &[String]) -> Result<usize, JobError> {
    let mut tx = db.begin().await?;
    let orphans = files::unreferenced(&mut tx, hashes).await?;
    release(&mut tx, &orphans).await?;
    tx.commit().await?;
    Ok(orphans.len())
}
