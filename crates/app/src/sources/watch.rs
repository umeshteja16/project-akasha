//! Filesystem events for watched folders: a change queues a scan of its source a few
//! seconds after the folder goes quiet (debounced), so saved notes show up quickly.
//!
//! Runs in every worker process (`serve --with-worker`, `akasha worker`); scans are
//! deduplicated by the queue, so several workers watching the same folders is harmless.
//! Events are only a hint: the periodic scan (`AKASHA_WATCH_SCAN_MINUTES`) still finds
//! everything when the platform drops events or watch limits are reached
//! (`fs.inotify.max_user_watches`). Changes inside hidden folders (`.obsidian/`, which
//! Obsidian writes constantly) are ignored.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use akasha_db::{PgPool, sources};
use notify::{RecursiveMode, Watcher};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::jobs::kinds::ScanSource;

/// Quiet time after the last event before a scan is queued.
const DEBOUNCE: Duration = Duration::from_secs(3);
/// A folder that never goes quiet is scanned at least this often.
const MAX_DELAY: Duration = Duration::from_secs(30);
/// How often the list of enabled sources is re-read.
const REFRESH: Duration = Duration::from_secs(60);

/// Start watching enabled sources until `stop` resolves. No-op when the feature is off.
pub fn spawn(
    db: PgPool,
    config: &akasha_core::Config,
    stop: impl std::future::Future<Output = ()> + Send + 'static,
) {
    if config.watch_roots.is_empty() || !config.watch_fs_events {
        return;
    }
    tokio::spawn(async move {
        tokio::select! {
            () = run(db) => {}
            () = stop => {}
        }
    });
}

async fn run(db: PgPool) {
    let (tx, mut rx) = mpsc::unbounded_channel::<PathBuf>();
    let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(event) = event
            && !matches!(event.kind, notify::EventKind::Access(_))
        {
            for path in event.paths {
                let _ = tx.send(path);
            }
        }
    });
    let mut watcher = match watcher {
        Ok(watcher) => watcher,
        Err(err) => {
            tracing::warn!(%err, "filesystem events unavailable; watched folders rely on periodic scans");
            return;
        }
    };
    let mut watched: HashMap<Uuid, PathBuf> = HashMap::new();
    let mut pending = Debouncer::default();
    let mut refresh = tokio::time::interval(REFRESH);
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = refresh.tick() => sync_watches(&db, &mut watcher, &mut watched).await,
            Some(path) = rx.recv() => {
                if let Some(id) = source_of(&watched, &path) {
                    pending.touch(id, Instant::now());
                }
            }
            _ = tick.tick() => {
                for id in pending.due(Instant::now()) {
                    queue_scan(&db, id).await;
                }
            }
        }
    }
}

/// Watch newly enabled sources, stop watching removed or paused ones.
async fn sync_watches(
    db: &PgPool,
    watcher: &mut notify::RecommendedWatcher,
    watched: &mut HashMap<Uuid, PathBuf>,
) {
    let enabled = match sources::enabled(db).await {
        Ok(enabled) => enabled,
        Err(err) => {
            tracing::warn!(%err, "could not list watched folders");
            return;
        }
    };
    let gone: Vec<Uuid> = watched
        .keys()
        .filter(|id| !enabled.iter().any(|(e, _)| e == *id))
        .copied()
        .collect();
    for id in gone {
        if let Some(path) = watched.remove(&id) {
            let _ = watcher.unwatch(&path);
        }
    }
    for (id, path) in enabled {
        if watched.contains_key(&id) {
            continue;
        }
        let path = PathBuf::from(path);
        match watcher.watch(&path, RecursiveMode::Recursive) {
            Ok(()) => {
                watched.insert(id, path);
            }
            Err(err) => {
                // Remembered anyway so the warning is not repeated every minute.
                tracing::warn!(source = %id, %err, "cannot watch folder; it is still scanned periodically");
                watched.insert(id, path);
            }
        }
    }
}

/// The source a changed path belongs to, unless it is inside a hidden folder.
fn source_of(watched: &HashMap<Uuid, PathBuf>, path: &Path) -> Option<Uuid> {
    watched
        .iter()
        .filter_map(|(id, root)| Some((*id, path.strip_prefix(root).ok()?)))
        .find(|(_, rel)| {
            !rel.components()
                .any(|c| c.as_os_str().to_str().is_some_and(|s| s.starts_with('.')))
        })
        .map(|(id, _)| id)
}

async fn queue_scan(db: &PgPool, id: Uuid) {
    let result = match db.acquire().await {
        Ok(mut conn) => akasha_jobs::enqueue(&mut conn, &ScanSource::new(id))
            .await
            .map(|_| ())
            .map_err(|e| e.to_string()),
        Err(err) => Err(err.to_string()),
    };
    if let Err(err) = result {
        tracing::warn!(source = %id, %err, "could not queue a folder scan");
    }
}

/// Per-source debounce: first and last event times.
#[derive(Debug, Default)]
struct Debouncer {
    pending: HashMap<Uuid, (Instant, Instant)>,
}

impl Debouncer {
    fn touch(&mut self, id: Uuid, now: Instant) {
        self.pending
            .entry(id)
            .and_modify(|(_, last)| *last = now)
            .or_insert((now, now));
    }

    /// Sources quiet for [`DEBOUNCE`], or waiting longer than [`MAX_DELAY`].
    fn due(&mut self, now: Instant) -> Vec<Uuid> {
        let due: Vec<Uuid> = self
            .pending
            .iter()
            .filter(|(_, (first, last))| {
                now.duration_since(*last) >= DEBOUNCE || now.duration_since(*first) >= MAX_DELAY
            })
            .map(|(id, _)| *id)
            .collect();
        for id in &due {
            self.pending.remove(id);
        }
        due
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debounces_until_quiet_or_too_long() {
        let mut d = Debouncer::default();
        let id = Uuid::new_v4();
        let t0 = Instant::now();
        d.touch(id, t0);
        assert!(d.due(t0 + Duration::from_secs(1)).is_empty());
        d.touch(id, t0 + Duration::from_secs(2));
        assert!(d.due(t0 + Duration::from_secs(4)).is_empty());
        assert_eq!(d.due(t0 + Duration::from_secs(5)), [id]);
        assert!(d.due(t0 + Duration::from_secs(60)).is_empty());
        // A folder that never goes quiet.
        for s in 0..=30 {
            d.touch(id, t0 + Duration::from_secs(s));
        }
        assert_eq!(d.due(t0 + Duration::from_secs(30)), [id]);
    }

    #[test]
    fn maps_paths_to_sources_and_ignores_hidden_folders() {
        let id = Uuid::new_v4();
        let watched = HashMap::from([(id, PathBuf::from("/vault"))]);
        assert_eq!(source_of(&watched, Path::new("/vault/a/note.md")), Some(id));
        assert_eq!(
            source_of(&watched, Path::new("/vault/.obsidian/workspace.json")),
            None
        );
        assert_eq!(source_of(&watched, Path::new("/elsewhere/x.md")), None);
    }
}
