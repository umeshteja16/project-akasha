//! Uploaded files and blob reference counting.
//!
//! Every query takes the owner's id: a file that belongs to someone else is simply
//! not found.
//!
//! Blobs are shared by content hash across users. Writers that make a blob visible
//! (upload) and writers that may delete it (file delete) both take
//! [`lock_hash`] inside their transaction first, so "is this hash still referenced?"
//! and "store the blob + insert the row" can never interleave.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct File {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub original_name: String,
    pub content_hash: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub status: String,
    pub error: Option<String>,
    pub is_pinned: bool,
    /// The user's own tags.
    pub tags: Vec<String>,
    /// Model-written description (see [`crate::enrichment`]).
    pub summary: Option<String>,
    /// Model-suggested tags, kept apart from the user's.
    pub auto_tags: Vec<String>,
    /// `done`, `skipped` or `failed`; `None`: never enriched.
    pub enrichment_status: Option<String>,
    pub enrichment_model: Option<String>,
    /// `file_extractions.created_at` of the text that was described.
    pub enriched_from: Option<DateTime<Utc>>,
    pub enriched_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Last time the owner opened the file's page (see [`mark_opened`]).
    pub last_opened_at: Option<DateTime<Utc>>,
    pub open_count: i32,
}

pub struct NewFile<'a> {
    pub owner_id: Uuid,
    pub original_name: &'a str,
    pub content_hash: &'a str,
    pub mime_type: &'a str,
    pub size_bytes: i64,
}

/// A user's storage quota (`None` = unlimited) and current usage in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    pub quota: Option<i64>,
    pub used: i64,
}

impl Usage {
    /// Bytes still available, `None` when unlimited.
    pub fn remaining(&self) -> Option<i64> {
        self.quota.map(|q| (q - self.used).max(0))
    }
}

/// Quota and usage. With `lock`, the user row is locked until the transaction ends,
/// so concurrent uploads by one user cannot both squeeze under the quota.
/// `None` if the user no longer exists.
pub async fn usage(
    conn: &mut PgConnection,
    owner_id: Uuid,
    lock: bool,
) -> Result<Option<Usage>, sqlx::Error> {
    let quota = if lock {
        sqlx::query_scalar!(
            "SELECT storage_quota_bytes FROM users WHERE id = $1 FOR UPDATE",
            owner_id
        )
        .fetch_optional(&mut *conn)
        .await?
    } else {
        sqlx::query_scalar!(
            "SELECT storage_quota_bytes FROM users WHERE id = $1",
            owner_id
        )
        .fetch_optional(&mut *conn)
        .await?
    };
    let Some(quota) = quota else {
        return Ok(None);
    };
    let used = sqlx::query_scalar!(
        r#"SELECT COALESCE(SUM(size_bytes), 0)::bigint AS "used!" FROM files WHERE owner_id = $1"#,
        owner_id
    )
    .fetch_one(conn)
    .await?;
    Ok(Some(Usage { quota, used }))
}

pub async fn set_quota(
    pool: &PgPool,
    owner_id: Uuid,
    quota: Option<i64>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE users SET storage_quota_bytes = $2 WHERE id = $1",
        owner_id,
        quota
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn find_by_hash(
    conn: &mut PgConnection,
    owner_id: Uuid,
    content_hash: &str,
) -> Result<Option<File>, sqlx::Error> {
    sqlx::query_as!(
        File,
        "SELECT * FROM files WHERE owner_id = $1 AND content_hash = $2",
        owner_id,
        content_hash,
    )
    .fetch_optional(conn)
    .await
}

/// Insert a new file. `None` if the owner already has a file with this hash.
pub async fn insert(
    conn: &mut PgConnection,
    new: &NewFile<'_>,
) -> Result<Option<File>, sqlx::Error> {
    sqlx::query_as!(
        File,
        r#"INSERT INTO files (owner_id, original_name, content_hash, mime_type, size_bytes)
           VALUES ($1, $2, $3, $4, $5)
           ON CONFLICT (owner_id, content_hash) DO NOTHING
           RETURNING *"#,
        new.owner_id,
        new.original_name,
        new.content_hash,
        new.mime_type,
        new.size_bytes,
    )
    .fetch_optional(conn)
    .await
}

pub async fn get<'e>(
    db: impl sqlx::PgExecutor<'e>,
    owner_id: Uuid,
    id: Uuid,
) -> Result<Option<File>, sqlx::Error> {
    sqlx::query_as!(
        File,
        "SELECT * FROM files WHERE owner_id = $1 AND id = $2",
        owner_id,
        id
    )
    .fetch_optional(db)
    .await
}

/// What [`update`] changes; `None` leaves a field as it is.
#[derive(Debug, Default, Clone, Copy)]
pub struct FileChanges<'a> {
    pub original_name: Option<&'a str>,
    pub is_pinned: Option<bool>,
    pub tags: Option<&'a [String]>,
    /// The user may drop (or edit) model-suggested tags.
    pub auto_tags: Option<&'a [String]>,
}

/// Change any of name, pin, tags and suggested tags.
pub async fn update<'e>(
    db: impl sqlx::PgExecutor<'e>,
    owner_id: Uuid,
    id: Uuid,
    changes: FileChanges<'_>,
) -> Result<Option<File>, sqlx::Error> {
    sqlx::query_as!(
        File,
        r#"UPDATE files SET
               original_name = COALESCE($3, original_name),
               is_pinned = COALESCE($4, is_pinned),
               tags = COALESCE($5, tags),
               auto_tags = COALESCE($6, auto_tags)
           WHERE owner_id = $1 AND id = $2
           RETURNING *"#,
        owner_id,
        id,
        changes.original_name,
        changes.is_pinned,
        changes.tags,
        changes.auto_tags,
    )
    .fetch_optional(db)
    .await
}

/// The content hash of one of the owner's files (for locking before [`delete`]).
pub async fn hash_of(
    conn: &mut PgConnection,
    owner_id: Uuid,
    id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT content_hash FROM files WHERE owner_id = $1 AND id = $2",
        owner_id,
        id
    )
    .fetch_optional(conn)
    .await
}

/// Delete one of the owner's files. Returns its content hash if it existed.
/// The blob is not touched: enqueue a blob-release job in the same transaction.
pub async fn delete(
    conn: &mut PgConnection,
    owner_id: Uuid,
    id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "DELETE FROM files WHERE owner_id = $1 AND id = $2 RETURNING content_hash",
        owner_id,
        id
    )
    .fetch_optional(conn)
    .await
}

/// A deleted file row (see [`delete_many`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deleted {
    pub id: Uuid,
    pub content_hash: String,
    pub original_name: String,
}

/// Delete several of the owner's files. Returns each one that existed.
pub async fn delete_many(
    conn: &mut PgConnection,
    owner_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<Deleted>, sqlx::Error> {
    sqlx::query_as!(
        Deleted,
        "DELETE FROM files WHERE owner_id = $1 AND id = ANY($2)
         RETURNING id, content_hash, original_name",
        owner_id,
        ids
    )
    .fetch_all(conn)
    .await
}

/// Record that the owner opened a file: `last_opened_at = now()` and one more
/// `open_count` (without touching `updated_at`). `None`: no such file.
pub async fn mark_opened(
    conn: &mut PgConnection,
    owner_id: Uuid,
    id: Uuid,
) -> Result<Option<File>, sqlx::Error> {
    sqlx::query_as!(
        File,
        r#"UPDATE files SET last_opened_at = now(), open_count = open_count + 1
           WHERE owner_id = $1 AND id = $2
           RETURNING *"#,
        owner_id,
        id
    )
    .fetch_optional(conn)
    .await
}

/// Put a file back to `pending` (before re-running extraction).
pub async fn mark_pending(
    conn: &mut PgConnection,
    owner_id: Uuid,
    id: Uuid,
) -> Result<Option<File>, sqlx::Error> {
    sqlx::query_as!(
        File,
        r#"UPDATE files SET status = 'pending', error = NULL
           WHERE owner_id = $1 AND id = $2
           RETURNING *"#,
        owner_id,
        id
    )
    .fetch_optional(conn)
    .await
}

mod list;
mod refs;
pub use list::{ListFilter, ListKey, ListOrder, TagCount, list, tag_counts};
pub use refs::{hashes_owned_by, is_referenced, lock_hash, unreferenced};

#[cfg(test)]
mod tests;
