//! Runtime configuration.
//!
//! Sources, later ones win:
//! 1. built-in defaults,
//! 2. `akasha.toml` in the working directory (optional),
//! 3. environment variables prefixed with `AKASHA_` (e.g. `AKASHA_BIND_ADDR`),
//! 4. `DATABASE_URL`, the variable sqlx tooling already uses.

use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Serialize};

mod types;

pub use types::{LlmProvider, LogFormat, Secret, StorageBackend};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    /// Postgres connection string.
    pub database_url: String,
    /// Address the HTTP server listens on.
    pub bind_addr: String,
    /// Maximum Postgres connections in the pool.
    pub db_max_connections: u32,
    /// Log output format.
    pub log_format: LogFormat,
    /// Mark the session cookie `Secure` (HTTPS only). Turn on in production.
    pub cookie_secure: bool,
    /// Allow anyone who can reach the server to create an account.
    pub allow_registration: bool,
    /// How long a login session lasts, in days.
    pub session_ttl_days: u32,
    /// Largest accepted upload, in MiB.
    pub max_upload_mb: u64,
    /// Where uploaded file contents are kept.
    pub storage_backend: StorageBackend,
    /// Root directory for the `local` storage backend.
    pub storage_dir: String,
    /// Bucket name for the `s3` backend.
    pub storage_s3_bucket: Option<String>,
    /// Region for the `s3` backend (S3-compatible services usually accept any value).
    pub storage_s3_region: Option<String>,
    /// Custom endpoint for S3-compatible services (MinIO, R2, Garage, ...).
    pub storage_s3_endpoint: Option<String>,
    /// Access key; falls back to the standard `AWS_ACCESS_KEY_ID` when unset.
    pub storage_s3_access_key_id: Option<String>,
    /// Secret key; falls back to the standard `AWS_SECRET_ACCESS_KEY` when unset.
    pub storage_s3_secret_access_key: Option<Secret>,
    /// Allow plain-HTTP endpoints (local MinIO). Never enable for remote services.
    pub storage_s3_allow_http: bool,
    /// `akasha serve` also runs the background worker (single-box installs).
    /// Same as `akasha serve --with-worker`.
    pub serve_with_worker: bool,
    /// Background jobs a worker runs at the same time.
    pub worker_concurrency: u32,
    /// Seconds between checks for due jobs; new jobs wake workers at once (LISTEN/NOTIFY).
    pub worker_poll_secs: u64,
    /// Seconds without a heartbeat after which a running job is presumed lost and retried.
    pub worker_visibility_timeout_secs: u64,
    /// Seconds in-flight jobs get to finish on shutdown before they are handed back.
    pub worker_shutdown_grace_secs: u64,
    /// Recognise text in images and scanned PDF pages. Off: such files are stored
    /// and marked as needing OCR, with no text.
    pub ocr_enabled: bool,
    /// Directory for ML model files (OCR, embedding and rerank models are downloaded
    /// here on first use, or ahead of time with `akasha models download`).
    pub models_dir: String,
    /// Base URL OCR models are downloaded from. Empty: never download (offline
    /// installs put the files into `models_dir` themselves).
    pub ocr_models_url: String,
    /// Embedding model for semantic search (see `akasha_ml::catalog`). Changing it
    /// later needs `akasha reembed`.
    pub embed_model: String,
    /// Cross-encoder reranker for search results; empty or `none` disables it.
    pub rerank_model: String,
    /// Hugging Face compatible endpoint embedding/rerank models are downloaded from.
    /// Empty: never download (offline installs copy `models_dir` from elsewhere).
    pub ml_models_url: String,
    /// ONNX Runtime shared library. Empty: `ORT_DYLIB_PATH`, else `libonnxruntime.so`
    /// next to the binary or on the library search path.
    pub ort_dylib_path: String,
    /// CPU threads per model for inference; 0 uses every core.
    pub ml_threads: u32,
    /// Searches each user may run per minute (burst of the same size); 0: unlimited.
    pub search_rate_per_minute: u32,

    // Chat and language models (ADR 0012).
    /// Who writes chat answers: `ollama` (default, local), `anthropic`, `gemini`,
    /// `openai` (any OpenAI-compatible server) or `none` (chat returns passages only).
    pub llm_provider: LlmProvider,
    /// Model id for the provider; empty: the provider's default (OpenAI-compatible
    /// servers need one).
    pub llm_model: String,
    /// Only local language models: refuse to start with a cloud provider or a
    /// non-local server address.
    pub strict_offline: bool,
    /// Ollama server.
    pub ollama_url: String,
    /// Context window requested from Ollama, in tokens.
    pub ollama_num_ctx: u32,
    pub anthropic_api_key: Option<Secret>,
    pub anthropic_base_url: String,
    /// Claude `output_config.effort` (`low`, `medium`, `high`, ...); empty: the model default.
    pub anthropic_effort: String,
    pub gemini_api_key: Option<Secret>,
    pub gemini_base_url: String,
    /// Optional for local servers.
    pub openai_api_key: Option<Secret>,
    /// Including the version path, e.g. `http://localhost:1234/v1`.
    pub openai_base_url: String,
    /// Seconds to wait for a connection to the model server.
    pub llm_connect_timeout_secs: u64,
    /// Seconds of silence (no new output) after which a generation is abandoned.
    pub llm_read_timeout_secs: u64,
    /// Retries after connection errors, 429 and 5xx (before any output).
    pub llm_max_retries: u32,
    /// Longest answer, in tokens.
    pub llm_max_tokens: u32,
    /// Sampling temperature where the provider accepts one.
    pub llm_temperature: f32,
    /// Summarise and tag every file with the language model once it is indexed
    /// (one short model call per file; skipped without a model).
    pub llm_enrich_files: bool,
    /// Name conversations with the language model after their first answer (one
    /// short model call; without it the first question, shortened, stays the title).
    pub llm_conversation_titles: bool,
    /// Chat questions each user may ask per minute; 0: unlimited.
    pub chat_rate_per_minute: u32,
    /// Passages given to the model per answer.
    pub chat_context_chunks: u32,
    /// Earlier messages (user and assistant) sent along with a question.
    pub chat_history_messages: u32,
    /// Rewrite follow-up questions into standalone ones with the model before
    /// searching (one extra short model call per follow-up).
    pub chat_condense_question: bool,
    /// Refuse to answer ("not in your files") unless the best passage's reranker
    /// score reaches this. Unset: the reranker's calibrated default.
    pub chat_min_rerank_score: Option<f32>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database_url: "postgres://akasha:akasha@localhost:5432/akasha".into(),
            bind_addr: "0.0.0.0:8080".into(),
            db_max_connections: 10,
            log_format: LogFormat::Pretty,
            cookie_secure: false,
            allow_registration: true,
            session_ttl_days: 30,
            max_upload_mb: 512,
            storage_backend: StorageBackend::Local,
            storage_dir: "./storage".into(),
            storage_s3_bucket: None,
            storage_s3_region: None,
            storage_s3_endpoint: None,
            storage_s3_access_key_id: None,
            storage_s3_secret_access_key: None,
            storage_s3_allow_http: false,
            serve_with_worker: false,
            worker_concurrency: 4,
            worker_poll_secs: 5,
            worker_visibility_timeout_secs: 300,
            worker_shutdown_grace_secs: 30,
            ocr_enabled: true,
            models_dir: "./models".into(),
            ocr_models_url: "https://ocrs-models.s3-accelerate.amazonaws.com".into(),
            embed_model: "multilingual-e5-small".into(),
            rerank_model: "jina-reranker-v1-turbo-en".into(),
            ml_models_url: "https://huggingface.co".into(),
            ort_dylib_path: String::new(),
            ml_threads: 0,
            search_rate_per_minute: 30,
            llm_provider: LlmProvider::Ollama,
            llm_model: String::new(),
            strict_offline: false,
            ollama_url: "http://localhost:11434".into(),
            ollama_num_ctx: 8192,
            anthropic_api_key: None,
            anthropic_base_url: "https://api.anthropic.com".into(),
            anthropic_effort: "low".into(),
            gemini_api_key: None,
            gemini_base_url: "https://generativelanguage.googleapis.com".into(),
            openai_api_key: None,
            openai_base_url: "https://api.openai.com/v1".into(),
            llm_connect_timeout_secs: 10,
            llm_read_timeout_secs: 120,
            llm_max_retries: 2,
            llm_max_tokens: 1024,
            llm_temperature: 0.1,
            llm_enrich_files: true,
            llm_conversation_titles: true,
            chat_rate_per_minute: 20,
            chat_context_chunks: 8,
            chat_history_messages: 6,
            chat_condense_question: true,
            chat_min_rerank_score: None,
        }
    }
}

impl Config {
    /// The figment used by [`Config::load`]; exposed so tests can inspect sources.
    pub fn figment() -> Figment {
        Figment::from(Serialized::defaults(Config::default()))
            .merge(Toml::file("akasha.toml"))
            .merge(Env::prefixed("AKASHA_"))
            .merge(Env::raw().only(&["DATABASE_URL"]))
    }

    /// [`Config::max_upload_mb`] in bytes.
    pub fn max_upload_bytes(&self) -> u64 {
        self.max_upload_mb.saturating_mul(1024 * 1024)
    }

    /// Load configuration from all sources.
    pub fn load() -> Result<Self, Box<figment::Error>> {
        Self::figment().extract().map_err(Box::new)
    }
}

#[cfg(test)]
// figment's Jail API returns its large error type by value.
#[allow(clippy::result_large_err)]
mod tests {
    use super::*;

    #[test]
    fn defaults_apply_without_any_source() {
        figment::Jail::expect_with(|_jail| {
            assert_eq!(Config::figment().extract::<Config>()?, Config::default());
            Ok(())
        });
    }

    #[test]
    fn env_overrides_toml_and_database_url_is_read_raw() {
        figment::Jail::expect_with(|jail| {
            jail.create_file("akasha.toml", r#"bind_addr = "127.0.0.1:1""#)?;
            jail.set_env("AKASHA_BIND_ADDR", "127.0.0.1:2");
            jail.set_env("AKASHA_LOG_FORMAT", "json");
            jail.set_env("DATABASE_URL", "postgres://x@y/z");

            let config = Config::figment().extract::<Config>()?;
            assert_eq!(config.bind_addr, "127.0.0.1:2");
            assert_eq!(config.log_format, LogFormat::Json);
            assert_eq!(config.database_url, "postgres://x@y/z");
            Ok(())
        });
    }

    #[test]
    fn storage_settings_come_from_env_and_secrets_are_redacted() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("AKASHA_STORAGE_BACKEND", "s3");
            jail.set_env("AKASHA_STORAGE_S3_BUCKET", "akasha");
            jail.set_env("AKASHA_STORAGE_S3_SECRET_ACCESS_KEY", "hunter2");

            let config = Config::figment().extract::<Config>()?;
            assert_eq!(config.storage_backend, StorageBackend::S3);
            assert_eq!(config.storage_s3_bucket.as_deref(), Some("akasha"));
            let secret = config.storage_s3_secret_access_key.clone();
            assert_eq!(secret.as_ref().map(Secret::expose), Some("hunter2"));
            assert!(!format!("{config:?}").contains("hunter2"));
            Ok(())
        });
    }

    #[test]
    fn llm_settings_come_from_env_and_keys_are_redacted() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("AKASHA_LLM_PROVIDER", "anthropic");
            jail.set_env("AKASHA_ANTHROPIC_API_KEY", "sk-ant-hunter2");
            jail.set_env("AKASHA_STRICT_OFFLINE", "true");
            jail.set_env("AKASHA_CHAT_MIN_RERANK_SCORE", "0.25");

            let config = Config::figment().extract::<Config>()?;
            assert_eq!(config.llm_provider, LlmProvider::Anthropic);
            assert!(config.strict_offline);
            assert_eq!(config.chat_min_rerank_score, Some(0.25));
            let key = config.anthropic_api_key.as_ref().map(Secret::expose);
            assert_eq!(key, Some("sk-ant-hunter2"));
            assert!(!format!("{config:?}").contains("hunter2"));
            Ok(())
        });
    }
}
