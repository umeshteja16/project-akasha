//! Watched folders ("sources") and the files seen in them.
//!
//! Every owner-facing query takes the owner's id. The scan job works by source id
//! ([`acquire_scan`]) and then only touches that source's rows. `source_files`
//! references its source and its file by `(id, owner_id)`, so the schema refuses to
//! link a source to another user's file.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Source {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub kind: String,
    pub name: String,
    pub path: String,
    pub include_globs: Vec<String>,
    pub exclude_globs: Vec<String>,
    pub on_delete: String,
    pub import_tags: bool,
    pub enabled: bool,
    pub status: String,
    pub last_error: Option<String>,
    pub scan_started_at: Option<DateTime<Utc>>,
    pub last_scan_at: Option<DateTime<Utc>>,
    pub last_scan: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A source with how many of its files are in the library and how many were skipped.
#[derive(Debug, Clone)]
pub struct SourceSummary {
    pub source: Source,
    pub file_count: i64,
    pub skipped_count: i64,
}

pub struct NewSource<'a> {
    pub owner_id: Uuid,
    pub name: &'a str,
    pub path: &'a str,
    pub include_globs: &'a [String],
    pub exclude_globs: &'a [String],
    pub on_delete: &'a str,
    pub import_tags: bool,
}

/// What [`update`] changes; `None` leaves a field as it is.
#[derive(Debug, Default, Clone, Copy)]
pub struct SourceChanges<'a> {
    pub name: Option<&'a str>,
    pub include_globs: Option<&'a [String]>,
    pub exclude_globs: Option<&'a [String]>,
    pub on_delete: Option<&'a str>,
    pub import_tags: Option<bool>,
    pub enabled: Option<bool>,
}

/// Insert a source. `None`: the owner already watches this path.
pub async fn create(
    conn: &mut PgConnection,
    new: &NewSource<'_>,
) -> Result<Option<Source>, sqlx::Error> {
    sqlx::query_as!(
        Source,
        r#"INSERT INTO sources (owner_id, name, path, include_globs, exclude_globs, on_delete,
                                import_tags)
           VALUES ($1, $2, $3, $4, $5, $6, $7)
           ON CONFLICT (owner_id, path) DO NOTHING
           RETURNING *"#,
        new.owner_id,
        new.name,
        new.path,
        new.include_globs,
        new.exclude_globs,
        new.on_delete,
        new.import_tags,
    )
    .fetch_optional(conn)
    .await
}

/// The owner's sources, oldest first, with file counts.
pub async fn list(pool: &PgPool, owner_id: Uuid) -> Result<Vec<SourceSummary>, sqlx::Error> {
    let sources = sqlx::query_as!(
        Source,
        "SELECT * FROM sources WHERE owner_id = $1 ORDER BY created_at, id",
        owner_id
    )
    .fetch_all(pool)
    .await?;
    let counts = sqlx::query!(
        r#"SELECT source_id,
                  count(file_id) AS "files!",
                  count(*) FILTER (WHERE skip_reason IS NOT NULL) AS "skipped!"
           FROM source_files WHERE owner_id = $1 GROUP BY source_id"#,
        owner_id
    )
    .fetch_all(pool)
    .await?;
    Ok(sources
        .into_iter()
        .map(|source| {
            let row = counts.iter().find(|c| c.source_id == source.id);
            SourceSummary {
                file_count: row.map_or(0, |r| r.files),
                skipped_count: row.map_or(0, |r| r.skipped),
                source,
            }
        })
        .collect())
}

pub async fn get<'e>(
    db: impl sqlx::PgExecutor<'e>,
    owner_id: Uuid,
    id: Uuid,
) -> Result<Option<Source>, sqlx::Error> {
    sqlx::query_as!(
        Source,
        "SELECT * FROM sources WHERE owner_id = $1 AND id = $2",
        owner_id,
        id
    )
    .fetch_optional(db)
    .await
}

/// One source with its counts (for responses).
pub async fn summary(
    pool: &PgPool,
    owner_id: Uuid,
    id: Uuid,
) -> Result<Option<SourceSummary>, sqlx::Error> {
    Ok(list(pool, owner_id)
        .await?
        .into_iter()
        .find(|s| s.source.id == id))
}

pub async fn update<'e>(
    db: impl sqlx::PgExecutor<'e>,
    owner_id: Uuid,
    id: Uuid,
    changes: SourceChanges<'_>,
) -> Result<Option<Source>, sqlx::Error> {
    sqlx::query_as!(
        Source,
        r#"UPDATE sources SET
               name = COALESCE($3, name),
               include_globs = COALESCE($4, include_globs),
               exclude_globs = COALESCE($5, exclude_globs),
               on_delete = COALESCE($6, on_delete),
               import_tags = COALESCE($7, import_tags),
               enabled = COALESCE($8, enabled)
           WHERE owner_id = $1 AND id = $2
           RETURNING *"#,
        owner_id,
        id,
        changes.name,
        changes.include_globs,
        changes.exclude_globs,
        changes.on_delete,
        changes.import_tags,
        changes.enabled,
    )
    .fetch_optional(db)
    .await
}

/// Delete a source and its `source_files` rows (not the files). `false`: not found.
pub async fn delete(
    conn: &mut PgConnection,
    owner_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "DELETE FROM sources WHERE owner_id = $1 AND id = $2",
        owner_id,
        id
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Files this source added to the library and that no other source of the owner
/// also maps (those stay).
pub async fn created_files(
    conn: &mut PgConnection,
    owner_id: Uuid,
    id: Uuid,
) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT DISTINCT sf.file_id AS "file_id!" FROM source_files sf
           WHERE sf.owner_id = $1 AND sf.source_id = $2 AND sf.created_file
             AND sf.file_id IS NOT NULL
             AND NOT EXISTS (SELECT 1 FROM source_files o
                             WHERE o.file_id = sf.file_id AND o.source_id <> sf.source_id)"#,
        owner_id,
        id
    )
    .fetch_all(conn)
    .await
}

/// Enabled sources (id, owner, path) for the periodic scan and the watcher.
pub async fn enabled(pool: &PgPool) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
    let rows = sqlx::query!("SELECT id, path FROM sources WHERE enabled ORDER BY id")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.path)).collect())
}

/// Start a scan: mark the source `scanning` unless it is paused, gone, or another
/// scan holds a lease younger than `lease_secs`.
pub async fn acquire_scan(
    pool: &PgPool,
    id: Uuid,
    lease_secs: f64,
) -> Result<Option<Source>, sqlx::Error> {
    sqlx::query_as!(
        Source,
        r#"UPDATE sources SET status = 'scanning', scan_started_at = now()
           WHERE id = $1 AND enabled
             AND (scan_started_at IS NULL
                  OR scan_started_at < now() - make_interval(secs => $2))
           RETURNING *"#,
        id,
        lease_secs
    )
    .fetch_optional(pool)
    .await
}

/// End a scan. `finished`: a full pass completed (sets `last_scan_at`/`last_scan`);
/// otherwise the source stays `scanning` for the follow-up job.
pub async fn finish_scan(
    pool: &PgPool,
    id: Uuid,
    status: &str,
    error: Option<&str>,
    stats: &serde_json::Value,
    finished: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE sources SET status = $2, last_error = $3, scan_started_at = NULL,
               last_scan = CASE WHEN $5 THEN $4 ELSE last_scan END,
               last_scan_at = CASE WHEN $5 THEN now() ELSE last_scan_at END
           WHERE id = $1"#,
        id,
        status,
        error,
        stats,
        finished
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// A file seen in a source.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct SourceFile {
    pub source_id: Uuid,
    pub owner_id: Uuid,
    pub rel_path: String,
    pub size_bytes: i64,
    pub mtime: DateTime<Utc>,
    pub content_hash: Option<String>,
    pub file_id: Option<Uuid>,
    pub created_file: bool,
    pub skip_reason: Option<String>,
    pub synced_at: DateTime<Utc>,
}

/// Every row of one source.
pub async fn files_of(pool: &PgPool, source_id: Uuid) -> Result<Vec<SourceFile>, sqlx::Error> {
    sqlx::query_as!(
        SourceFile,
        "SELECT * FROM source_files WHERE source_id = $1",
        source_id
    )
    .fetch_all(pool)
    .await
}

/// What [`upsert_file`] stores for a path.
pub struct FileState<'a> {
    pub source_id: Uuid,
    pub owner_id: Uuid,
    pub rel_path: &'a str,
    pub size_bytes: i64,
    pub mtime: DateTime<Utc>,
    pub content_hash: Option<&'a str>,
    pub file_id: Option<Uuid>,
    pub created_file: bool,
    pub skip_reason: Option<&'a str>,
}

pub async fn upsert_file(conn: &mut PgConnection, s: &FileState<'_>) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"INSERT INTO source_files (source_id, owner_id, rel_path, size_bytes, mtime,
                                     content_hash, file_id, created_file, skip_reason)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
           ON CONFLICT (source_id, rel_path) DO UPDATE SET
               size_bytes = EXCLUDED.size_bytes, mtime = EXCLUDED.mtime,
               content_hash = EXCLUDED.content_hash, file_id = EXCLUDED.file_id,
               created_file = EXCLUDED.created_file, skip_reason = EXCLUDED.skip_reason,
               synced_at = now()"#,
        s.source_id,
        s.owner_id,
        s.rel_path,
        s.size_bytes,
        s.mtime,
        s.content_hash,
        s.file_id,
        s.created_file,
        s.skip_reason,
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete_file_row(
    conn: &mut PgConnection,
    source_id: Uuid,
    rel_path: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM source_files WHERE source_id = $1 AND rel_path = $2",
        source_id,
        rel_path
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Another row (of any of the owner's sources) that maps `file_id`, e.g. the new path
/// of a renamed file. Locks it.
pub async fn other_mapping(
    conn: &mut PgConnection,
    file_id: Uuid,
    source_id: Uuid,
    rel_path: &str,
) -> Result<Option<(Uuid, String)>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT source_id, rel_path FROM source_files
           WHERE file_id = $1 AND NOT (source_id = $2 AND rel_path = $3)
           ORDER BY (source_id = $2) DESC, rel_path LIMIT 1 FOR UPDATE"#,
        file_id,
        source_id,
        rel_path
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|r| (r.source_id, r.rel_path)))
}

/// Hand "this source created the file" over to another row (a rename).
pub async fn set_created(
    conn: &mut PgConnection,
    source_id: Uuid,
    rel_path: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE source_files SET created_file = true WHERE source_id = $1 AND rel_path = $2",
        source_id,
        rel_path
    )
    .execute(conn)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests;
