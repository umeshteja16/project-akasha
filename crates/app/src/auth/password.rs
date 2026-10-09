//! Argon2id password hashing. Hashing is deliberately slow, so it runs on the
//! blocking thread pool instead of stalling the async runtime.

use std::sync::OnceLock;

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};

use akasha_core::Error;

pub const MIN_LEN: usize = 8;
pub const MAX_LEN: usize = 128;

pub fn validate(password: &str) -> Result<(), Error> {
    let len = password.chars().count();
    if len < MIN_LEN {
        return Err(Error::bad_request(format!(
            "password must be at least {MIN_LEN} characters"
        )));
    }
    if len > MAX_LEN {
        return Err(Error::bad_request(format!(
            "password must be at most {MAX_LEN} characters"
        )));
    }
    Ok(())
}

fn hash_blocking(password: &str) -> Result<String, Error> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|err| Error::internal(format!("password hashing failed: {err}")))
}

fn verify_blocking(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
        .unwrap_or(false)
}

pub async fn hash(password: String) -> Result<String, Error> {
    tokio::task::spawn_blocking(move || hash_blocking(&password))
        .await
        .map_err(|err| Error::internal(format!("hashing task failed: {err}")))?
}

/// Verify `password` against `hash`. When `hash` is `None` (unknown user) a dummy hash
/// is checked anyway so response timing does not reveal which emails are registered.
pub async fn verify(password: String, hash: Option<String>) -> bool {
    static DUMMY: OnceLock<String> = OnceLock::new();
    tokio::task::spawn_blocking(move || {
        let known = hash.is_some();
        let hash = hash.unwrap_or_else(|| {
            DUMMY
                .get_or_init(|| hash_blocking("timing-equaliser").unwrap_or_default())
                .clone()
        });
        verify_blocking(&password, &hash) && known
    })
    .await
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn hash_then_verify() {
        let h = hash("correct horse".into()).await.expect("hash");
        assert!(h.starts_with("$argon2id$"));
        assert!(verify("correct horse".into(), Some(h.clone())).await);
        assert!(!verify("wrong horse".into(), Some(h)).await);
        assert!(!verify("anything".into(), None).await);
    }

    #[test]
    fn length_limits() {
        assert!(validate("short").is_err());
        assert!(validate("long enough").is_ok());
        assert!(validate(&"x".repeat(MAX_LEN + 1)).is_err());
    }
}
