//! Rate limiting.
//!
//! - Credential endpoints: per client IP, to slow down password guessing. Keyed on the
//!   address [`crate::client_ip`] resolved (the TCP peer, or the client behind a trusted
//!   reverse proxy).
//! - Search: per signed-in user ([`UserLimiter`]), checked inside the handler once the
//!   session is known.

use std::{num::NonZeroU32, sync::Arc, time::Duration};

use axum::response::IntoResponse;
use governor::{
    Quota, RateLimiter,
    clock::{Clock, DefaultClock},
    middleware::NoOpMiddleware,
};
use tower_governor::{
    GovernorLayer,
    governor::{GovernorConfig, GovernorConfigBuilder},
};
use uuid::Uuid;

use crate::{client_ip::ClientIpKeyExtractor, error::ApiError};
use akasha_core::{Error, ErrorCode};

pub type AuthLimiter = Arc<GovernorConfig<ClientIpKeyExtractor, NoOpMiddleware>>;

/// Burst of 10 attempts, then one more every 6 seconds.
pub fn auth_limiter() -> AuthLimiter {
    let config = GovernorConfigBuilder::default()
        .key_extractor(ClientIpKeyExtractor)
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
    Arc::new(config.unwrap_or_else(|| unreachable!("non-zero rate limit constants")))
}

pub fn layer(limiter: &AuthLimiter) -> GovernorLayer<ClientIpKeyExtractor, NoOpMiddleware> {
    GovernorLayer {
        config: Arc::clone(limiter),
    }
}

/// Per-user limiter; `None` when unlimited.
pub type UserLimiter = Option<Arc<governor::DefaultKeyedRateLimiter<Uuid>>>;

/// `per_minute` requests per user per minute, as a burst refilled evenly; 0: unlimited.
pub fn user_limiter(per_minute: u32) -> UserLimiter {
    NonZeroU32::new(per_minute).map(|n| Arc::new(RateLimiter::keyed(Quota::per_minute(n))))
}

/// Count one request by `user`; `rate_limited` once the user is over the limit.
/// `what` names the requests in the error ("searches", "questions").
pub fn check_user(limiter: &UserLimiter, user: Uuid, what: &str) -> Result<(), Error> {
    let Some(limiter) = limiter else {
        return Ok(());
    };
    limiter.check_key(&user).map_err(|not_until| {
        let wait = not_until.wait_time_from(DefaultClock::default().now());
        Error::new(
            ErrorCode::RateLimited,
            format!("too many {what}, retry in {}s", wait.as_secs().max(1)),
        )
    })
}

/// [`check_user`], and on a hit note it in the user's security log (at most
/// once per 10 minutes, in the background).
pub fn check_user_audited(
    db: &akasha_db::PgPool,
    limiter: &UserLimiter,
    actor: crate::activity::Actor,
    what: &'static str,
) -> Result<(), Error> {
    check_user(limiter, actor.user_id, what).inspect_err(|_| {
        crate::activity::rate_limited(db, actor, what);
    })
}

/// Periodically forget idle clients so the limiters' memory stays bounded.
pub fn spawn_cleanup(limiter: AuthLimiter, users: Vec<UserLimiter>) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        loop {
            tick.tick().await;
            limiter.limiter().retain_recent();
            for users in users.iter().flatten() {
                users.retain_recent();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_limiter_counts_per_user() {
        let limiter = user_limiter(2);
        let (ada, bob) = (Uuid::new_v4(), Uuid::new_v4());
        assert!(check_user(&limiter, ada, "searches").is_ok());
        assert!(check_user(&limiter, ada, "searches").is_ok());
        let err = check_user(&limiter, ada, "searches").expect_err("third in a minute");
        assert_eq!(err.code, ErrorCode::RateLimited);
        assert!(
            check_user(&limiter, bob, "searches").is_ok(),
            "other users are unaffected"
        );
        let unlimited = user_limiter(0);
        assert!((0..100).all(|_| check_user(&unlimited, ada, "searches").is_ok()));
    }
}
