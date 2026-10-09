//! Extracted text and chunks (`file_extractions`, `file_chunks`).
//!
//! The write side is used by the extraction job, which only knows a file id: those
//! functions are internal and must never be reachable from a request without an
//! ownership check. The read side ([`get`]) takes the owner like every request query.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool, types::JsonValue};
use uuid::Uuid;

/// Rows per `INSERT` when storing chunks.
const CHUNK_BATCH: usize = 1000;

/// What the extraction job needs to know about a file.
#[derive(Debug, Clone)]
pub struct Target {
    pub owner_id: Uuid,
    pub content_hash: String,
    pub mime_type: String,
}

/// The file to extract, or `None` if it was deleted. Internal: no owner check.
pub async fn target(pool: &PgPool, file_id: Uuid) -> Result<Option<Target>, sqlx::Error> {
    sqlx::query_as!(
        Target,
        "SELECT owner_id, content_hash, mime_type FROM files WHERE id = $1",
        file_id
    )
    .fetch_optional(pool)
    .await
}

/// Set `processing` and clear an old error. `false` if the file is gone.
pub async fn mark_processing(pool: &PgPool, file_id: Uuid) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE files SET status = 'processing', error = NULL WHERE id = $1",
        file_id
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Set `failed` with a message the owner will see.
pub async fn mark_failed(pool: &PgPool, file_id: Uuid, error: &str) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE files SET status = 'failed', error = $2 WHERE id = $1",
        file_id,
        error
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Set `ready` (inside the transaction that stored the extraction).
pub async fn mark_ready(conn: &mut PgConnection, file_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE files SET status = 'ready', error = NULL WHERE id = $1",
        file_id
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub struct NewExtraction<'a> {
    pub extractor: &'a str,
    pub extractor_version: &'a str,
    pub page_count: Option<i32>,
    pub char_count: i32,
    pub text: &'a str,
    pub pages: JsonValue,
    pub notes: &'a [String],
}

pub struct NewChunk<'a> {
    pub chunk_index: i32,
    pub page: Option<i32>,
    pub char_start: i32,
    pub char_end: i32,
    pub text: &'a str,
}

/// Replace the file's extraction and chunks (call inside a transaction). Locks the
/// file row first; returns `None` (and writes nothing) if the file is gone,
/// otherwise its owner, which the chunks are stored under.
pub async fn replace(
    conn: &mut PgConnection,
    file_id: Uuid,
    extraction: &NewExtraction<'_>,
    chunks: &[NewChunk<'_>],
) -> Result<Option<Uuid>, sqlx::Error> {
    let Some(owner_id) = sqlx::query_scalar!(
        "SELECT owner_id FROM files WHERE id = $1 FOR UPDATE",
        file_id
    )
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };

    sqlx::query!("DELETE FROM file_chunks WHERE file_id = $1", file_id)
        .execute(&mut *conn)
        .await?;
    for batch in chunks.chunks(CHUNK_BATCH) {
        let index: Vec<i32> = batch.iter().map(|c| c.chunk_index).collect();
        let page: Vec<Option<i32>> = batch.iter().map(|c| c.page).collect();
        let start: Vec<i32> = batch.iter().map(|c| c.char_start).collect();
        let end: Vec<i32> = batch.iter().map(|c| c.char_end).collect();
        let text: Vec<&str> = batch.iter().map(|c| c.text).collect();
        sqlx::query!(
            r#"INSERT INTO file_chunks (file_id, owner_id, chunk_index, page, char_start, char_end, text)
               SELECT $1, $2, * FROM UNNEST($3::int4[], $4::int4[], $5::int4[], $6::int4[], $7::text[])"#,
            file_id,
            owner_id,
            &index,
            &page as &[Option<i32>],
            &start,
            &end,
            &text as &[&str],
        )
        .execute(&mut *conn)
        .await?;
    }

    sqlx::query!(
        r#"INSERT INTO file_extractions
               (file_id, extractor, extractor_version, page_count, char_count, text, pages, notes)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
           ON CONFLICT (file_id) DO UPDATE SET
               extractor = EXCLUDED.extractor,
               extractor_version = EXCLUDED.extractor_version,
               page_count = EXCLUDED.page_count,
               char_count = EXCLUDED.char_count,
               text = EXCLUDED.text,
               pages = EXCLUDED.pages,
               notes = EXCLUDED.notes,
               created_at = now()"#,
        file_id,
        extraction.extractor,
        extraction.extractor_version,
        extraction.page_count,
        extraction.char_count,
        extraction.text,
        extraction.pages,
        extraction.notes,
    )
    .execute(&mut *conn)
    .await?;
    Ok(Some(owner_id))
}

/// A stored extraction with a window of its text.
#[derive(Debug, Clone)]
pub struct Extraction {
    pub extractor: String,
    pub extractor_version: String,
    pub page_count: Option<i32>,
    pub char_count: i32,
    pub chunk_count: i64,
    pub pages: JsonValue,
    pub notes: Vec<String>,
    /// Characters `offset..offset + limit` of the text.
    pub text: String,
    pub created_at: DateTime<Utc>,
}

/// The owner's extraction of `file_id`, with `limit` characters of text starting
/// at character `offset` (0-based). `None` if there is no such file or it has not
/// been extracted yet.
pub async fn get(
    pool: &PgPool,
    owner_id: Uuid,
    file_id: Uuid,
    offset: i32,
    limit: i32,
) -> Result<Option<Extraction>, sqlx::Error> {
    sqlx::query_as!(
        Extraction,
        r#"SELECT e.extractor, e.extractor_version, e.page_count, e.char_count, e.pages,
                  e.notes, e.created_at,
                  substr(e.text, $3 + 1, $4) AS "text!",
                  (SELECT count(*) FROM file_chunks c WHERE c.file_id = e.file_id) AS "chunk_count!"
           FROM file_extractions e
           JOIN files f ON f.id = e.file_id
           WHERE f.owner_id = $1 AND e.file_id = $2"#,
        owner_id,
        file_id,
        offset,
        limit,
    )
    .fetch_optional(pool)
    .await
}
