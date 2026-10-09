//! Shared building blocks for every Akasha crate: configuration and error types.
//!
//! Keep this crate free of I/O frameworks (no axum, no sqlx) so every other crate
//! can depend on it cheaply.

pub mod config;
pub mod error;

pub use config::{Config, LlmProvider, LogFormat, Secret, StorageBackend};
pub use error::{Error, ErrorCode};
