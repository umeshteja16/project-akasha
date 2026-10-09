use std::sync::Arc;

use akasha_core::Config;
use akasha_db::PgPool;
use akasha_llm::ChatModel;
use akasha_storage::Storage;

use crate::{
    jobs::ml::MlProvider,
    rate_limit::{self, AuthLimiter, UserLimiter},
    web::WebAssets,
};

/// `POST /files/{id}/enrich` calls per user per minute.
const ENRICH_PER_MINUTE: u32 = 10;

/// Shared, cheaply clonable application state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    /// Content-addressed blob store for uploaded files.
    pub storage: Storage,
    /// Per-IP limiter for the credential endpoints (login, register, password change).
    pub auth_limiter: AuthLimiter,
    /// Per-user limiter for search (`AKASHA_SEARCH_RATE_PER_MINUTE`).
    pub search_limiter: UserLimiter,
    /// Per-user limiter for chat questions (`AKASHA_CHAT_RATE_PER_MINUTE`).
    pub chat_limiter: UserLimiter,
    /// Per-user limiter for re-running file enrichment (model calls cost money).
    pub enrich_limiter: UserLimiter,
    /// Embedding model and reranker, loaded on first use (shared with the worker).
    pub ml: Arc<MlProvider>,
    /// The chat model; `None`: chat answers with passages only.
    pub llm: Option<Arc<dyn ChatModel>>,
    /// The built web UI (embedded with feature `embed-ui`; otherwise none).
    pub web: Arc<WebAssets>,
}

impl AppState {
    /// State with the configured chat model. A model that cannot be built is
    /// logged and left out (`serve` checks it first with [`crate::llm::build`]).
    pub fn new(db: PgPool, config: Config, storage: Storage) -> Self {
        let llm = crate::llm::build(&config).unwrap_or_else(|err| {
            tracing::error!(%err, "chat model unavailable");
            None
        });
        Self::with_llm(db, config, storage, llm)
    }

    pub fn with_llm(
        db: PgPool,
        config: Config,
        storage: Storage,
        llm: Option<Arc<dyn ChatModel>>,
    ) -> Self {
        Self {
            ml: Arc::new(MlProvider::from_config(&config)),
            llm,
            web: Arc::new(WebAssets::embedded()),
            search_limiter: rate_limit::user_limiter(config.search_rate_per_minute),
            chat_limiter: rate_limit::user_limiter(config.chat_rate_per_minute),
            enrich_limiter: rate_limit::user_limiter(ENRICH_PER_MINUTE),
            db,
            config: Arc::new(config),
            storage,
            auth_limiter: rate_limit::auth_limiter(),
        }
    }
}
