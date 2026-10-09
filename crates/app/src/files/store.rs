//! Keeping `files` rows and blobs consistent.
//!
//! Blobs are shared by content hash, across users.
//!
//! - **save**: takes the per-hash advisory lock ([`files::lock_hash`]), moves the
//!   staged bytes to their content-addressed key, inserts the row and enqueues the
//!   extraction job, all in one transaction.
//! - **delete** (single, bulk, account): deletes the rows and enqueues a
//!   `delete_blob_if_unreferenced` job per hash *in the same transaction*. Storage is
//!   never touched before the commit, so a failed commit cannot lose bytes that a
//!   surviving row still points at.
//! - the **job** ([`crate::jobs::blobs::delete_if_unreferenced`]) takes the hash
//!   lock, re-checks that no row references the hash, and only then deletes the blob.
//!
//! An upload racing the job either commits first (the job sees the row and keeps the
//! blob) or waits for the lock and re-creates the blob from its staged copy. Failure
//! modes only ever leak an unreferenced blob (collected by the daily orphan sweep),
//! never lose a referenced one.

use akasha_db::files::{self, File, NewFile};
use akasha_storage::FinishedBlob;
use uuid::Uuid;

use crate::{
    error::ApiError,
    jobs::{
        blobs,
        kinds::{ExtractFile, MakeThumbnail},
    },
    state::AppState,
};
use akasha_core::Error;

/// Result of [`save`].
pub enum Saved {
    Created(File),
    /// The user already had a file with these exact bytes; nothing was added.
    Existing(File),
}

/// Record a finished upload for `owner`, enforcing their storage quota.
pub async fn save(
    state: &AppState,
    owner: Uuid,
    blob: FinishedBlob,
    name: &str,
    mime: &str,
) -> Result<Saved, ApiError> {
    let hash = blob.hash().to_hex();
    let size =
        i64::try_from(blob.size()).map_err(|_| Error::payload_too_large("file too large"))?;

    let mut tx = state.db.begin().await?;
    files::lock_hash(&mut tx, &hash).await?;
    let Some(usage) = files::usage(&mut tx, owner, true).await? else {
        blob.discard().await;
        return Err(Error::unauthorized("account no longer exists").into());
    };
    if let Some(existing) = files::find_by_hash(&mut tx, owner, &hash).await? {
        blob.discard().await;
        tx.commit().await?;
        return Ok(Saved::Existing(existing));
    }
    if usage.remaining().is_some_and(|left| size > left) {
        blob.discard().await;
        return Err(Error::quota_exceeded("storage quota exceeded").into());
    }
    blob.commit().await.map_err(Error::from)?;
    let new = NewFile {
        owner_id: owner,
        original_name: name,
        content_hash: &hash,
        mime_type: mime,
        size_bytes: size,
    };
    let file = files::insert(&mut tx, &new)
        .await?
        .ok_or_else(|| Error::conflict("file was uploaded concurrently; retry"))?;
    akasha_jobs::enqueue(&mut tx, &ExtractFile { file_id: file.id }).await?;
    enqueue_thumbnail(&mut tx, &file).await?;
    tx.commit().await?;
    Ok(Saved::Created(file))
}

/// Queue a thumbnail for image files (a no-op for other types).
pub async fn enqueue_thumbnail(
    conn: &mut sqlx::PgConnection,
    file: &File,
) -> Result<(), akasha_jobs::QueueError> {
    if file.mime_type.starts_with("image/") {
        akasha_jobs::enqueue(
            conn,
            &MakeThumbnail {
                hash: file.content_hash.clone(),
            },
        )
        .await?;
    }
    Ok(())
}

/// Delete one of `owner`'s files. Returns `false` if there was no such file.
pub async fn delete(state: &AppState, owner: Uuid, id: Uuid) -> Result<bool, ApiError> {
    Ok(!delete_many(state, owner, &[id]).await?.is_empty())
}

/// Delete several of `owner`'s files in one transaction; blobs are released by a
/// job enqueued in that transaction. Returns the ids that existed, in request order
/// without duplicates.
pub async fn delete_many(
    state: &AppState,
    owner: Uuid,
    ids: &[Uuid],
) -> Result<Vec<Uuid>, ApiError> {
    let mut tx = state.db.begin().await?;
    let rows = files::delete_many(&mut tx, owner, ids).await?;
    let hashes: Vec<String> = rows.iter().map(|(_, hash)| hash.clone()).collect();
    blobs::release(&mut tx, &hashes).await?;
    tx.commit().await?;
    let mut deleted: Vec<Uuid> = Vec::with_capacity(rows.len());
    for id in ids {
        if !deleted.contains(id) && rows.iter().any(|(row, _)| row == id) {
            deleted.push(*id);
        }
    }
    Ok(deleted)
}
