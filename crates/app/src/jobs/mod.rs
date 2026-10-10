//! Akasha's background jobs: the typed kinds, their handlers, the periodic
//! schedule and how to build a worker. The queue itself is `akasha-jobs`.
//!
//! Handlers must be idempotent (delivery is at least once): re-check state in the
//! database, never trust that a payload still describes reality.

pub mod blobs;
mod embed;
mod enrich;
mod extract;
pub mod kinds;
mod maintenance;
mod media;
pub mod ml;
pub mod ocr;
pub mod thumbnail;
mod title;
pub mod transcribe;

use std::{sync::Arc, time::Duration};

use akasha_core::Config;
use akasha_db::PgPool;
use akasha_jobs::{QueueError, Registry, Schedule, Worker, WorkerConfig};
use akasha_llm::ChatModel;
use akasha_storage::Storage;

use self::kinds::{
    PruneActivity, PruneJobs, PruneSessions, PruneStaging, ScanAllSources, SweepOrphanBlobs,
};
use self::{ml::MlProvider, ocr::OcrProvider, transcribe::TranscriberProvider};
use crate::state::AppState;

const HOUR: Duration = Duration::from_secs(60 * 60);
const DAY: Duration = Duration::from_secs(24 * 60 * 60);

/// What every handler gets.
#[derive(Clone)]
pub struct JobContext {
    pub db: PgPool,
    pub storage: Storage,
    /// The OCR engine, loaded on first use (shared by all handlers of a worker).
    pub ocr: Arc<OcrProvider>,
    /// The speech model for audio and video, loaded on first use.
    pub transcriber: Arc<TranscriberProvider>,
    /// Embedding model and reranker, loaded on first use (shared with the API).
    pub ml: Arc<MlProvider>,
    /// The language model for summaries, tags and titles; `None`: those jobs
    /// are not queued (and skip if they run anyway).
    pub llm: Option<Arc<dyn ChatModel>>,
    pub config: Arc<Config>,
}

impl JobContext {
    /// A context without a language model (add one with [`Self::with_llm`]).
    pub fn new(db: PgPool, storage: Storage, config: &Config) -> Self {
        Self {
            db,
            storage,
            ocr: Arc::new(OcrProvider::from_config(config)),
            transcriber: Arc::new(TranscriberProvider::from_config(config)),
            ml: Arc::new(MlProvider::from_config(config)),
            llm: None,
            config: Arc::new(config.clone()),
        }
    }

    pub fn with_llm(mut self, llm: Option<Arc<dyn ChatModel>>) -> Self {
        self.llm = llm;
        self
    }

    /// Files get a model-written summary and tags once indexed.
    pub fn enriches_files(&self) -> bool {
        self.llm.is_some() && self.config.llm_enrich_files
    }
}

impl From<&AppState> for JobContext {
    fn from(state: &AppState) -> Self {
        Self {
            ml: Arc::clone(&state.ml),
            llm: state.llm.clone(),
            config: Arc::clone(&state.config),
            ..Self::new(state.db.clone(), state.storage.clone(), &state.config)
        }
    }
}

/// Every kind this binary can run.
pub fn registry() -> Registry<JobContext> {
    Registry::new()
        .register(extract::extract_file)
        .register(embed::embed_file)
        .register(enrich::enrich_file)
        .register(title::title_conversation)
        .register(thumbnail::make_thumbnail)
        .register(blobs::delete_if_unreferenced)
        .register(blobs::sweep_orphans)
        .register(maintenance::prune_sessions)
        .register(maintenance::prune_staging)
        .register(maintenance::prune_jobs)
        .register(maintenance::prune_activity)
        .register(crate::sources::scan::scan_source)
        .register(crate::sources::scan::scan_all)
}

pub fn schedules(config: &Config) -> Result<Vec<Schedule>, QueueError> {
    let mut schedules = vec![
        Schedule::new("prune-sessions", HOUR, &PruneSessions {})?,
        Schedule::new("prune-staging", HOUR, &PruneStaging {})?,
        Schedule::new("sweep-orphan-blobs", DAY, &SweepOrphanBlobs {})?,
        Schedule::new("prune-jobs", DAY, &PruneJobs {})?,
        Schedule::new("prune-activity", DAY, &PruneActivity {})?,
    ];
    if !config.watch_roots.is_empty() && config.watch_scan_minutes > 0 {
        let every = Duration::from_secs(config.watch_scan_minutes.saturating_mul(60));
        schedules.push(Schedule::new("scan-sources", every, &ScanAllSources {})?);
    }
    Ok(schedules)
}

/// A worker configured from `config`, with every handler and schedule.
pub fn worker(ctx: JobContext, config: &Config) -> Result<Worker<JobContext>, QueueError> {
    let tuning = WorkerConfig {
        concurrency: usize::try_from(config.worker_concurrency).unwrap_or(1),
        poll_interval: Duration::from_secs(config.worker_poll_secs.max(1)),
        visibility_timeout: Duration::from_secs(config.worker_visibility_timeout_secs.max(3)),
        shutdown_grace: Duration::from_secs(config.worker_shutdown_grace_secs),
    };
    let db = ctx.db.clone();
    Ok(Worker::new(db, ctx, registry(), tuning).with_schedules(schedules(config)?))
}
