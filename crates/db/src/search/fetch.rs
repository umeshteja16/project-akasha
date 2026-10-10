//! Turning candidate ids into rows: chunks with their file and a highlighted
//! excerpt, file summaries, and a file's mean embedding.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// Marks the start of a highlighted term in [`ChunkRow::headline`] (a private-use
/// character, removed from the text first so it cannot appear otherwise).
pub const HIGHLIGHT_START: char = '\u{E000}';
/// Marks the end of a highlighted term in [`ChunkRow::headline`].
pub const HIGHLIGHT_END: char = '\u{E001}';

/// A chunk with its file's metadata.
#[derive(Debug, Clone)]
pub struct ChunkRow {
    pub id: i64,
    pub file_id: Uuid,
    pub chunk_index: i32,
    pub page: Option<i32>,
    pub start_ms: Option<i32>,
    pub end_ms: Option<i32>,
    pub char_start: i32,
    pub char_end: i32,
    pub text: String,
    /// `ts_headline` excerpt; query terms are wrapped in [`HIGHLIGHT_START`] /
    /// [`HIGHLIGHT_END`], fragments joined by " … ".
    pub headline: String,
    pub file_name: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub status: String,
    pub tags: Vec<String>,
    pub auto_tags: Vec<String>,
    pub summary: Option<String>,
    pub is_pinned: bool,
    pub created_at: DateTime<Utc>,
}

/// The owner's chunks among `ids` (in no particular order), with a headline
/// for `query` (a `websearch_to_tsquery` string).
pub async fn chunks(
    pool: &PgPool,
    owner_id: Uuid,
    ids: &[i64],
    query: &str,
) -> Result<Vec<ChunkRow>, sqlx::Error> {
    sqlx::query_as!(
        ChunkRow,
        r#"SELECT c.id, c.file_id, c.chunk_index, c.page, c.start_ms, c.end_ms,
                  c.char_start, c.char_end, c.text,
                  ts_headline('english', translate(c.text, E'', ''), q.tsq,
                      E'StartSel=, StopSel=, MaxWords=35, MinWords=15, '
                      'MaxFragments=2, FragmentDelimiter=" … "') AS "headline!",
                  f.original_name AS file_name, f.mime_type, f.size_bytes, f.status,
                  f.tags, f.auto_tags, f.summary, f.is_pinned, f.created_at
           FROM file_chunks c
           JOIN files f ON f.id = c.file_id AND f.owner_id = $1
           CROSS JOIN websearch_to_tsquery('english', $3) AS q(tsq)
           WHERE c.owner_id = $1 AND c.id = ANY($2)"#,
        owner_id,
        ids,
        query,
    )
    .fetch_all(pool)
    .await
}

/// File metadata for result lists.
#[derive(Debug, Clone)]
pub struct FileRow {
    pub id: Uuid,
    pub original_name: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub status: String,
    pub tags: Vec<String>,
    pub auto_tags: Vec<String>,
    pub summary: Option<String>,
    pub is_pinned: bool,
    pub created_at: DateTime<Utc>,
}

/// The owner's files among `ids` (in no particular order).
pub async fn files_by_ids(
    pool: &PgPool,
    owner_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<FileRow>, sqlx::Error> {
    sqlx::query_as!(
        FileRow,
        "SELECT id, original_name, mime_type, size_bytes, status, tags, auto_tags, summary,
                is_pinned, created_at
         FROM files WHERE owner_id = $1 AND id = ANY($2)",
        owner_id,
        ids,
    )
    .fetch_all(pool)
    .await
}

/// Mean of the file's chunk vectors, or `None` if it has none (not embedded
/// yet, nothing extracted, or not the owner's file).
pub async fn mean_embedding(
    pool: &PgPool,
    owner_id: Uuid,
    file_id: Uuid,
) -> Result<Option<Vec<f32>>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT avg(embedding)::real[] AS "mean" FROM file_chunks
           WHERE owner_id = $1 AND file_id = $2 AND embedding IS NOT NULL"#,
        owner_id,
        file_id,
    )
    .fetch_one(pool)
    .await
}
