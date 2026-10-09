//! Per-client-IP rate limiting for credential endpoints, to slow down password guessing.
//!
//! Keyed on the TCP peer address. Behind a reverse proxy every request shares the proxy's
//! IP; trusting `X-Forwarded-For` is a planned config option (see PROGRESS.md).

use std::{sync::Arc, time::Duration};

use axum::response::IntoResponse;
use governor::middleware::NoOpMiddleware;
use tower_governor::{
    GovernorLayer,
    governor::{GovernorConfig, GovernorConfigBuilder},
    key_extractor::PeerIpKeyExtractor,
};

use crate::error::ApiError;
use akasha_core::{Error, ErrorCode};

pub type AuthLimiter = Arc<GovernorConfig<PeerIpKeyExtractor, NoOpMiddleware>>;

/// Burst of 10 attempts, then one more every 6 seconds.
pub fn auth_limiter() -> AuthLimiter {
    let config = GovernorConfigBuilder::default()
        .per_second(6)
        .burst_size(10)
        .error_handler(|err| {
            let error = match err {
                tower_governor::GovernorError::TooManyRequests { wait_time, .. } => Error::new(
                    ErrorCode::RateLimited,
                    format!("too many attempts, retry in {wait_time}s"),
                ),
                _ => Error::internal("rate limiter could not identify the client"),
            };
            ApiError(error).into_response()
        })
        .finish();
    // `finish` only fails for a zero period or burst, which the constants above rule out.
    Arc::new(config.unwrap_or_default())
}

pub fn layer(limiter: &AuthLimiter) -> GovernorLayer<PeerIpKeyExtractor, NoOpMiddleware> {
    GovernorLayer {
        config: Arc::clone(limiter),
    }
}

/// Periodically forget idle clients so the limiter's memory stays bounded.
pub fn spawn_cleanup(limiter: AuthLimiter) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        loop {
            tick.tick().await;
            limiter.limiter().retain_recent();
        }
    });
}
