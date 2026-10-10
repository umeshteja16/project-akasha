//! `/api/v1/system/status`: which models and services this server runs with,
//! for the Settings → System panel. Signed-in users only; never includes
//! secrets (API keys, database or storage credentials) or other users' data.

use std::path::{Path, PathBuf};

use akasha_core::{Config, LlmProvider};
use akasha_ingest::models as ocr_models;
use akasha_ml::catalog::{self, EmbedBackend, RerankBackend};
use axum::extract::State;
use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    auth::AuthUser,
    error::{ApiError, ErrorBody},
    extract::Json,
    jobs::ml::ModelState,
    state::AppState,
};

/// Claimable jobs waiting longer than this with nothing running mean no
/// worker is taking them.
const STALLED_AFTER_SECS: f64 = 120.0;

/// How a component stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ComponentStatus {
    /// Loaded / available.
    Ready,
    /// Loads on first use (nothing has needed it yet).
    NotLoaded,
    /// Model files are fetched on first use.
    DownloadsOnFirstUse,
    /// Tried and failed, or required files are missing.
    Unavailable,
    /// Turned off in the configuration.
    Disabled,
    /// Not needed by the configured models.
    NotNeeded,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ModelStatus {
    /// Configured model name; `null` when this kind of model is turned off.
    pub name: Option<String>,
    pub status: ComponentStatus,
    /// Why it is unavailable, when it is.
    pub detail: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EmbeddingStatus {
    #[serde(flatten)]
    pub model: ModelStatus,
    pub dimensions: u32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ChatModelStatus {
    /// `ollama`, `anthropic`, `gemini`, `openai`, `fake` or `none`.
    pub provider: String,
    /// The model answers come from; empty without a provider.
    pub model: String,
    /// `true` when questions and passages never leave this machine or network.
    pub local: bool,
    pub status: ComponentStatus,
}

/// The relevance floor search uses for results found by meaning alone.
#[derive(Debug, Serialize, ToSchema)]
pub struct RelevanceStatus {
    /// Minimum cosine similarity (configured or the model's default).
    pub min_similarity: Option<f32>,
    /// Minimum reranker score (configured or the reranker's default).
    pub min_rerank_score: Option<f32>,
}

/// `ok`: working through jobs or nothing to do; `stalled`: jobs wait and no
/// worker takes them (start one with `akasha worker` or `serve --with-worker`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkerHealth {
    Busy,
    Idle,
    Stalled,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WorkerStatus {
    pub health: WorkerHealth,
    /// This process runs a worker (`serve --with-worker`); others may run elsewhere.
    pub in_process: bool,
    /// Jobs ready to run now (queue depth).
    pub queued: i64,
    pub running: i64,
    /// Waiting to retry after a failure.
    pub retrying: i64,
    /// Jobs given up on in the last 24 hours.
    pub failed_last_day: i64,
    /// How long the oldest ready job has waited, in seconds.
    pub oldest_wait_secs: Option<u64>,
    pub last_finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SystemStatus {
    pub version: &'static str,
    /// Cloud language models are refused (`AKASHA_STRICT_OFFLINE`).
    pub strict_offline: bool,
    pub embedding: EmbeddingStatus,
    pub reranker: ModelStatus,
    pub chat: ChatModelStatus,
    /// ONNX Runtime, which the real embedding and rerank models run on.
    pub onnx_runtime: ModelStatus,
    /// Text recognition for images and scanned PDFs.
    pub ocr: ModelStatus,
    /// Speech recognition for audio and video (Whisper).
    pub transcription: ModelStatus,
    pub relevance: RelevanceStatus,
    pub worker: WorkerStatus,
}

/// Which models and services this server runs with, and the job queue.
#[utoipa::path(
    get, path = "/api/v1/system/status", tag = "meta",
    responses((status = 200, body = SystemStatus), (status = 401, body = ErrorBody)),
    security(("session_cookie" = []))
)]
pub async fn status(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Result<Json<SystemStatus>, ApiError> {
    let config = &state.config;
    let embed = catalog::embed_model(&config.embed_model).ok();
    let rerank = catalog::rerank_model(&config.rerank_model).ok().flatten();
    let embedder_state = state.ml.embedder_state();
    let reranker_state = state.ml.reranker_state();

    let embedding = EmbeddingStatus {
        model: model_status(Some(config.embed_model.clone()), &embedder_state),
        dimensions: embed
            .map(|m| u32::try_from(m.dim).unwrap_or(0))
            .unwrap_or(0),
    };
    let reranker = match rerank {
        Some(m) => model_status(Some(m.name.to_owned()), &reranker_state),
        None => ModelStatus {
            name: None,
            status: ComponentStatus::Disabled,
            detail: None,
        },
    };
    let needs_onnx = embed.is_some_and(|m| matches!(m.backend, EmbedBackend::Onnx { .. }))
        || rerank.is_some_and(|m| matches!(m.backend, RerankBackend::Onnx(_)));
    let onnx_loaded = (embed.is_some_and(|m| matches!(m.backend, EmbedBackend::Onnx { .. }))
        && embedder_state == ModelState::Ready)
        || (rerank.is_some_and(|m| matches!(m.backend, RerankBackend::Onnx(_)))
            && reranker_state == ModelState::Ready);

    let stats =
        akasha_db::system::queue_stats(&state.db, config.worker_visibility_timeout_secs as f64)
            .await?;
    let oldest = stats.oldest_ready_secs.map(|s| s.max(0.0) as u64);
    let health = if stats.running > stats.stale && stats.running > 0 {
        WorkerHealth::Busy
    } else if stats.stale > 0
        || stats
            .oldest_ready_secs
            .is_some_and(|s| s > STALLED_AFTER_SECS)
    {
        WorkerHealth::Stalled
    } else {
        WorkerHealth::Idle
    };

    Ok(Json(SystemStatus {
        version: env!("CARGO_PKG_VERSION"),
        strict_offline: config.strict_offline,
        embedding,
        reranker,
        chat: chat_status(&state),
        onnx_runtime: onnx_status(config, needs_onnx, onnx_loaded),
        ocr: ocr_status(config),
        transcription: transcription_status(config),
        relevance: RelevanceStatus {
            min_similarity: config
                .search_min_similarity
                .or(embed.map(|m| m.min_similarity)),
            min_rerank_score: config
                .search_min_rerank_score
                .or(rerank.map(|m| m.min_score)),
        },
        worker: WorkerStatus {
            health,
            in_process: config.serve_with_worker,
            queued: stats.ready,
            running: stats.running,
            retrying: stats.retrying,
            failed_last_day: stats.dead_recent,
            oldest_wait_secs: oldest,
            last_finished_at: stats.last_finished_at,
        },
    }))
}

fn model_status(name: Option<String>, state: &ModelState) -> ModelStatus {
    let (status, detail) = match state {
        ModelState::Ready => (ComponentStatus::Ready, None),
        ModelState::NotLoaded => (ComponentStatus::NotLoaded, None),
        ModelState::Failed(why) => (ComponentStatus::Unavailable, Some(why.clone())),
    };
    ModelStatus {
        name,
        status,
        detail,
    }
}

fn chat_status(state: &AppState) -> ChatModelStatus {
    let config = &state.config;
    let local_url = akasha_llm::local::is_local_url;
    let (provider, local) = match config.llm_provider {
        LlmProvider::None => ("none", true),
        LlmProvider::Ollama => ("ollama", local_url(&config.ollama_url)),
        LlmProvider::Anthropic => ("anthropic", false),
        LlmProvider::Gemini => ("gemini", false),
        LlmProvider::Openai => ("openai", local_url(&config.openai_base_url)),
        LlmProvider::Fake => ("fake", true),
    };
    match &state.llm {
        Some(m) => ChatModelStatus {
            provider: provider.to_owned(),
            model: m.model().to_owned(),
            local,
            status: ComponentStatus::Ready,
        },
        None => ChatModelStatus {
            provider: provider.to_owned(),
            model: String::new(),
            local,
            status: if config.llm_provider == LlmProvider::None {
                ComponentStatus::Disabled
            } else {
                ComponentStatus::Unavailable
            },
        },
    }
}

fn onnx_status(config: &Config, needed: bool, loaded: bool) -> ModelStatus {
    let library = runtime_library(&config.ort_dylib_path);
    let name = Some(
        library
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
    );
    let (status, detail) = if !needed {
        (ComponentStatus::NotNeeded, None)
    } else if loaded {
        (ComponentStatus::Ready, None)
    } else if library.components().count() > 1 && !library.exists() {
        (
            ComponentStatus::Unavailable,
            Some("the ONNX Runtime library was not found (run `just onnxruntime` or set AKASHA_ORT_DYLIB_PATH)".to_owned()),
        )
    } else {
        (ComponentStatus::NotLoaded, None)
    };
    ModelStatus {
        name,
        status,
        detail,
    }
}

#[cfg(feature = "onnx")]
fn runtime_library(configured: &str) -> PathBuf {
    akasha_ml::onnx::runtime_library(configured)
}

#[cfg(not(feature = "onnx"))]
fn runtime_library(configured: &str) -> PathBuf {
    PathBuf::from(configured)
}

fn transcription_status(config: &Config) -> ModelStatus {
    use akasha_media::models::{self, ModelChoice};
    let name = Some(format!("whisper {}", config.whisper_model.trim()));
    let (status, detail) = if !config.transcribe_enabled {
        (ComponentStatus::Disabled, None)
    } else {
        match models::find(&config.whisper_model) {
            Err(e) => (ComponentStatus::Unavailable, Some(e.to_string())),
            Ok(ModelChoice::Fake) => (ComponentStatus::Ready, None),
            Ok(ModelChoice::Whisper(_)) if !akasha_media::WHISPER_AVAILABLE => (
                ComponentStatus::Unavailable,
                Some("this build has no Whisper support".to_owned()),
            ),
            Ok(ModelChoice::Whisper(m)) => {
                if models::path(Path::new(&config.models_dir), m).is_file() {
                    (ComponentStatus::Ready, None)
                } else if !config.ml_models_url.trim().is_empty() {
                    (ComponentStatus::DownloadsOnFirstUse, None)
                } else {
                    (
                        ComponentStatus::Unavailable,
                        Some("the Whisper model file is missing and downloads are off".to_owned()),
                    )
                }
            }
        }
    };
    ModelStatus {
        name,
        status,
        detail,
    }
}

fn ocr_status(config: &Config) -> ModelStatus {
    let dir = Path::new(&config.models_dir);
    let present = [ocr_models::DETECTION, ocr_models::RECOGNITION]
        .iter()
        .all(|m| dir.join(m.name).is_file());
    let (status, detail) = if !config.ocr_enabled {
        (ComponentStatus::Disabled, None)
    } else if present {
        (ComponentStatus::Ready, None)
    } else if !config.ocr_models_url.trim().is_empty() {
        (ComponentStatus::DownloadsOnFirstUse, None)
    } else {
        (
            ComponentStatus::Unavailable,
            Some("OCR model files are missing and downloads are off".to_owned()),
        )
    };
    ModelStatus {
        name: Some("ocrs".to_owned()),
        status,
        detail,
    }
}
