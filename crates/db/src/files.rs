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
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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

/// Filters for [`list`]. `None` / empty means "any".
#[derive(Debug, Default, Clone)]
pub struct ListFilter {
    pub status: Option<String>,
    pub pinned: Option<bool>,
    pub tag: Option<String>,
    /// `LIKE` patterns on `mime_type`; a file matches if any pattern does.
    pub mime_patterns: Vec<String>,
    /// Keyset cursor: only rows strictly older than `(created_at, id)`.
    pub before: Option<(DateTime<Utc>, Uuid)>,
    pub limit: i64,
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

pub async fn get(pool: &PgPool, owner_id: Uuid, id: Uuid) -> Result<Option<File>, sqlx::Error> {
    sqlx::query_as!(
        File,
        "SELECT * FROM files WHERE owner_id = $1 AND id = $2",
        owner_id,
        id
    )
    .fetch_optional(pool)
    .await
}

/// Newest first, keyset-paginated on `(created_at, id)`.
pub async fn list(
    pool: &PgPool,
    owner_id: Uuid,
    filter: &ListFilter,
) -> Result<Vec<File>, sqlx::Error> {
    let (before_ts, before_id) = filter.before.unzip();
    sqlx::query_as!(
        File,
        r#"SELECT * FROM files
           WHERE owner_id = $1
             AND ($2::text IS NULL OR status = $2)
             AND ($3::bool IS NULL OR is_pinned = $3)
             AND ($4::text IS NULL OR tags @> ARRAY[$4::text])
             AND (cardinality($5::text[]) = 0 OR mime_type LIKE ANY($5))
             AND ($6::timestamptz IS NULL OR (created_at, id) < ($6, $7::uuid))
           ORDER BY created_at DESC, id DESC
           LIMIT $8"#,
        owner_id,
        filter.status,
        filter.pinned,
        filter.tag,
        &filter.mime_patterns,
        before_ts,
        before_id,
        filter.limit,
    )
    .fetch_all(pool)
    .await
}

/// Change any of name, pin and tags; `None` leaves a field as it is.
pub async fn update(
    pool: &PgPool,
    owner_id: Uuid,
    id: Uuid,
    original_name: Option<&str>,
    is_pinned: Option<bool>,
    tags: Option<&[String]>,
) -> Result<Option<File>, sqlx::Error> {
    sqlx::query_as!(
        File,
        r#"UPDATE files SET
               original_name = COALESCE($3, original_name),
               is_pinned = COALESCE($4, is_pinned),
               tags = COALESCE($5, tags)
           WHERE owner_id = $1 AND id = $2
           RETURNING *"#,
        owner_id,
        id,
        original_name,
        is_pinned,
        tags,
    )
    .fetch_optional(pool)
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

/// Delete several of the owner's files. Returns `(id, content_hash)` of each one
/// that existed.
pub async fn delete_many(
    conn: &mut PgConnection,
    owner_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
    let rows = sqlx::query!(
        "DELETE FROM files WHERE owner_id = $1 AND id = ANY($2) RETURNING id, content_hash",
        owner_id,
        ids
    )
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.content_hash)).collect())
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

mod refs;
pub use refs::{hashes_owned_by, is_referenced, lock_hash, unreferenced};

#[cfg(test)]
mod tests;
