//! The job types Akasha enqueues. Kinds are stored in the database: never rename
//! one that may still be queued.

use akasha_jobs::Job;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Delete a blob once no file references it. Enqueued in the same transaction that
/// deletes file rows, so a blob is only ever removed after that delete committed;
/// the handler re-checks under the per-hash lock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteBlobIfUnreferenced {
    /// SHA-256, lowercase hex.
    pub hash: String,
}

impl Job for DeleteBlobIfUnreferenced {
    const KIND: &'static str = "delete_blob_if_unreferenced";
    const MAX_ATTEMPTS: i32 = 10;

    fn dedupe_key(&self) -> Option<String> {
        Some(self.hash.clone())
    }
}

/// Extract text from a newly uploaded (or reindexed) file, store its chunks and
/// move its status pending → processing → ready/failed (see `jobs::extract`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractFile {
    pub file_id: Uuid,
}

impl Job for ExtractFile {
    const KIND: &'static str = "extract_file";

    fn dedupe_key(&self) -> Option<String> {
        Some(self.file_id.to_string())
    }
}

/// Embed the chunks of an extracted file that have no vector yet, then mark the
/// file `ready` (see `jobs::embed`). Enqueued by extraction and `akasha reembed`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedFile {
    pub file_id: Uuid,
}

impl Job for EmbedFile {
    const KIND: &'static str = "embed_file";

    fn dedupe_key(&self) -> Option<String> {
        Some(self.file_id.to_string())
    }
}

/// Delete expired login sessions (hourly).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PruneSessions {}

impl Job for PruneSessions {
    const KIND: &'static str = "prune_sessions";
}

/// Delete abandoned staged uploads (hourly).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PruneStaging {}

impl Job for PruneStaging {
    const KIND: &'static str = "prune_staging";
}

/// Find blobs no file references (e.g. left by a failed upload commit) and enqueue
/// [`DeleteBlobIfUnreferenced`] for each (daily).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SweepOrphanBlobs {}

impl Job for SweepOrphanBlobs {
    const KIND: &'static str = "sweep_orphan_blobs";
}

/// Delete old finished jobs (daily).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PruneJobs {}

impl Job for PruneJobs {
    const KIND: &'static str = "prune_jobs";
}

/// Render the thumbnail of an image blob (see `jobs::thumbnail`). Keyed by content
/// hash: files sharing bytes share one thumbnail. Enqueued on image upload and
/// reindex.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MakeThumbnail {
    /// SHA-256, lowercase hex.
    pub hash: String,
}

impl Job for MakeThumbnail {
    const KIND: &'static str = "make_thumbnail";

    fn dedupe_key(&self) -> Option<String> {
        Some(self.hash.clone())
    }
}

/// Ask the language model for a summary and suggested tags of a file (see
/// `jobs::enrich`). Enqueued when a file becomes `ready` (if a model is
/// configured) and by `POST /files/{id}/enrich` (`force`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichFile {
    pub file_id: Uuid,
    /// Run even if the summary already describes the current text.
    #[serde(default)]
    pub force: bool,
}

impl Job for EnrichFile {
    const KIND: &'static str = "enrich_file";
    /// Model calls cost money; a few retries cover outages and bad JSON.
    const MAX_ATTEMPTS: i32 = 3;

    fn dedupe_key(&self) -> Option<String> {
        // A forced re-run is not swallowed by a queued automatic one.
        Some(if self.force {
            format!("{}:force", self.file_id)
        } else {
            self.file_id.to_string()
        })
    }
}

/// Replace a conversation's question-based title with a model-written one
/// (see `jobs::title`). Enqueued with the first answered reply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TitleConversation {
    pub conversation_id: Uuid,
}

impl Job for TitleConversation {
    const KIND: &'static str = "title_conversation";
    const MAX_ATTEMPTS: i32 = 2;

    fn dedupe_key(&self) -> Option<String> {
        Some(self.conversation_id.to_string())
    }
}

/// Delete activity and security-log events older than
/// `AKASHA_ACTIVITY_RETENTION_DAYS` (daily; 0 keeps them).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PruneActivity {}

impl Job for PruneActivity {
    const KIND: &'static str = "prune_activity";
}

/// Bring one watched folder's imports up to date (see `sources::scan`). Queued when a
/// source is added or changed, by "Rescan now", by the filesystem watcher and by
/// [`ScanAllSources`]; a large folder queues follow-up runs carrying the counts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSource {
    pub source_id: Uuid,
    /// Counts of the pass so far (follow-up runs only).
    #[serde(default)]
    pub carried: crate::sources::scan::ScanStats,
}

impl ScanSource {
    pub fn new(source_id: Uuid) -> Self {
        Self {
            source_id,
            carried: Default::default(),
        }
    }
}

impl Job for ScanSource {
    const KIND: &'static str = "scan_source";
    const MAX_ATTEMPTS: i32 = 5;

    fn dedupe_key(&self) -> Option<String> {
        Some(self.source_id.to_string())
    }
}

/// Queue a [`ScanSource`] for every enabled watched folder (every
/// `AKASHA_WATCH_SCAN_MINUTES`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanAllSources {}

impl Job for ScanAllSources {
    const KIND: &'static str = "scan_all_sources";
}
