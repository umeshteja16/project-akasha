//! Search queries over `file_chunks` (keyword, file name, vector) and the
//! lookups that turn candidate chunk ids into results.
//!
//! Every function takes the owner: candidates are filtered on the denormalised
//! `file_chunks.owner_id` *and* the joined `files.owner_id`, so another user's
//! chunks can never come back, whatever the filters say.
//!
//! Fusion, reranking and snippets live in `akasha-search`.

mod fetch;
mod terms;

pub use fetch::{
    ChunkRow, FileRow, HIGHLIGHT_END, HIGHLIGHT_START, chunks, files_by_ids, mean_embedding,
};
pub use terms::closest_terms;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// Text search configuration of `file_chunks.tsv`.
pub const TS_CONFIG: &str = "english";

/// Owners with at most this many embedded chunks (and searches restricted to
/// specific files) get an exact vector scan instead of the HNSW index: it is fast
/// at that size and, unlike the index, cannot lose results to the owner filter.
pub const EXACT_SCAN_MAX_CHUNKS: i64 = 10_000;

/// Restricts which files' chunks a search may return. Empty / `None` means "any".
#[derive(Debug, Default, Clone)]
pub struct ChunkFilter {
    /// `LIKE` patterns on `files.mime_type`; a file matches if any pattern does.
    pub mime_patterns: Vec<String>,
    /// Uploaded at or after.
    pub from: Option<DateTime<Utc>>,
    /// Uploaded before (exclusive).
    pub to: Option<DateTime<Utc>>,
    /// The file must carry every one of these tags.
    pub tags: Vec<String>,
    pub pinned: Option<bool>,
    /// Only these files.
    pub file_ids: Vec<Uuid>,
    /// Never this file (similar-file lookups).
    pub exclude_file: Option<Uuid>,
    // Collections (step 6) become one more `file_id IN (SELECT ...)` condition here.
}

/// A candidate chunk with the score its retriever gave it (higher is better).
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub chunk_id: i64,
    pub file_id: Uuid,
    pub score: f32,
}

/// Full-text matches: `websearch_to_tsquery` against `tsv`, ranked by
/// `ts_rank_cd` (normalised to 0..1).
pub async fn keyword(
    pool: &PgPool,
    owner_id: Uuid,
    query: &str,
    filter: &ChunkFilter,
    limit: i64,
) -> Result<Vec<Candidate>, sqlx::Error> {
    sqlx::query_as!(
        Candidate,
        r#"SELECT c.id AS chunk_id, c.file_id, ts_rank_cd(c.tsv, q.tsq, 32)::real AS "score!"
           FROM file_chunks c
           JOIN files f ON f.id = c.file_id AND f.owner_id = $1
           CROSS JOIN websearch_to_tsquery('english', $2) AS q(tsq)
           WHERE c.owner_id = $1 AND c.tsv @@ q.tsq
             AND (cardinality($3::text[]) = 0 OR f.mime_type LIKE ANY($3))
             AND ($4::timestamptz IS NULL OR f.created_at >= $4)
             AND ($5::timestamptz IS NULL OR f.created_at < $5)
             AND f.tags @> $6::text[]
             AND ($7::bool IS NULL OR f.is_pinned = $7)
             AND (cardinality($8::uuid[]) = 0 OR c.file_id = ANY($8))
             AND ($9::uuid IS NULL OR c.file_id <> $9)
           ORDER BY 3 DESC, c.id
           LIMIT $10"#,
        owner_id,
        query,
        &filter.mime_patterns,
        filter.from,
        filter.to,
        &filter.tags,
        filter.pinned,
        &filter.file_ids,
        filter.exclude_file,
        limit,
    )
    .fetch_all(pool)
    .await
}

/// Files whose name matches the query (punctuation counts as a word break, so
/// `q3_budget-final.pdf` matches "budget"), each represented by its first chunk.
/// Files without chunks are skipped: there is nothing to show or cite.
pub async fn filename(
    pool: &PgPool,
    owner_id: Uuid,
    query: &str,
    filter: &ChunkFilter,
    limit: i64,
) -> Result<Vec<Candidate>, sqlx::Error> {
    sqlx::query_as!(
        Candidate,
        r#"SELECT c.id AS "chunk_id!", c.file_id AS "file_id!", n.score AS "score!"
           FROM (
               SELECT f.id, ts_rank_cd(v.doc, q.tsq, 32)::real AS score
               FROM files f
               CROSS JOIN websearch_to_tsquery('english', $2) AS q(tsq)
               CROSS JOIN LATERAL (SELECT to_tsvector('english',
                   regexp_replace(f.original_name, '[^[:alnum:]]+', ' ', 'g')) AS doc) v
               WHERE f.owner_id = $1 AND v.doc @@ q.tsq
                 AND (cardinality($3::text[]) = 0 OR f.mime_type LIKE ANY($3))
                 AND ($4::timestamptz IS NULL OR f.created_at >= $4)
                 AND ($5::timestamptz IS NULL OR f.created_at < $5)
                 AND f.tags @> $6::text[]
                 AND ($7::bool IS NULL OR f.is_pinned = $7)
                 AND (cardinality($8::uuid[]) = 0 OR f.id = ANY($8))
                 AND ($9::uuid IS NULL OR f.id <> $9)
           ) n
           CROSS JOIN LATERAL (
               SELECT id, file_id FROM file_chunks
               WHERE file_id = n.id AND owner_id = $1
               ORDER BY chunk_index LIMIT 1
           ) c
           ORDER BY n.score DESC, c.id
           LIMIT $10"#,
        owner_id,
        query,
        &filter.mime_patterns,
        filter.from,
        filter.to,
        &filter.tags,
        filter.pinned,
        &filter.file_ids,
        filter.exclude_file,
        limit,
    )
    .fetch_all(pool)
    .await
}

/// Nearest chunks to `vector` by cosine distance; `score` is cosine similarity.
///
/// Small libraries (and file-restricted searches) are scanned exactly. Larger
/// ones use the HNSW index with `ef_search` raised to `max(limit, ef_search)`
/// and, on pgvector 0.8+, iterative scans, so the owner filter does not starve
/// the result list.
pub async fn semantic(
    pool: &PgPool,
    owner_id: Uuid,
    vector: &[f32],
    filter: &ChunkFilter,
    limit: i64,
    ef_search: i64,
) -> Result<Vec<Candidate>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let embedded = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM (
               SELECT 1 FROM file_chunks
               WHERE owner_id = $1 AND embedding IS NOT NULL LIMIT $2
           ) s"#,
        owner_id,
        EXACT_SCAN_MAX_CHUNKS + 1
    )
    .fetch_one(&mut *tx)
    .await?;
    let exact = !filter.file_ids.is_empty() || embedded <= EXACT_SCAN_MAX_CHUNKS;
    if !exact {
        let ef = ef_search.max(limit).clamp(1, 1000).to_string();
        sqlx::query_scalar!("SELECT set_config('hnsw.ef_search', $1, true)", ef)
            .fetch_one(&mut *tx)
            .await?;
        if supports_iterative_scan(&mut tx).await? {
            sqlx::query_scalar!("SELECT set_config('hnsw.iterative_scan', 'strict_order', true)")
                .fetch_one(&mut *tx)
                .await?;
        }
    }
    // `+ 0` hides the distance from the planner, which forces the exact scan.
    let rows = if exact {
        sqlx::query_as!(
            Candidate,
            r#"SELECT c.id AS chunk_id, c.file_id,
                      (1 - (c.embedding <=> $2::real[]::vector))::real AS "score!"
               FROM file_chunks c
               JOIN files f ON f.id = c.file_id AND f.owner_id = $1
               WHERE c.owner_id = $1 AND c.embedding IS NOT NULL
                 AND (cardinality($3::text[]) = 0 OR f.mime_type LIKE ANY($3))
                 AND ($4::timestamptz IS NULL OR f.created_at >= $4)
                 AND ($5::timestamptz IS NULL OR f.created_at < $5)
                 AND f.tags @> $6::text[]
                 AND ($7::bool IS NULL OR f.is_pinned = $7)
                 AND (cardinality($8::uuid[]) = 0 OR c.file_id = ANY($8))
                 AND ($9::uuid IS NULL OR c.file_id <> $9)
               ORDER BY (c.embedding <=> $2::real[]::vector) + 0, c.id
               LIMIT $10"#,
            owner_id,
            vector,
            &filter.mime_patterns,
            filter.from,
            filter.to,
            &filter.tags,
            filter.pinned,
            &filter.file_ids,
            filter.exclude_file,
            limit,
        )
        .fetch_all(&mut *tx)
        .await?
    } else {
        sqlx::query_as!(
            Candidate,
            r#"SELECT c.id AS chunk_id, c.file_id,
                      (1 - (c.embedding <=> $2::real[]::vector))::real AS "score!"
               FROM file_chunks c
               JOIN files f ON f.id = c.file_id AND f.owner_id = $1
               WHERE c.owner_id = $1 AND c.embedding IS NOT NULL
                 AND (cardinality($3::text[]) = 0 OR f.mime_type LIKE ANY($3))
                 AND ($4::timestamptz IS NULL OR f.created_at >= $4)
                 AND ($5::timestamptz IS NULL OR f.created_at < $5)
                 AND f.tags @> $6::text[]
                 AND ($7::bool IS NULL OR f.is_pinned = $7)
                 AND (cardinality($8::uuid[]) = 0 OR c.file_id = ANY($8))
                 AND ($9::uuid IS NULL OR c.file_id <> $9)
               ORDER BY c.embedding <=> $2::real[]::vector
               LIMIT $10"#,
            owner_id,
            vector,
            &filter.mime_patterns,
            filter.from,
            filter.to,
            &filter.tags,
            filter.pinned,
            &filter.file_ids,
            filter.exclude_file,
            limit,
        )
        .fetch_all(&mut *tx)
        .await?
    };
    tx.commit().await?;
    Ok(rows)
}

/// `hnsw.iterative_scan` exists from pgvector 0.8; setting it on older versions
/// is an error once the extension is loaded.
async fn supports_iterative_scan(conn: &mut sqlx::PgConnection) -> Result<bool, sqlx::Error> {
    let version =
        sqlx::query_scalar!("SELECT extversion FROM pg_extension WHERE extname = 'vector'")
            .fetch_optional(conn)
            .await?
            .unwrap_or_default();
    Ok(at_least(&version, (0, 8)))
}

fn at_least(version: &str, (major, minor): (u32, u32)) -> bool {
    let mut parts = version.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    let found = (parts.next().unwrap_or(0), parts.next().unwrap_or(0));
    found >= (major, minor)
}

#[cfg(test)]
mod tests {
    use super::at_least;

    #[test]
    fn version_comparison() {
        assert!(at_least("0.8.0", (0, 8)));
        assert!(at_least("0.10.1", (0, 8)));
        assert!(at_least("1.0", (0, 8)));
        assert!(!at_least("0.6.0", (0, 8)));
        assert!(!at_least("", (0, 8)));
    }
}
