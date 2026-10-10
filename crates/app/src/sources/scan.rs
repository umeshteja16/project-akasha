//! The `scan_source` job: bring one watched folder's imports up to date.
//!
//! 1. Take the source's scan lease (paused or deleted sources are left alone; a scan
//!    already running means: try again shortly).
//! 2. Re-check the folder against the owner's watch roots (the admin may have changed
//!    them, or the folder may have been swapped for a symlink) and list it.
//! 3. Import new and changed files (size or modification time differ from the
//!    `source_files` row), a bounded batch per run with bounded concurrency; when more
//!    remain, queue a follow-up run and stop. Unchanged files cost one `stat`.
//! 4. After a complete pass with no unreadable entries, handle files that disappeared:
//!    a rename hands the import over to the new path, otherwise the file is deleted or
//!    kept as the source says. An empty folder with known files is treated as an
//!    unmounted volume and deletes nothing.
//!
//! Idempotent and resumable: every file is committed with its row in one transaction,
//! so a crashed or repeated run just finds less to do.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use akasha_db::{
    files,
    sources::{self, Source, SourceFile},
};
use akasha_jobs::{JobError, enqueue_delayed};
use futures_util::{StreamExt, stream};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    filter::Filter,
    import::{self, Outcome, QUOTA_REASON, UNREADABLE},
    paths,
    walk::{self, Listing, WalkError},
};
use crate::{
    activity::{self, ActivityKind},
    files::name,
    jobs::{
        JobContext, blobs,
        kinds::{ScanAllSources, ScanSource},
    },
};

/// Files imported per run before handing over to a follow-up run.
const BATCH_FILES: usize = 200;
/// Time spent importing per run before handing over.
const BATCH_TIME: Duration = Duration::from_secs(60);
/// Files read and stored at once.
const CONCURRENCY: usize = 4;
/// A scan lease older than this belongs to a crashed worker.
const LEASE_SECS: f64 = 15.0 * 60.0;
/// Retry delay when another scan holds the lease, or files were still changing.
const AGAIN: Duration = Duration::from_secs(15);

/// Counts of one pass (carried across follow-up runs).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScanStats {
    /// Matching files in the folder.
    pub files: u64,
    pub imported: u64,
    pub updated: u64,
    pub removed: u64,
    /// Not imported (unsupported content, too large, quota, symlinks).
    pub skipped: u64,
    /// Left for a later run (still changing or unreadable right now).
    pub pending: u64,
}

impl ScanStats {
    fn changed(&self) -> bool {
        self.imported + self.updated + self.removed > 0
    }
}

/// Why a scan stopped early.
enum Stop {
    /// A problem the owner or admin must fix (shown on the source).
    User(String),
    /// Database or storage trouble: the job is retried.
    Retry(JobError),
}

impl<E: Into<JobError>> From<E> for Stop {
    fn from(err: E) -> Self {
        Self::Retry(err.into())
    }
}

enum Done {
    Complete(ScanStats),
    More(ScanStats),
}

pub async fn scan_source(ctx: JobContext, job: ScanSource) -> Result<(), JobError> {
    let Some(source) = sources::acquire_scan(&ctx.db, job.source_id, LEASE_SECS).await? else {
        if sources::enabled(&ctx.db)
            .await?
            .iter()
            .any(|(id, _)| *id == job.source_id)
        {
            // Another scan is running: look again once it is likely done.
            let mut conn = ctx.db.acquire().await?;
            enqueue_delayed(&mut conn, &ScanSource::new(job.source_id), AGAIN).await?;
        }
        return Ok(());
    };
    let started = Instant::now();
    let result = run(&ctx, Arc::new(source.clone()), job.carried).await;
    let db = &ctx.db;
    match result {
        Ok(Done::Complete(stats)) => {
            let json = serde_json::to_value(stats)?;
            sources::finish_scan(db, source.id, "ok", None, &json, true).await?;
            tracing::info!(source = %source.id, ?stats, elapsed_ms = started.elapsed().as_millis() as u64, "folder scanned");
            if stats.changed() {
                record_sync(&ctx, &source, &stats).await;
            }
            if stats.pending > 0 {
                let mut conn = db.acquire().await?;
                enqueue_delayed(&mut conn, &ScanSource::new(source.id), AGAIN).await?;
            }
            Ok(())
        }
        Ok(Done::More(stats)) => {
            sources::finish_scan(
                db,
                source.id,
                "scanning",
                None,
                &serde_json::Value::Null,
                false,
            )
            .await?;
            let mut conn = db.acquire().await?;
            let next = ScanSource {
                source_id: source.id,
                carried: stats,
            };
            enqueue_delayed(&mut conn, &next, Duration::ZERO).await?;
            Ok(())
        }
        Err(Stop::User(message)) => {
            tracing::warn!(source = %source.id, %message, "folder scan stopped");
            sources::finish_scan(
                db,
                source.id,
                "error",
                Some(&message),
                &serde_json::Value::Null,
                false,
            )
            .await?;
            Ok(())
        }
        Err(Stop::Retry(err)) => {
            let message = "the last scan failed because of a server problem; it will be retried";
            sources::finish_scan(
                db,
                source.id,
                "error",
                Some(message),
                &serde_json::Value::Null,
                false,
            )
            .await?;
            Err(err)
        }
    }
}

async fn run(ctx: &JobContext, source: Arc<Source>, carried: ScanStats) -> Result<Done, Stop> {
    let root = check_folder(ctx, &source).await?;
    let filter = Filter::new(&source.include_globs, &source.exclude_globs).map_err(Stop::User)?;
    let listing: Listing = {
        let (root, filter) = (Arc::clone(&root), filter.clone());
        tokio::task::spawn_blocking(move || walk::list(&root, &filter))
            .await?
            .map_err(|err: WalkError| Stop::User(err.to_string()))?
    };
    let known: HashMap<String, SourceFile> = sources::files_of(&ctx.db, source.id)
        .await?
        .into_iter()
        .map(|row| (row.rel_path.clone(), row))
        .collect();

    let mut stats = ScanStats {
        files: listing.files.len() as u64,
        ..carried
    };
    let mut todo: Vec<(&String, &walk::Seen)> = listing
        .files
        .iter()
        .filter(|(rel, seen)| needs_import(known.get(*rel), seen))
        .collect();
    todo.sort_unstable_by(|a, b| a.0.cmp(b.0));
    let batch = todo.len().min(BATCH_FILES);
    let deadline = Instant::now() + BATCH_TIME;
    let mut work = Vec::with_capacity(batch);
    for (rel, seen) in todo.iter().take(batch) {
        let (source, root) = (Arc::clone(&source), Arc::clone(&root));
        let row = known.get(*rel).cloned();
        work.push(import::import(
            ctx,
            source,
            root,
            (*rel).clone(),
            (*seen).clone(),
            row,
        ));
    }
    let mut results = stream::iter(work).buffer_unordered(CONCURRENCY);
    let mut done = 0usize;
    while let Some(outcome) = results.next().await {
        done += 1;
        match outcome? {
            Outcome::Imported => stats.imported += 1,
            Outcome::Updated => stats.updated += 1,
            Outcome::Unchanged => {}
            Outcome::Skipped => stats.skipped += 1,
            Outcome::Busy => stats.pending += 1,
        }
        if Instant::now() > deadline {
            break;
        }
    }
    drop(results);
    if done < todo.len() {
        return Ok(Done::More(ScanStats {
            pending: 0,
            ..stats
        }));
    }

    let missing: Vec<&SourceFile> = known
        .values()
        .filter(|row| !listing.files.contains_key(&row.rel_path))
        .collect();
    if !missing.is_empty() {
        if listing.errors > 0 || stats.pending > 0 {
            tracing::info!(source = %source.id, "not removing files: the folder was not read completely");
        } else if listing.files.is_empty() {
            return Err(Stop::User(format!(
                "the folder is empty, so its {} imported files were kept; is the volume mounted?",
                missing.len()
            )));
        } else {
            for row in missing {
                stats.removed += u64::from(remove(ctx, &source, row).await?);
            }
        }
    }
    stats.skipped += listing.ignored;
    Ok(Done::Complete(stats))
}

/// Whether a listed file must be (re)imported.
fn needs_import(known: Option<&SourceFile>, seen: &walk::Seen) -> bool {
    let Some(row) = known else { return true };
    i64::try_from(seen.size).ok() != Some(row.size_bytes)
        || row.mtime != seen.mtime
        || row
            .skip_reason
            .as_deref()
            .is_some_and(|r| r == QUOTA_REASON || r.starts_with(UNREADABLE))
}

/// The source's folder, re-checked against the owner's watch roots right now.
async fn check_folder(ctx: &JobContext, source: &Source) -> Result<Arc<Path>, Stop> {
    let Some(user) = akasha_db::users::find_by_id(&ctx.db, source.owner_id).await? else {
        return Err(Stop::User("the owner no longer exists".into()));
    };
    let configured = ctx.config.watch_roots.clone();
    let path = PathBuf::from(&source.path);
    let checked = tokio::task::spawn_blocking(move || {
        let roots = paths::roots_for(&configured, user.id, &user.email);
        match std::fs::canonicalize(&path) {
            Ok(canonical) if canonical == path && paths::within(&canonical, &roots) => {
                Ok(canonical)
            }
            Ok(_) => Err("the folder is no longer inside the allowed watch roots".to_owned()),
            Err(_) if !paths::within(&path, &roots) => {
                Err("the folder is no longer inside the allowed watch roots".to_owned())
            }
            Err(err) => Err(format!(
                "the folder cannot be opened ({err}); is the volume mounted?"
            )),
        }
    })
    .await?;
    checked.map(Arc::from).map_err(Stop::User)
}

/// A file vanished from the folder. Returns whether a file left the library.
async fn remove(ctx: &JobContext, source: &Source, row: &SourceFile) -> Result<bool, JobError> {
    let mut tx = ctx.db.begin().await?;
    sources::delete_file_row(&mut tx, source.id, &row.rel_path).await?;
    let removed = match row.file_id {
        Some(file_id) => {
            release_file(&mut tx, source, file_id, &row.rel_path, row.created_file).await?
        }
        None => false,
    };
    tx.commit().await?;
    Ok(removed)
}

/// `rel` no longer maps `file_id`. If this source created the file: hand it to another
/// path that maps it (a rename: the file takes the new name), else delete it when the
/// source says so. Returns whether the file was deleted.
pub(super) async fn release_file(
    tx: &mut sqlx::PgConnection,
    source: &Source,
    file_id: Uuid,
    rel: &str,
    created: bool,
) -> Result<bool, JobError> {
    if !created {
        return Ok(false);
    }
    if let Some((other_source, other_rel)) =
        sources::other_mapping(tx, file_id, source.id, rel).await?
    {
        sources::set_created(tx, other_source, &other_rel).await?;
        if other_source == source.id {
            let new_name = name::sanitize(other_rel.rsplit('/').next().unwrap_or(&other_rel));
            let changes = files::FileChanges {
                original_name: Some(&new_name),
                ..Default::default()
            };
            files::update(&mut *tx, source.owner_id, file_id, changes).await?;
        }
        return Ok(false);
    }
    if source.on_delete != "delete" {
        return Ok(false);
    }
    let rows = files::delete_many(tx, source.owner_id, &[file_id]).await?;
    let hashes: Vec<String> = rows.iter().map(|r| r.content_hash.clone()).collect();
    blobs::release(tx, &hashes).await?;
    Ok(!rows.is_empty())
}

/// One activity event per pass that changed the library.
async fn record_sync(ctx: &JobContext, source: &Source, stats: &ScanStats) {
    let mut ev = akasha_db::activity::NewEvent::new(
        source.owner_id,
        ActivityKind::SourceSynced.as_str(),
        ActivityKind::SourceSynced.category().as_str(),
    );
    ev.subject = Some(&source.name);
    ev.details = serde_json::json!({
        "source_id": source.id,
        "imported": stats.imported,
        "updated": stats.updated,
        "removed": stats.removed,
    });
    activity::record_best_effort(&ctx.db, &ev).await;
}

/// The periodic job: queue a scan of every enabled source.
pub async fn scan_all(ctx: JobContext, _job: ScanAllSources) -> Result<(), JobError> {
    let enabled = sources::enabled(&ctx.db).await?;
    let mut conn = ctx.db.acquire().await?;
    for (id, _) in enabled {
        akasha_jobs::enqueue(&mut conn, &ScanSource::new(id)).await?;
    }
    Ok(())
}
