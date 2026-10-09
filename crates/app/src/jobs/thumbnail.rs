//! The `make_thumbnail` handler: decode an image blob, scale it down and store
//! the result next to the blob (`thumbs/…/<hash>/<size>`).
//!
//! Idempotent: an existing thumbnail ends the job, and rendering the same bytes
//! again writes the same object. The write happens under the per-hash lock after
//! re-checking that a file still references the blob, so a thumbnail can never
//! outlive its blob: blob deletion (`delete_blob_if_unreferenced`) removes the
//! thumbnails under the same lock.

use akasha_db::files;
use akasha_ingest::thumbnail::{self, ThumbnailError};
use akasha_jobs::JobError;
use akasha_storage::{ContentHash, StorageError};
use bytes::Bytes;

use super::{JobContext, kinds::MakeThumbnail};

/// Longest side of the stored thumbnail, in pixels.
pub const THUMBNAIL_SIZE: u32 = 256;
/// Images larger than this (compressed) are not previewed.
const MAX_SOURCE_BYTES: usize = 64 * 1024 * 1024;

pub async fn make_thumbnail(ctx: JobContext, job: MakeThumbnail) -> Result<(), JobError> {
    let hash: ContentHash = job.hash.parse().map_err(JobError::permanent)?;
    if ctx
        .storage
        .get_thumbnail(&hash, THUMBNAIL_SIZE)
        .await?
        .is_some()
    {
        return Ok(());
    }
    let bytes = match ctx.storage.get_bytes(&hash).await {
        Ok(bytes) => bytes,
        // Deleted since the job was queued.
        Err(StorageError::NotFound) => return Ok(()),
        Err(err) => return Err(err.into()),
    };
    if bytes.len() > MAX_SOURCE_BYTES {
        tracing::info!(hash = %job.hash, size = bytes.len(), "image too large for a thumbnail");
        return Ok(());
    }
    let rendered =
        tokio::task::spawn_blocking(move || thumbnail::thumbnail(&bytes, THUMBNAIL_SIZE))
            .await
            .map_err(|e| JobError::retry(format!("thumbnail task failed: {e}")))?;
    let thumb = match rendered {
        Ok(thumb) => thumb,
        Err(err @ (ThumbnailError::Unsupported | ThumbnailError::TooLarge { .. })) => {
            tracing::info!(hash = %job.hash, %err, "no thumbnail");
            return Ok(());
        }
        Err(err) => return Err(JobError::permanent(err)),
    };

    let mut tx = ctx.db.begin().await?;
    files::lock_hash(&mut tx, &job.hash).await?;
    if !files::is_referenced(&mut tx, &job.hash).await? {
        tracing::debug!(hash = %job.hash, "blob released before its thumbnail was stored");
        return Ok(());
    }
    ctx.storage
        .put_thumbnail(&hash, THUMBNAIL_SIZE, Bytes::from(thumb.bytes))
        .await?;
    tx.commit().await?;
    tracing::info!(hash = %job.hash, width = thumb.width, height = thumb.height, "thumbnail stored");
    Ok(())
}
