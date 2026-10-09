use std::sync::Arc;

use akasha_core::Config;
use akasha_db::PgPool;

use crate::rate_limit::{self, AuthLimiter};

/// Shared, cheaply clonable application state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    /// Per-IP limiter for the credential endpoints (login, register, password change).
    pub auth_limiter: AuthLimiter,
}

impl AppState {
    pub fn new(db: PgPool, config: Config) -> Self {
        Self {
            db,
            config: Arc::new(config),
            auth_limiter: rate_limit::auth_limiter(),
        }
    }
}
