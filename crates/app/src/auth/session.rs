//! Session tokens and the session cookie.
//!
//! The cookie holds 32 random bytes (base64url). The database stores only their
//! SHA-256, so reading the `sessions` table does not let anyone sign in.

use argon2::password_hash::rand_core::{OsRng, RngCore};
use axum_extra::extract::cookie::{Cookie, SameSite};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

pub const COOKIE_NAME: &str = "akasha_session";

/// A fresh random token and the hash to store for it.
pub fn new_token() -> (String, [u8; 32]) {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let token = URL_SAFE_NO_PAD.encode(bytes);
    let hash = hash_token(&token);
    (token, hash)
}

pub fn hash_token(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

/// The cookie that carries a session. `HttpOnly` keeps it away from scripts and
/// `SameSite=Lax` stops other sites from sending it with their POSTs.
pub fn cookie(token: String, ttl_days: u32, secure: bool) -> Cookie<'static> {
    Cookie::build((COOKIE_NAME, token))
        .path("/")
        .http_only(true)
        .secure(secure)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::days(i64::from(ttl_days)))
        .build()
}

/// A cookie that tells the browser to forget the session.
pub fn removal(secure: bool) -> Cookie<'static> {
    let mut cookie = cookie(String::new(), 0, secure);
    cookie.make_removal();
    cookie
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_unique_and_hash_consistently() {
        let (a, ha) = new_token();
        let (b, _) = new_token();
        assert_ne!(a, b);
        assert_eq!(a.len(), 43);
        assert_eq!(hash_token(&a), ha);
    }
}
