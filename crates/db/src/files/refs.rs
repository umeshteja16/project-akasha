//! Blob reference counting: which content hashes are still used, and the per-hash
//! lock that serialises everything that creates or deletes a blob.

use sqlx::PgConnection;
use uuid::Uuid;

/// Serialise every writer of one content hash until the transaction ends.
pub async fn lock_hash(conn: &mut PgConnection, content_hash: &str) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"SELECT 1 AS "locked!" FROM (
               SELECT pg_advisory_xact_lock(('x' || substr($1, 1, 16))::bit(64)::bigint)
           ) AS l"#,
        content_hash,
    )
    .fetch_one(conn)
    .await?;
    Ok(())
}

/// Does any file (of any user) still use this blob?
pub async fn is_referenced(
    conn: &mut PgConnection,
    content_hash: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM files WHERE content_hash = $1) AS "exists!""#,
        content_hash
    )
    .fetch_one(conn)
    .await
}

/// Content hashes of every file the user owns (to clean up blobs on account deletion).
pub async fn hashes_owned_by(
    conn: &mut PgConnection,
    owner_id: Uuid,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT DISTINCT content_hash FROM files WHERE owner_id = $1",
        owner_id
    )
    .fetch_all(conn)
    .await
}

/// The subset of `hashes` that no file references (orphan-blob candidates).
pub async fn unreferenced(
    conn: &mut PgConnection,
    hashes: &[String],
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT h AS "h!" FROM unnest($1::text[]) AS h
           WHERE NOT EXISTS (SELECT 1 FROM files WHERE content_hash = h)"#,
        hashes
    )
    .fetch_all(conn)
    .await
}
