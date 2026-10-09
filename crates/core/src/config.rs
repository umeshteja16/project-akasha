//! Runtime configuration.
//!
//! Sources, later ones win:
//! 1. built-in defaults,
//! 2. `akasha.toml` in the working directory (optional),
//! 3. environment variables prefixed with `AKASHA_` (e.g. `AKASHA_BIND_ADDR`),
//! 4. `DATABASE_URL`, the variable sqlx tooling already uses.

use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    /// Postgres connection string.
    pub database_url: String,
    /// Address the HTTP server listens on.
    pub bind_addr: String,
    /// Maximum Postgres connections in the pool.
    pub db_max_connections: u32,
    /// Log output format.
    pub log_format: LogFormat,
    /// Mark the session cookie `Secure` (HTTPS only). Turn on in production.
    pub cookie_secure: bool,
    /// Allow anyone who can reach the server to create an account.
    pub allow_registration: bool,
    /// How long a login session lasts, in days.
    pub session_ttl_days: u32,
    /// Where uploaded file contents are kept.
    pub storage_backend: StorageBackend,
    /// Root directory for the `local` storage backend.
    pub storage_dir: String,
    /// Bucket name for the `s3` backend.
    pub storage_s3_bucket: Option<String>,
    /// Region for the `s3` backend (S3-compatible services usually accept any value).
    pub storage_s3_region: Option<String>,
    /// Custom endpoint for S3-compatible services (MinIO, R2, Garage, ...).
    pub storage_s3_endpoint: Option<String>,
    /// Access key; falls back to the standard `AWS_ACCESS_KEY_ID` when unset.
    pub storage_s3_access_key_id: Option<String>,
    /// Secret key; falls back to the standard `AWS_SECRET_ACCESS_KEY` when unset.
    pub storage_s3_secret_access_key: Option<Secret>,
    /// Allow plain-HTTP endpoints (local MinIO). Never enable for remote services.
    pub storage_s3_allow_http: bool,
}

/// Blob storage backend.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StorageBackend {
    /// A directory on the local filesystem.
    Local,
    /// Amazon S3 or an S3-compatible service.
    S3,
}

/// A configuration value that must never appear in logs (`Debug` prints `[redacted]`).
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The secret value. Only call this where the value is actually used.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[redacted]")
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// Human-readable, for local development.
    Pretty,
    /// One JSON object per line, for production log shipping.
    Json,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database_url: "postgres://akasha:akasha@localhost:5432/akasha".into(),
            bind_addr: "0.0.0.0:8080".into(),
            db_max_connections: 10,
            log_format: LogFormat::Pretty,
            cookie_secure: false,
            allow_registration: true,
            session_ttl_days: 30,
            storage_backend: StorageBackend::Local,
            storage_dir: "./storage".into(),
            storage_s3_bucket: None,
            storage_s3_region: None,
            storage_s3_endpoint: None,
            storage_s3_access_key_id: None,
            storage_s3_secret_access_key: None,
            storage_s3_allow_http: false,
        }
    }
}

impl Config {
    /// The figment used by [`Config::load`]; exposed so tests can inspect sources.
    pub fn figment() -> Figment {
        Figment::from(Serialized::defaults(Config::default()))
            .merge(Toml::file("akasha.toml"))
            .merge(Env::prefixed("AKASHA_"))
            .merge(Env::raw().only(&["DATABASE_URL"]))
    }

    /// Load configuration from all sources.
    pub fn load() -> Result<Self, Box<figment::Error>> {
        Self::figment().extract().map_err(Box::new)
    }
}

#[cfg(test)]
// figment's Jail API returns its large error type by value.
#[allow(clippy::result_large_err)]
mod tests {
    use super::*;

    #[test]
    fn defaults_apply_without_any_source() {
        figment::Jail::expect_with(|_jail| {
            assert_eq!(Config::figment().extract::<Config>()?, Config::default());
            Ok(())
        });
    }

    #[test]
    fn env_overrides_toml_and_database_url_is_read_raw() {
        figment::Jail::expect_with(|jail| {
            jail.create_file("akasha.toml", r#"bind_addr = "127.0.0.1:1""#)?;
            jail.set_env("AKASHA_BIND_ADDR", "127.0.0.1:2");
            jail.set_env("AKASHA_LOG_FORMAT", "json");
            jail.set_env("DATABASE_URL", "postgres://x@y/z");

            let config = Config::figment().extract::<Config>()?;
            assert_eq!(config.bind_addr, "127.0.0.1:2");
            assert_eq!(config.log_format, LogFormat::Json);
            assert_eq!(config.database_url, "postgres://x@y/z");
            Ok(())
        });
    }

    #[test]
    fn storage_settings_come_from_env_and_secrets_are_redacted() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("AKASHA_STORAGE_BACKEND", "s3");
            jail.set_env("AKASHA_STORAGE_S3_BUCKET", "akasha");
            jail.set_env("AKASHA_STORAGE_S3_SECRET_ACCESS_KEY", "hunter2");

            let config = Config::figment().extract::<Config>()?;
            assert_eq!(config.storage_backend, StorageBackend::S3);
            assert_eq!(config.storage_s3_bucket.as_deref(), Some("akasha"));
            let secret = config.storage_s3_secret_access_key.clone();
            assert_eq!(secret.as_ref().map(Secret::expose), Some("hunter2"));
            assert!(!format!("{config:?}").contains("hunter2"));
            Ok(())
        });
    }
}
