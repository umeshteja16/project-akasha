//! A background job queue on Postgres (ADR 0002: no Redis).
//!
//! Domain-agnostic: this crate knows about the `jobs` table, claiming, retries and
//! the worker loop, but nothing about files or users. Job types and their handlers
//! live with the code they serve (`crates/app/src/jobs`).
//!
//! # Lifecycle
//! - [`enqueue`] inserts a `queued` row, usually inside the caller's transaction, so
//!   the job exists if and only if the change that needs it commits. A trigger
//!   `NOTIFY`s idle workers on commit.
//! - Workers [`queue::claim`] batches with `FOR UPDATE SKIP LOCKED` (no two workers
//!   ever get the same job), mark them `running` and heartbeat `locked_at`.
//! - Success → `succeeded`. A retryable error → `failed` with `run_at` pushed out by
//!   exponential backoff with jitter; it is claimed again once due. After
//!   `max_attempts`, a permanent error or an undecodable payload → `dead`.
//! - A `running` job whose heartbeat is older than the visibility timeout belonged to
//!   a crashed worker and is put back (`failed`, or `dead` if out of attempts).
//!
//! # Rules for handlers
//! Delivery is **at least once**: a handler may run again after a crash, a lost
//! heartbeat or a duplicate enqueue. Every handler must be idempotent: re-check the
//! current state in the database instead of trusting the payload, and make repeated
//! runs harmless.
//!
//! Kinds without a registered handler are never claimed: they stay `queued` until a
//! worker that knows them starts (the worker logs them once at start-up).

mod backoff;
mod outcome;
pub mod queue;
mod registry;
pub mod schedule;
mod worker;

use serde::{Serialize, de::DeserializeOwned};

pub use backoff::backoff;
pub use queue::{JobInfo, enqueue, enqueue_delayed};
pub use registry::Registry;
pub use schedule::Schedule;
pub use worker::{Worker, WorkerConfig};

/// The `NOTIFY` channel the insert trigger signals.
pub const CHANNEL: &str = "akasha_jobs";

/// A typed job. The payload is the serde form of the implementing type.
pub trait Job: Serialize + DeserializeOwned + Send + 'static {
    /// Stable identifier stored in `jobs.kind`. Never rename a kind that may still be
    /// queued somewhere.
    const KIND: &'static str;
    /// Attempts before the job is dead-lettered.
    const MAX_ATTEMPTS: i32 = 5;

    /// Jobs with the same key (and kind) are enqueued at most once while queued.
    fn dedupe_key(&self) -> Option<String> {
        None
    }
}

/// Queue errors (database or payload encoding).
#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    #[error("job queue database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("job payload could not be encoded: {0}")]
    Payload(#[from] serde_json::Error),
}

/// Why a handler did not succeed.
///
/// Any `std::error::Error` converts into [`JobError::Retry`], so `?` works in
/// handlers. Use [`JobError::permanent`] when retrying cannot help.
#[derive(Debug)]
pub enum JobError {
    /// Try again later (with backoff), until attempts run out.
    Retry(String),
    /// Do not retry: dead-letter immediately.
    Permanent(String),
}

impl JobError {
    pub fn retry(msg: impl std::fmt::Display) -> Self {
        Self::Retry(msg.to_string())
    }

    pub fn permanent(msg: impl std::fmt::Display) -> Self {
        Self::Permanent(msg.to_string())
    }
}

impl std::fmt::Display for JobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Retry(msg) | Self::Permanent(msg) => f.write_str(msg),
        }
    }
}

impl<E: std::error::Error> From<E> for JobError {
    fn from(err: E) -> Self {
        Self::Retry(err.to_string())
    }
}

#[cfg(test)]
mod tests;
