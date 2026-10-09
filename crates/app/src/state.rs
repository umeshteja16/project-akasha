use std::sync::Arc;

use akasha_core::Config;
use akasha_db::PgPool;
use akasha_storage::Storage;

use crate::rate_limit::{self, AuthLimiter};

/// Shared, cheaply clonable application state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    /// Content-addressed blob store for uploaded files.
    pub storage: Storage,
    /// Per-IP limiter for the credential endpoints (login, register, password change).
    pub auth_limiter: AuthLimiter,
}

impl AppState {
    pub fn new(db: PgPool, config: Config, storage: Storage) -> Self {
        Self {
            db,
            config: Arc::new(config),
            storage,
            auth_limiter: rate_limit::auth_limiter(),
        }
    }
}
