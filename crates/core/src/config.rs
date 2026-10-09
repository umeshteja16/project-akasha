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
}
