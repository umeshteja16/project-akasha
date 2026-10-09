//! Authentication: password hashing, session tokens and the [`AuthUser`] extractor.

pub mod password;
pub mod session;

use axum::{extract::FromRequestParts, http::request::Parts};
use axum_extra::extract::CookieJar;
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};
use akasha_core::Error;

/// The signed-in user. Add it as a handler argument to require authentication;
/// requests without a valid session get `401 unauthorized`.
#[derive(Debug, Clone, Copy)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub session_id: Uuid,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar
            .get(session::COOKIE_NAME)
            .map(|c| c.value().to_owned())
            .ok_or_else(|| Error::unauthorized("not signed in"))?;
        let found = akasha_db::sessions::touch(&state.db, &session::hash_token(&token)).await?;
        let session = found.ok_or_else(|| Error::unauthorized("session expired or invalid"))?;
        Ok(Self {
            user_id: session.user_id,
            session_id: session.session_id,
        })
    }
}
