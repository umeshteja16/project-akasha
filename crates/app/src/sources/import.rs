//! Importing one file of a watched folder through the upload pipeline.
//!
//! The bytes go through the same checks as an upload (size limit, sniffed type on the
//! allow-list, UTF-8 text) into staging, then one transaction stores the file (or new
//! content for a file this folder imported before), queues its processing and writes
//! the `source_files` row, so a crash leaves either both or neither.

use std::{path::Path, sync::Arc};

use akasha_core::{Error, ErrorCode};
use akasha_db::{
    files,
    sources::{self, FileState, Source, SourceFile},
};
use akasha_jobs::JobError;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures_util::{StreamExt, TryStreamExt};
use tokio::io::AsyncReadExt;
use uuid::Uuid;

use super::{
    frontmatter,
    paths::{self, OpenError},
    walk::{Seen, micros},
};
use crate::{
    files::{
        name,
        receive::receive,
        store::{self, Replaced, Saved},
    },
    jobs::JobContext,
};

/// Recorded when the owner's quota is full; such files are retried on every scan.
pub const QUOTA_REASON: &str = "storage quota exceeded";
/// Start of the reason recorded for files that could not be opened; retried on every scan.
pub const UNREADABLE: &str = "could not be read";
/// Files modified this recently may still be being written: picked up next time.
const SETTLE: chrono::TimeDelta = chrono::TimeDelta::seconds(2);
const READ_CHUNK: usize = 64 * 1024;

/// What happened to one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Imported,
    Updated,
    /// Touched but the same bytes (or a file the owner had removed from Akasha).
    Unchanged,
    Skipped,
    /// Still being written: try again shortly.
    Busy,
}

/// Import or update `rel` of `source`. Database failures are errors (the job retries);
/// problems with the file itself are outcomes.
pub async fn import(
    ctx: &JobContext,
    source: Arc<Source>,
    root: Arc<Path>,
    rel: String,
    seen: Seen,
    known: Option<SourceFile>,
) -> Result<Outcome, JobError> {
    if Utc::now() - seen.mtime < SETTLE {
        return Ok(Outcome::Busy);
    }
    let limit = ctx.config.max_upload_bytes();
    if seen.size > limit {
        let reason = format!(
            "larger than the upload limit of {} MiB",
            limit / (1024 * 1024)
        );
        return skip(ctx, &source, &rel, &seen, &reason).await;
    }
    let opened = {
        let (root, rel) = (Arc::clone(&root), rel.clone());
        tokio::task::spawn_blocking(move || paths::open_inside(&root, Path::new(&rel))).await?
    };
    let (file, meta) = match opened {
        Ok(opened) => opened,
        Err(OpenError::Unsafe) => return Ok(Outcome::Skipped),
        Err(OpenError::Io(err)) => {
            let reason = format!("{UNREADABLE}: {}", err.kind());
            return skip(ctx, &source, &rel, &seen, &reason).await;
        }
    };
    let mut file = tokio::fs::File::from_std(file);
    let file_name = name::sanitize(rel.rsplit('/').next().unwrap_or(&rel));
    let head = std::sync::Mutex::new(Vec::<u8>::new());
    let body = futures_util::stream::try_unfold(&mut file, |f| async move {
        let mut buf = vec![0; READ_CHUNK];
        let n = f.read(&mut buf).await?;
        buf.truncate(n);
        Ok::<_, std::io::Error>((n > 0).then(|| (Bytes::from(buf), f)))
    })
    .map_err(|err| Error::internal(format!("reading a watched file: {err}")))
    .inspect_ok(|chunk| {
        if let Ok(mut head) = head.lock()
            && head.len() < frontmatter::MAX_HEAD
        {
            let take = chunk.len().min(frontmatter::MAX_HEAD - head.len());
            head.extend_from_slice(&chunk[..take]);
        }
    })
    .boxed();
    let (blob, detected) = match receive(&ctx.storage, &file_name, limit, body).await {
        Ok(received) => received,
        Err(err) if is_file_problem(&err) => {
            return skip(ctx, &source, &rel, &seen, &err.message).await;
        }
        Err(err) => return Err(JobError::retry(err)),
    };
    // Changed while being read: drop it and take the settled version next time.
    let after = file.metadata().await?;
    if after.len() != meta.len()
        || after.modified().ok().map(micros) != Some(seen.mtime)
        || after.len() != seen.size
    {
        blob.discard().await;
        return Ok(Outcome::Busy);
    }
    let hash = blob.hash().to_hex();
    let tags = if source.import_tags && detected.mime == "text/markdown" {
        let head = head.lock().map(|h| h.clone()).unwrap_or_default();
        frontmatter::tags(&String::from_utf8_lossy(&head))
    } else {
        Vec::new()
    };

    let mut tx = ctx.db.begin().await?;
    let mut row = Row {
        source: &source,
        rel: &rel,
        seen: &seen,
        hash: &hash,
    };
    // Same bytes as last time (only the time changed), or a file the owner removed
    // from Akasha that did not really change: just remember the new time.
    if let Some(known) = &known
        && known.content_hash.as_deref() == Some(hash.as_str())
        && known.skip_reason.as_deref() != Some(QUOTA_REASON)
    {
        blob.discard().await;
        row.store(&mut tx, known.file_id, known.created_file, None)
            .await?;
        tx.commit().await?;
        return Ok(Outcome::Unchanged);
    }
    let owner = source.owner_id;
    let replace = known
        .as_ref()
        .filter(|k| k.created_file)
        .and_then(|k| k.file_id);
    let outcome = if let Some(file_id) = replace {
        match store::replace_in(&mut tx, owner, file_id, blob, detected.mime).await {
            Ok(Replaced::Updated(file)) => {
                add_tags(&mut tx, owner, file.id, &file.tags, &tags).await?;
                row.store(&mut tx, Some(file.id), true, None).await?;
                Outcome::Updated
            }
            Ok(Replaced::Duplicate(existing)) => {
                // The new bytes are another file the owner already has: map to it and
                // drop the old one unless something else still maps it.
                row.store(&mut tx, Some(existing.id), false, None).await?;
                super::scan::release_file(&mut tx, &source, file_id, &rel, true).await?;
                Outcome::Updated
            }
            Ok(Replaced::Gone) => {
                // The owner deleted it meanwhile: it stays out, like any file they delete.
                row.store(&mut tx, None, false, None).await?;
                Outcome::Unchanged
            }
            Err(err) => return quota_or_retry(ctx, tx, &source, &rel, &seen, err.0).await,
        }
    } else {
        save_new(&mut tx, &mut row, blob, &detected, &file_name, &tags).await?
    };
    tx.commit().await?;
    Ok(outcome)
}

/// Store a file that this folder has not imported before (or whose import is gone).
async fn save_new(
    tx: &mut sqlx::PgConnection,
    row: &mut Row<'_>,
    blob: akasha_storage::FinishedBlob,
    detected: &crate::files::sniff::Detected,
    file_name: &str,
    tags: &[String],
) -> Result<Outcome, JobError> {
    let owner = row.source.owner_id;
    match store::save_in(tx, owner, blob, file_name, detected.mime).await {
        Ok(Saved::Created(file)) => {
            add_tags(tx, owner, file.id, &file.tags, tags).await?;
            row.store(tx, Some(file.id), true, None).await?;
            Ok(Outcome::Imported)
        }
        Ok(Saved::Existing(file)) => {
            row.store(tx, Some(file.id), false, None).await?;
            Ok(Outcome::Unchanged)
        }
        Err(err) if err.0.code == ErrorCode::QuotaExceeded => {
            row.store(tx, None, false, Some(QUOTA_REASON)).await?;
            Ok(Outcome::Skipped)
        }
        Err(err) => Err(JobError::retry(err.0)),
    }
}

async fn quota_or_retry(
    ctx: &JobContext,
    tx: sqlx::Transaction<'_, sqlx::Postgres>,
    source: &Source,
    rel: &str,
    seen: &Seen,
    err: Error,
) -> Result<Outcome, JobError> {
    drop(tx);
    if err.code == ErrorCode::QuotaExceeded {
        return skip(ctx, source, rel, seen, QUOTA_REASON).await;
    }
    Err(JobError::retry(err))
}

/// The file is fine to skip (wrong type, empty, too large, not text).
fn is_file_problem(err: &Error) -> bool {
    matches!(
        err.code,
        ErrorCode::UnsupportedMediaType | ErrorCode::PayloadTooLarge | ErrorCode::BadRequest
    )
}

/// Remember that `rel` is not imported and why (keeps an earlier import's mapping).
async fn skip(
    ctx: &JobContext,
    source: &Source,
    rel: &str,
    seen: &Seen,
    reason: &str,
) -> Result<Outcome, JobError> {
    let mut conn = ctx.db.acquire().await?;
    let state = FileState {
        source_id: source.id,
        owner_id: source.owner_id,
        rel_path: rel,
        size_bytes: i64::try_from(seen.size).unwrap_or(i64::MAX),
        mtime: seen.mtime,
        content_hash: None,
        file_id: None,
        created_file: false,
        skip_reason: Some(reason),
    };
    sources::upsert_file(&mut conn, &state).await?;
    Ok(Outcome::Skipped)
}

/// Merge front-matter tags into the file's own tags.
async fn add_tags(
    conn: &mut sqlx::PgConnection,
    owner: Uuid,
    file_id: Uuid,
    current: &[String],
    tags: &[String],
) -> Result<(), sqlx::Error> {
    let mut merged = current.to_vec();
    for tag in tags {
        if !merged.contains(tag) && merged.len() < 50 {
            merged.push(tag.clone());
        }
    }
    if merged.len() != current.len() {
        let changes = files::FileChanges {
            tags: Some(&merged),
            ..Default::default()
        };
        files::update(&mut *conn, owner, file_id, changes).await?;
    }
    Ok(())
}

/// The `source_files` row being written.
struct Row<'a> {
    source: &'a Source,
    rel: &'a str,
    seen: &'a Seen,
    hash: &'a str,
}

impl Row<'_> {
    async fn store(
        &mut self,
        conn: &mut sqlx::PgConnection,
        file_id: Option<Uuid>,
        created_file: bool,
        skip_reason: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let mtime: DateTime<Utc> = self.seen.mtime;
        let state = FileState {
            source_id: self.source.id,
            owner_id: self.source.owner_id,
            rel_path: self.rel,
            size_bytes: i64::try_from(self.seen.size).unwrap_or(i64::MAX),
            mtime,
            content_hash: Some(self.hash),
            file_id,
            created_file,
            skip_reason,
        };
        sources::upsert_file(conn, &state).await
    }
}
