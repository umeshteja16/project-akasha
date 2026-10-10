//! Authentication: password hashing, session tokens, API tokens and the
//! [`AuthUser`] / [`SessionUser`] extractors.

pub mod password;
pub mod session;
pub mod token;

use axum::{
    extract::FromRequestParts,
    http::{Method, request::Parts},
};
use axum_extra::extract::CookieJar;
use uuid::Uuid;

use self::token::{Grant, Scopes};
use crate::{error::ApiError, state::AppState};
use akasha_core::Error;

/// How a request proved who it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Credential {
    /// The browser session cookie (full access).
    Session { session_id: Uuid },
    /// A personal API token (`Authorization: Bearer`), limited to its scopes.
    Token { token_id: Uuid, scopes: Scopes },
}

/// The signed-in user. Add it as a handler argument to require authentication;
/// requests without a valid session cookie or API token get `401 unauthorized`.
///
/// Scope rule: a read-only token may only make safe requests (`GET`/`HEAD`);
/// anything else gets `403 forbidden` here, so handlers never forget the check.
#[derive(Debug, Clone, Copy)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub credential: Credential,
}

impl AuthUser {
    pub fn from_grant(grant: Grant) -> Self {
        Self {
            user_id: grant.user_id,
            credential: Credential::Token {
                token_id: grant.token_id,
                scopes: grant.scopes,
            },
        }
    }

    /// May this credential change data?
    pub fn can_write(&self) -> bool {
        match self.credential {
            Credential::Session { .. } => true,
            Credential::Token { scopes, .. } => scopes.write,
        }
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        // An Authorization header wins over the cookie (and must be valid).
        if let Some(bearer) = token::bearer(&parts.headers)? {
            let user = Self::from_grant(token::authenticate(state, bearer).await?);
            let safe = matches!(parts.method, Method::GET | Method::HEAD);
            if !safe && !user.can_write() {
                return Err(Error::forbidden("this API token is read-only").into());
            }
            return Ok(user);
        }
        let session = SessionUser::from_request_parts(parts, state).await?;
        Ok(Self {
            user_id: session.user_id,
            credential: Credential::Session {
                session_id: session.session_id,
            },
        })
    }
}

/// A user signed in with the session cookie. Account and token management take
/// this instead of [`AuthUser`], so an API token can never mint more tokens,
/// change the password or delete the account.
#[derive(Debug, Clone, Copy)]
pub struct SessionUser {
    pub user_id: Uuid,
    pub session_id: Uuid,
}

impl FromRequestParts<AppState> for SessionUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let jar = CookieJar::from_headers(&parts.headers);
        let Some(token) = jar.get(session::COOKIE_NAME).map(|c| c.value().to_owned()) else {
            return Err(if parts
                .headers
                .contains_key(axum::http::header::AUTHORIZATION)
            {
                Error::forbidden("API tokens cannot manage the account; sign in in the browser")
            } else {
                Error::unauthorized("not signed in")
            }
            .into());
        };
        let found = akasha_db::sessions::touch(&state.db, &session::hash_token(&token)).await?;
        let session = found.ok_or_else(|| Error::unauthorized("session expired or invalid"))?;
        Ok(Self {
            user_id: session.user_id,
            session_id: session.session_id,
        })
    }
}
