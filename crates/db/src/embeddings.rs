//! Chunk embeddings (`file_chunks.embedding`) and the record of which model made
//! them (`embedding_model`, ADR 0009).
//!
//! Internal: used by the embed job and admin commands, which only know file ids.
//! Never reach these from a request without an ownership check.

use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// pgvector's HNSW index supports at most this many dimensions.
pub const MAX_INDEXED_DIM: usize = 2000;

/// The model the stored vectors belong to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedModel {
    pub name: String,
    pub dim: i32,
}

/// Result of comparing the configured model with the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelCheck {
    /// Same model (recorded now if nothing was recorded yet).
    Matches,
    /// Vectors in the database come from another model.
    Mismatch(RecordedModel),
    /// Nothing recorded yet, but the column has another dimension.
    ColumnMismatch { column_dim: i32 },
}

/// The recorded model, if any.
pub async fn recorded(pool: &PgPool) -> Result<Option<RecordedModel>, sqlx::Error> {
    sqlx::query_as!(RecordedModel, "SELECT name, dim FROM embedding_model")
        .fetch_optional(pool)
        .await
}

/// Dimension of `file_chunks.embedding` (`None` if the column is missing).
pub async fn column_dim(conn: &mut PgConnection) -> Result<Option<i32>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT atttypmod AS "dim!" FROM pg_attribute
           WHERE attrelid = 'file_chunks'::regclass AND attname = 'embedding'
             AND NOT attisdropped"#
    )
    .fetch_optional(conn)
    .await
}

/// Compare `name`/`dim` with the recorded model; on a fresh database (nothing
/// recorded) record it, provided the column has the right dimension.
pub async fn check_model(pool: &PgPool, name: &str, dim: i32) -> Result<ModelCheck, sqlx::Error> {
    if recorded(pool).await?.is_none() {
        let mut conn = pool.acquire().await?;
        let column = column_dim(&mut conn).await?;
        if column != Some(dim) {
            return Ok(ModelCheck::ColumnMismatch {
                column_dim: column.unwrap_or(0),
            });
        }
        // Concurrent starters: the first insert wins, the others compare below.
        sqlx::query!(
            "INSERT INTO embedding_model (name, dim) VALUES ($1, $2) ON CONFLICT (id) DO NOTHING",
            name,
            dim
        )
        .execute(&mut *conn)
        .await?;
    }
    match recorded(pool).await? {
        Some(r) if r.name == name && r.dim == dim => Ok(ModelCheck::Matches),
        Some(r) => Ok(ModelCheck::Mismatch(r)),
        None => Ok(ModelCheck::ColumnMismatch { column_dim: 0 }),
    }
}

/// A chunk waiting for its vector.
#[derive(Debug, Clone)]
pub struct PendingChunk {
    pub id: i64,
    pub text: String,
}

/// Up to `limit` chunks of the file without an embedding, in reading order.
pub async fn pending(
    pool: &PgPool,
    file_id: Uuid,
    limit: i64,
) -> Result<Vec<PendingChunk>, sqlx::Error> {
    sqlx::query_as!(
        PendingChunk,
        "SELECT id, text FROM file_chunks
         WHERE file_id = $1 AND embedding IS NULL
         ORDER BY chunk_index LIMIT $2",
        file_id,
        limit
    )
    .fetch_all(pool)
    .await
}

/// Store vectors for `ids` (`vectors` is `ids.len() * dim` floats, row after row).
/// Only fills chunks that still have no vector, and only while `model` is the
/// recorded model (a worker running with a stale config writes nothing).
/// Returns how many chunks were updated.
pub async fn store(
    conn: &mut PgConnection,
    model: &str,
    dim: i32,
    ids: &[i64],
    vectors: &[f32],
) -> Result<u64, sqlx::Error> {
    let done = sqlx::query!(
        r#"UPDATE file_chunks c
           SET embedding = (($3::real[])[(v.ord - 1) * $2 + 1 : v.ord * $2])::vector
           FROM unnest($4::bigint[]) WITH ORDINALITY AS v(id, ord)
           WHERE c.id = v.id AND c.embedding IS NULL
             AND EXISTS (SELECT 1 FROM embedding_model m WHERE m.name = $1 AND m.dim = $2)"#,
        model,
        dim,
        vectors,
        ids,
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected())
}

/// How finishing a file went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finish {
    /// The file was deleted.
    Gone,
    /// Chunks without vectors remain (e.g. re-extracted meanwhile).
    Pending,
    /// Every chunk has a vector; a `processing` file is now `ready`.
    Done,
}

/// Mark the file `ready` if all its chunks have vectors (locks the file row).
pub async fn finish(conn: &mut PgConnection, file_id: Uuid) -> Result<Finish, sqlx::Error> {
    let exists = sqlx::query_scalar!(
        "SELECT 1 AS one FROM files WHERE id = $1 FOR UPDATE",
        file_id
    )
    .fetch_optional(&mut *conn)
    .await?;
    if exists.is_none() {
        return Ok(Finish::Gone);
    }
    let pending = sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM file_chunks WHERE file_id = $1 AND embedding IS NULL) AS "pending!""#,
        file_id
    )
    .fetch_one(&mut *conn)
    .await?;
    if pending {
        return Ok(Finish::Pending);
    }
    // A file put back to `pending` (reindex) waits for its new extraction instead.
    sqlx::query!(
        "UPDATE files SET status = 'ready', error = NULL WHERE id = $1 AND status = 'processing'",
        file_id
    )
    .execute(&mut *conn)
    .await?;
    Ok(Finish::Done)
}

/// Switch the database to another model (admin, `akasha reembed`): drop every
/// vector, re-type the column to `dim`, rebuild the HNSW index, record the model
/// and put every file with chunks back to `processing`. Returns those files, for
/// the caller to queue embed jobs in the same transaction.
///
/// The only schema change made outside migrations (ADR 0009). Takes an exclusive
/// lock on `file_chunks` until the transaction ends.
pub async fn reset(
    conn: &mut PgConnection,
    name: &str,
    dim: usize,
) -> Result<Vec<Uuid>, sqlx::Error> {
    if dim == 0 || dim > MAX_INDEXED_DIM {
        return Err(sqlx::Error::Protocol(format!(
            "embedding dimension {dim} is outside 1..={MAX_INDEXED_DIM}"
        )));
    }
    // DDL cannot take bind parameters; `dim` is a checked integer.
    sqlx::query("DROP INDEX IF EXISTS file_chunks_embedding_idx")
        .execute(&mut *conn)
        .await?;
    sqlx::query(&format!(
        "ALTER TABLE file_chunks ALTER COLUMN embedding TYPE vector({dim}) USING NULL"
    ))
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "CREATE INDEX file_chunks_embedding_idx ON file_chunks USING hnsw (embedding vector_cosine_ops)",
    )
    .execute(&mut *conn)
    .await?;
    let dim = i32::try_from(dim).map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
    sqlx::query!(
        "INSERT INTO embedding_model (name, dim) VALUES ($1, $2)
         ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, dim = EXCLUDED.dim, updated_at = now()",
        name,
        dim
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query_scalar!(
        "UPDATE files f SET status = 'processing', error = NULL
         WHERE EXISTS (SELECT 1 FROM file_chunks c WHERE c.file_id = f.id)
         RETURNING id"
    )
    .fetch_all(&mut *conn)
    .await
}
