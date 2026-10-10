//! Recording how a job attempt ended.

use sqlx::PgPool;

use crate::{
    JobError,
    queue::{self, ClaimedJob, Failed},
};

/// Store the outcome of one attempt and log it (inside the job's span).
pub(crate) async fn record(
    pool: &PgPool,
    worker: &str,
    job: &ClaimedJob,
    outcome: Result<(), JobError>,
    elapsed_ms: u64,
) {
    let finished = |outcome: &'static str| {
        metrics::counter!("akasha_jobs_finished_total", "kind" => job.kind.clone(), "outcome" => outcome)
            .increment(1);
    };
    match outcome {
        Ok(()) => match queue::complete(pool, worker, job).await {
            Ok(true) => {
                finished("succeeded");
                tracing::info!(elapsed_ms, "job succeeded");
            }
            Ok(false) => tracing::warn!(elapsed_ms, "job succeeded but was no longer ours"),
            Err(err) => tracing::error!(%err, "could not record job success"),
        },
        Err(error) => {
            let retry = matches!(error, JobError::Retry(_));
            let message = error.to_string();
            match queue::fail(pool, worker, job, &message, retry).await {
                Ok(Failed::Retrying) => {
                    finished("retry");
                    tracing::warn!(elapsed_ms, error = %message, "job failed; will retry");
                }
                Ok(Failed::Dead) => {
                    finished("dead");
                    tracing::error!(elapsed_ms, error = %message, "job dead-lettered");
                }
                Ok(Failed::Lost) => {
                    tracing::warn!(error = %message, "job failed but was no longer ours");
                }
                Err(err) => tracing::error!(%err, error = %message, "could not record job failure"),
            }
        }
    }
}

pub(crate) fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = panic.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_owned()
    }
}
