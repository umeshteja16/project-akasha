use std::sync::Arc;

use akasha_core::Config;
use akasha_db::PgPool;
use akasha_storage::Storage;

use crate::{
    jobs::ml::MlProvider,
    rate_limit::{self, AuthLimiter, UserLimiter},
};

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
    /// Embedding model and reranker, loaded on first use (shared with the worker).
    pub ml: Arc<MlProvider>,
}

impl AppState {
    pub fn new(db: PgPool, config: Config, storage: Storage) -> Self {
        Self {
            ml: Arc::new(MlProvider::from_config(&config)),
            search_limiter: rate_limit::user_limiter(config.search_rate_per_minute),
            db,
            config: Arc::new(config),
            storage,
            auth_limiter: rate_limit::auth_limiter(),
        }
    }
}
