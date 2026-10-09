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

/// Extract text from a newly uploaded (or reindexed) file and move its status
/// pending → processing → ready/failed.
///
/// **No handler is registered yet** (extraction is step 2.5). Workers never claim
/// kinds they have no handler for, so these jobs wait safely in `queued` and the
/// files stay `pending` until a worker that can extract starts. A no-op handler
/// would have marked them done without extracting anything.
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
