//! Which attempt a handler is running as, so it can tell a retry from the last try
//! (e.g. to record a user-visible failure only once the job is about to die).

use std::future::Future;

tokio::task_local! {
    static ATTEMPT: Attempt;
}

/// The attempt the current job is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attempt {
    /// The job being run.
    pub job_id: uuid::Uuid,
    /// 1-based, including this one.
    pub number: i32,
    pub max: i32,
}

impl Attempt {
    /// A retryable failure now dead-letters the job.
    pub fn is_last(&self) -> bool {
        self.number >= self.max
    }

    /// Run `fut` with this attempt visible through [`current_attempt`].
    pub(crate) fn scope<F: Future>(self, fut: F) -> impl Future<Output = F::Output> {
        ATTEMPT.scope(self, fut)
    }
}

/// The attempt of the job whose handler is running on this task, or `None` outside
/// a worker (e.g. a handler called directly from a test).
pub fn current_attempt() -> Option<Attempt> {
    ATTEMPT.try_with(|a| *a).ok()
}
