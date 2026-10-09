//! Keeping `files` rows and blobs consistent.
//!
//! Blobs are shared by content hash, across users. Both paths that touch a blob's
//! existence run inside one transaction holding the per-hash advisory lock
//! ([`files::lock_hash`]):
//!
//! - **save**: lock → move the staged bytes to their content-addressed key → insert
//!   the row → commit.
//! - **delete**: lock → delete the row → if no row references the hash any more,
//!   delete the blob → commit.
//!
//! So a delete can never remove a blob that a concurrent upload has just pointed a
//! row at: whichever transaction takes the lock second sees the other's result
//! (a deleted blob is re-created from the upload's staged copy; a new row keeps the
//! blob alive). Failure modes only ever leak an unreferenced blob, never lose a
//! referenced one, except if the commit itself fails after a blob was deleted
//! (then the delete is rolled back but the bytes are gone; downloads return 404).

use akasha_db::files::{self, File, NewFile};
use akasha_storage::{ContentHash, FinishedBlob, Storage};
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};
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
    tx.commit().await?;
    Ok(Saved::Created(file))
}

/// Delete one of `owner`'s files and its blob if nothing else uses it.
/// Returns `false` if there was no such file.
pub async fn delete(state: &AppState, owner: Uuid, id: Uuid) -> Result<bool, ApiError> {
    let mut tx = state.db.begin().await?;
    let Some(hash) = files::hash_of(&mut tx, owner, id).await? else {
        return Ok(false);
    };
    files::lock_hash(&mut tx, &hash).await?;
    if files::delete(&mut tx, owner, id).await?.is_none() {
        // Deleted concurrently.
        return Ok(false);
    }
    remove_blob_if_unreferenced(&mut tx, &state.storage, &hash).await?;
    tx.commit().await?;
    Ok(true)
}

/// Delete the blob for `hash` if no file references it (e.g. after the owning rows
/// were removed by a cascade).
pub async fn release(state: &AppState, hash: &str) -> Result<(), ApiError> {
    let mut tx = state.db.begin().await?;
    files::lock_hash(&mut tx, hash).await?;
    remove_blob_if_unreferenced(&mut tx, &state.storage, hash).await?;
    tx.commit().await?;
    Ok(())
}

async fn remove_blob_if_unreferenced(
    conn: &mut sqlx::PgConnection,
    storage: &Storage,
    hash: &str,
) -> Result<(), ApiError> {
    if files::is_referenced(conn, hash).await? {
        return Ok(());
    }
    let parsed: ContentHash = hash.parse().map_err(Error::from)?;
    // A failed blob delete only leaks bytes; it must not keep the row alive.
    if let Err(err) = storage.delete(&parsed).await {
        tracing::warn!(%err, %hash, "failed to delete unreferenced blob");
    }
    Ok(())
}
