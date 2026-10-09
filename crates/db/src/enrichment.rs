//! Model-written summaries and tags of files (the `enrich_file` job).
//!
//! The job runs outside any request, so these queries take the file id alone;
//! callers that act for a user check ownership first. Results are written only
//! while the extraction they describe is still the current one, so a slow model
//! call can never attach an old summary to new text.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// What enrichment needs to know about a file.
#[derive(Debug, Clone)]
pub struct Source {
    pub owner_id: Uuid,
    pub original_name: String,
    pub status: String,
    /// `None`: never enriched.
    pub enrichment_status: Option<String>,
    /// The extraction the current summary describes.
    pub enriched_from: Option<DateTime<Utc>>,
    /// The current extraction (`None`: not extracted).
    pub extracted_at: Option<DateTime<Utc>>,
    /// Characters of extracted text.
    pub char_count: Option<i32>,
    /// The first `head_chars` characters of the extracted text.
    pub head: Option<String>,
}

impl Source {
    /// The summary describes the current extraction already.
    pub fn is_current(&self) -> bool {
        self.extracted_at.is_some()
            && self.enriched_from == self.extracted_at
            && matches!(self.enrichment_status.as_deref(), Some("done" | "skipped"))
    }
}

/// The file and the start of its text; `None` if the file is gone.
pub async fn source(
    pool: &PgPool,
    file_id: Uuid,
    head_chars: i32,
) -> Result<Option<Source>, sqlx::Error> {
    sqlx::query_as!(
        Source,
        r#"SELECT f.owner_id, f.original_name, f.status, f.enrichment_status, f.enriched_from,
                  e.created_at AS "extracted_at?", e.char_count AS "char_count?",
                  substr(e.text, 1, $2) AS "head?"
           FROM files f
           LEFT JOIN file_extractions e ON e.file_id = f.id
           WHERE f.id = $1"#,
        file_id,
        head_chars,
    )
    .fetch_optional(pool)
    .await
}

/// Up to `count` chunks starting at or after character `after`, spread evenly
/// over the rest of the text (the first chunk of each of `count` equal runs),
/// each cut to `max_chars` characters. In text order.
pub async fn samples(
    pool: &PgPool,
    file_id: Uuid,
    after: i32,
    count: i32,
    max_chars: i32,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT left(text, $4) AS "text!" FROM (
               SELECT DISTINCT ON (bucket) text, char_start
               FROM (SELECT text, char_start, ntile($3) OVER (ORDER BY char_start) AS bucket
                     FROM file_chunks WHERE file_id = $1 AND char_start >= $2) runs
               ORDER BY bucket, char_start
           ) picked
           ORDER BY char_start"#,
        file_id,
        after,
        count,
        max_chars,
    )
    .fetch_all(pool)
    .await
}

/// Store a summary and suggested tags made from the extraction created at
/// `from`. `false` (nothing written) if the file is gone or was re-extracted
/// meanwhile. The user's own `tags` are never touched.
pub async fn store(
    pool: &PgPool,
    file_id: Uuid,
    from: DateTime<Utc>,
    summary: &str,
    auto_tags: &[String],
    model: &str,
) -> Result<bool, sqlx::Error> {
    let res = sqlx::query!(
        r#"UPDATE files SET summary = $3, auto_tags = $4, enrichment_status = 'done',
                  enrichment_model = $5, enriched_from = $2, enriched_at = now()
           WHERE id = $1
             AND EXISTS (SELECT 1 FROM file_extractions
                         WHERE file_id = $1 AND created_at = $2)"#,
        file_id,
        from,
        summary,
        auto_tags,
        model,
    )
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// Record that enrichment was `skipped` (nothing to describe) or `failed`.
/// An earlier summary and tags are kept.
pub async fn mark(
    pool: &PgPool,
    file_id: Uuid,
    status: &str,
    from: Option<DateTime<Utc>>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE files SET enrichment_status = $2, enriched_from = $3, enriched_at = now()
         WHERE id = $1",
        file_id,
        status,
        from,
    )
    .execute(pool)
    .await?;
    Ok(())
}
