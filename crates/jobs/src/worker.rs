//! The worker loop: claim, run, heartbeat, reap, schedule, shut down gracefully.

use std::{
    collections::HashMap,
    future::Future,
    panic::AssertUnwindSafe,
    pin::pin,
    sync::Arc,
    time::{Duration, Instant},
};

use futures_util::FutureExt;
use sqlx::{PgPool, postgres::PgListener};
use tokio::{
    task::{Id, JoinError, JoinSet},
    time::{MissedTickBehavior, interval},
};
use tracing::Instrument;

use crate::{
    Attempt, CHANNEL, JobError, QueueError, Registry, Schedule,
    outcome::{panic_message, record},
    queue::{self, ClaimedJob},
    schedule,
};

/// Tuning for [`Worker`].
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// Jobs run at the same time.
    pub concurrency: usize,
    /// How often to look for due jobs without a notification (retries, delayed jobs,
    /// schedules, reaping). New jobs wake the worker immediately via `LISTEN`.
    pub poll_interval: Duration,
    /// A running job whose heartbeat is older than this is presumed lost and
    /// reclaimed. Heartbeats are sent every third of it.
    pub visibility_timeout: Duration,
    /// On shutdown, how long in-flight jobs may take to finish before they are
    /// aborted and handed back to the queue.
    pub shutdown_grace: Duration,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            concurrency: 4,
            poll_interval: Duration::from_secs(5),
            visibility_timeout: Duration::from_secs(300),
            shutdown_grace: Duration::from_secs(30),
        }
    }
}

/// Runs the handlers in a [`Registry`] against the queue.
pub struct Worker<C> {
    pool: PgPool,
    ctx: C,
    registry: Arc<Registry<C>>,
    config: WorkerConfig,
    schedules: Vec<Schedule>,
    id: Arc<str>,
}

impl<C: Clone + Send + Sync + 'static> Worker<C> {
    pub fn new(pool: PgPool, ctx: C, registry: Registry<C>, config: WorkerConfig) -> Self {
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let id = format!("worker-{}-{}", std::process::id(), &suffix[..8]);
        Self {
            pool,
            ctx,
            registry: Arc::new(registry),
            config,
            schedules: Vec::new(),
            id: id.into(),
        }
    }

    /// Periodic jobs this worker keeps enqueueing.
    pub fn with_schedules(mut self, schedules: Vec<Schedule>) -> Self {
        self.schedules = schedules;
        self
    }

    /// Identifies this worker in `jobs.locked_by` and in logs.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Run until `shutdown` resolves, then let in-flight jobs finish (up to
    /// [`WorkerConfig::shutdown_grace`]) and hand back the rest.
    pub async fn run(self, shutdown: impl Future<Output = ()>) -> Result<(), QueueError> {
        let kinds = self.registry.kinds();
        let names: Vec<String> = self.schedules.iter().map(|s| s.name.to_owned()).collect();
        schedule::upsert(&self.pool, &self.schedules).await?;
        self.log_unhandled(&kinds).await;
        let mut listener = self.listen().await;

        let mut tasks: JoinSet<()> = JoinSet::new();
        let mut running: HashMap<Id, ClaimedJob> = HashMap::new();
        let mut poll = interval(self.config.poll_interval);
        poll.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut beat = interval((self.config.visibility_timeout / 3).max(Duration::from_secs(1)));
        beat.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut shutdown = pin!(shutdown);
        let concurrency = self.config.concurrency.max(1);
        tracing::info!(worker = %self.id, concurrency, ?kinds, "worker started");

        loop {
            let free = concurrency.saturating_sub(tasks.len());
            if free > 0 {
                let limit = i64::try_from(free).unwrap_or(1);
                match queue::claim(&self.pool, &self.id, &kinds, limit).await {
                    Ok(jobs) => {
                        for job in jobs {
                            let handle = tasks.spawn(self.execute(job.clone()));
                            running.insert(handle.id(), job);
                        }
                    }
                    Err(err) => tracing::warn!(%err, "claiming jobs failed"),
                }
            }
            tokio::select! {
                () = &mut shutdown => break,
                Some(done) = tasks.join_next_with_id(), if !tasks.is_empty() => {
                    finished(done, &mut running);
                }
                () = notified(&mut listener, self.config.poll_interval) => {}
                _ = poll.tick() => self.maintain(&names).await,
                _ = beat.tick() => {
                    let ids: Vec<_> = running.values().map(|j| j.id).collect();
                    if let Err(err) = queue::heartbeat(&self.pool, &self.id, &ids).await {
                        tracing::warn!(%err, "job heartbeat failed");
                    }
                }
            }
        }

        self.drain(tasks, running).await;
        tracing::info!(worker = %self.id, "worker stopped");
        Ok(())
    }

    /// Run due jobs one at a time until none are left; returns how many ran.
    /// For tests and one-shot maintenance. Does not reap or schedule.
    pub async fn run_until_idle(&self) -> Result<usize, QueueError> {
        const MAX_JOBS: usize = 10_000;
        let kinds = self.registry.kinds();
        let mut ran = 0;
        while ran < MAX_JOBS {
            let Some(job) = queue::claim(&self.pool, &self.id, &kinds, 1)
                .await?
                .into_iter()
                .next()
            else {
                break;
            };
            self.execute(job).await;
            ran += 1;
        }
        Ok(ran)
    }

    /// Reclaim jobs from dead workers and fire due schedules.
    async fn maintain(&self, names: &[String]) {
        match queue::reap_stale(&self.pool, self.config.visibility_timeout).await {
            Ok(0) => {}
            Ok(n) => tracing::warn!(count = n, "reclaimed jobs from unresponsive workers"),
            Err(err) => tracing::warn!(%err, "reaping stale jobs failed"),
        }
        if !names.is_empty() {
            match schedule::tick(&self.pool, names).await {
                Ok(0) => {}
                Ok(n) => tracing::debug!(count = n, "enqueued scheduled jobs"),
                Err(err) => tracing::warn!(%err, "firing job schedules failed"),
            }
        }
    }

    /// One job, start to finish, including recording the outcome.
    fn execute(&self, job: ClaimedJob) -> impl Future<Output = ()> + Send + 'static {
        let pool = self.pool.clone();
        let worker = Arc::clone(&self.id);
        let started = self
            .registry
            .start(&job.kind, self.ctx.clone(), job.payload.clone());
        let span =
            tracing::info_span!("job", kind = %job.kind, id = %job.id, attempt = job.attempts);
        let attempt = Attempt {
            job_id: job.id,
            number: job.attempts,
            max: job.max_attempts,
        };
        async move {
            let clock = Instant::now();
            let outcome = match started {
                None => Err(JobError::Permanent(format!(
                    "no handler for `{}`",
                    job.kind
                ))),
                Some(Err(err)) => Err(JobError::Permanent(format!("invalid payload: {err}"))),
                Some(Ok(fut)) => match AssertUnwindSafe(attempt.scope(fut)).catch_unwind().await {
                    Ok(result) => result,
                    Err(panic) => Err(JobError::Retry(format!(
                        "handler panicked: {}",
                        panic_message(panic.as_ref())
                    ))),
                },
            };
            let elapsed = clock.elapsed();
            let elapsed_ms = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
            let kind = job.kind.clone();
            metrics::histogram!("akasha_job_duration_seconds", "kind" => kind.clone())
                .record(elapsed.as_secs_f64());
            metrics::histogram!("akasha_job_wait_seconds", "kind" => kind).record(job.waited_secs);
            record(&pool, &worker, &job, outcome, elapsed_ms).await;
        }
        .instrument(span)
    }

    async fn drain(&self, mut tasks: JoinSet<()>, mut running: HashMap<Id, ClaimedJob>) {
        if tasks.is_empty() {
            return;
        }
        tracing::info!(in_flight = tasks.len(), "waiting for in-flight jobs");
        let waited = tokio::time::timeout(self.config.shutdown_grace, async {
            while let Some(done) = tasks.join_next_with_id().await {
                finished(done, &mut running);
            }
        })
        .await;
        if waited.is_ok() {
            return;
        }
        tracing::warn!(
            in_flight = tasks.len(),
            "shutdown grace period over; aborting jobs"
        );
        tasks.abort_all();
        while let Some(done) = tasks.join_next_with_id().await {
            if let Ok((id, ())) = done {
                // It finished on its own before the abort landed.
                running.remove(&id);
            }
        }
        for job in running.values() {
            match queue::release(&self.pool, &self.id, job).await {
                Ok(_) => tracing::info!(kind = %job.kind, id = %job.id, "job handed back"),
                Err(err) => tracing::warn!(%err, id = %job.id, "could not hand back job"),
            }
        }
    }

    async fn listen(&self) -> Option<PgListener> {
        let connected = async {
            let mut listener = PgListener::connect_with(&self.pool).await?;
            listener.listen(CHANNEL).await?;
            Ok::<_, sqlx::Error>(listener)
        };
        match connected.await {
            Ok(listener) => Some(listener),
            Err(err) => {
                tracing::warn!(%err, "LISTEN failed; relying on polling");
                None
            }
        }
    }

    async fn log_unhandled(&self, kinds: &[String]) {
        match queue::unhandled_kinds(&self.pool, kinds).await {
            Ok(rows) => {
                for (kind, count) in rows {
                    tracing::warn!(%kind, count, "jobs waiting for a handler this worker lacks");
                }
            }
            Err(err) => tracing::warn!(%err, "could not count unhandled jobs"),
        }
    }
}

fn finished(done: Result<(Id, ()), JoinError>, running: &mut HashMap<Id, ClaimedJob>) {
    let id = match done {
        Ok((id, ())) => id,
        Err(err) => {
            // Panics are caught inside the task; this is a cancellation.
            tracing::warn!(%err, "job task ended abnormally");
            err.id()
        }
    };
    running.remove(&id);
}

/// Wait for a `NOTIFY`. Without a listener (or after an error) this only resolves
/// after a pause, so it never spins.
async fn notified(listener: &mut Option<PgListener>, pause: Duration) {
    match listener {
        Some(l) => {
            if let Err(err) = l.recv().await {
                tracing::warn!(%err, "job notifications interrupted");
                tokio::time::sleep(pause).await;
            }
        }
        None => std::future::pending().await,
    }
}
