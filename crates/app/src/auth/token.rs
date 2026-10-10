//! Personal API tokens for machine clients (MCP, scripts).
//!
//! A token is `akasha_pat_` followed by 32 random bytes (base64url). Like session
//! cookies, only its SHA-256 is stored. Clients send it as
//! `Authorization: Bearer akasha_pat_…`.

use argon2::password_hash::rand_core::{OsRng, RngCore};
use axum::http::{HeaderMap, header::AUTHORIZATION};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use uuid::Uuid;

use super::session::hash_token;
use crate::state::AppState;
use akasha_core::Error;

/// Every token starts with this, so leaked tokens are easy to recognise (and to
/// scan for in repositories).
pub const PREFIX: &str = "akasha_pat_";
/// Characters of the token kept in the clear to tell tokens apart.
const SHOWN_CHARS: usize = PREFIX.len() + 4;

/// What a token may do. `write` always comes with `read`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scopes {
    pub write: bool,
}

impl Scopes {
    pub const READ: Self = Self { write: false };
    pub const READ_WRITE: Self = Self { write: true };

    pub fn from_db(scopes: &[String]) -> Self {
        Self {
            write: scopes.iter().any(|s| s == "write"),
        }
    }

    pub fn to_db(self) -> Vec<String> {
        let mut out = vec!["read".to_owned()];
        if self.write {
            out.push("write".to_owned());
        }
        out
    }
}

/// A new token: the secret (shown once), its hash and its visible prefix.
pub struct NewSecret {
    pub secret: String,
    pub hash: [u8; 32],
    pub prefix: String,
}

pub fn generate() -> NewSecret {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let secret = format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes));
    let hash = hash_token(&secret);
    let prefix = secret.chars().take(SHOWN_CHARS).collect();
    NewSecret {
        secret,
        hash,
        prefix,
    }
}

/// A request authenticated by a token.
#[derive(Debug, Clone, Copy)]
pub struct Grant {
    pub token_id: Uuid,
    pub user_id: Uuid,
    pub scopes: Scopes,
}

/// The bearer token in `headers`, if any. `Err` when the header is there but is
/// not a bearer token.
pub fn bearer(headers: &HeaderMap) -> Result<Option<&str>, Error> {
    let Some(value) = headers.get(AUTHORIZATION) else {
        return Ok(None);
    };
    let value = value
        .to_str()
        .map_err(|_| Error::unauthorized("invalid Authorization header"))?;
    let (scheme, token) = value
        .split_once(' ')
        .ok_or_else(|| Error::unauthorized("expected `Authorization: Bearer <token>`"))?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return Err(Error::unauthorized(
            "expected `Authorization: Bearer <token>`",
        ));
    }
    Ok(Some(token.trim()))
}

/// Check a bearer token. Unknown, revoked and expired tokens are all just
/// "invalid" (no hint which).
pub async fn authenticate(state: &AppState, token: &str) -> Result<Grant, Error> {
    let invalid = || Error::unauthorized("invalid, expired or revoked API token");
    if !token.starts_with(PREFIX) {
        return Err(invalid());
    }
    let found = akasha_db::api_tokens::authenticate(&state.db, &hash_token(token))
        .await
        .map_err(|err| {
            tracing::error!(%err, "database error checking an API token");
            Error::internal("internal error")
        })?;
    let grant = found.ok_or_else(invalid)?;
    Ok(Grant {
        token_id: grant.token_id,
        user_id: grant.owner_id,
        scopes: Scopes::from_db(&grant.scopes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_prefixed_and_hash_consistently() {
        let a = generate();
        let b = generate();
        assert_ne!(a.secret, b.secret);
        assert!(a.secret.starts_with(PREFIX));
        assert_eq!(a.secret.len(), PREFIX.len() + 43);
        assert_eq!(hash_token(&a.secret), a.hash);
        assert_eq!(a.prefix.len(), SHOWN_CHARS);
        assert!(a.secret.starts_with(&a.prefix));
    }

    #[test]
    fn bearer_header_parsing() {
        let mut h = HeaderMap::new();
        assert_eq!(bearer(&h).expect("none"), None);
        h.insert(AUTHORIZATION, "Bearer  abc ".parse().expect("header"));
        assert_eq!(bearer(&h).expect("some"), Some("abc"));
        h.insert(AUTHORIZATION, "bearer abc".parse().expect("header"));
        assert_eq!(bearer(&h).expect("some"), Some("abc"));
        h.insert(AUTHORIZATION, "Basic abc".parse().expect("header"));
        assert!(bearer(&h).is_err());
    }

    #[test]
    fn scopes_round_trip() {
        assert_eq!(Scopes::READ.to_db(), vec!["read"]);
        assert_eq!(
            Scopes::from_db(&Scopes::READ_WRITE.to_db()),
            Scopes::READ_WRITE
        );
    }
}
