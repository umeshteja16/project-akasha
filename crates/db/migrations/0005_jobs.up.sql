-- Background job queue (ADR 0002: Postgres is the only stateful service).
-- Workers claim with `FOR UPDATE SKIP LOCKED`; see crates/jobs for the lifecycle.
--
--   queued ──claim──▶ running ──ok──▶ succeeded
--      ▲                 │ error / lost worker
--      │                 ▼
--      │              failed (waits until run_at, then claimable again)
--      │                 │ attempts exhausted, permanent error or bad payload
--      │                 ▼
--      └─ enqueue        dead

CREATE TABLE jobs (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    kind         text NOT NULL CHECK (length(kind) BETWEEN 1 AND 100),
    payload      jsonb NOT NULL DEFAULT '{}',
    status       text NOT NULL DEFAULT 'queued'
                 CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'dead')),
    attempts     integer NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts integer NOT NULL DEFAULT 5 CHECK (max_attempts >= 1),
    -- Not claimable before this time (delayed jobs and retry backoff).
    run_at       timestamptz NOT NULL DEFAULT now(),
    -- Set while running; refreshed by the worker's heartbeat. A running job whose
    -- locked_at is older than the visibility timeout belongs to a dead worker.
    locked_at    timestamptz,
    locked_by    text,
    last_error   text,
    -- Optional: at most one *queued* job per (kind, dedupe_key).
    dedupe_key   text,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    finished_at  timestamptz,
    CHECK ((status = 'running') = (locked_at IS NOT NULL))
);

CREATE TRIGGER jobs_updated_at BEFORE UPDATE ON jobs
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- The claim query: claimable rows ordered by run_at.
CREATE INDEX jobs_claim_idx ON jobs (run_at) WHERE status IN ('queued', 'failed');
-- The reaper: running rows by heartbeat age.
CREATE INDEX jobs_running_idx ON jobs (locked_at) WHERE status = 'running';
-- Pruning finished jobs.
CREATE INDEX jobs_finished_idx ON jobs (finished_at) WHERE status IN ('succeeded', 'dead');
-- Looking up the latest job for a key (e.g. a file's extraction status).
CREATE INDEX jobs_kind_key_idx ON jobs (kind, dedupe_key, created_at DESC)
    WHERE dedupe_key IS NOT NULL;
-- Dedupe: enqueueing the same logical job twice while it waits is a no-op. Running or
-- retrying jobs do not block a new one: it may carry newer intent (handlers are
-- idempotent, so an extra run is harmless).
CREATE UNIQUE INDEX jobs_dedupe_idx ON jobs (kind, dedupe_key)
    WHERE dedupe_key IS NOT NULL AND status = 'queued';

-- Wake idle workers as soon as a job is enqueued. NOTIFY is transactional: it is
-- delivered only if the enqueueing transaction commits.
CREATE FUNCTION jobs_notify() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('akasha_jobs', '');
    RETURN NULL;
END;
$$;

CREATE TRIGGER jobs_notify AFTER INSERT ON jobs
FOR EACH STATEMENT EXECUTE FUNCTION jobs_notify();

-- Periodic jobs. Each worker upserts the schedules it knows at start-up; whoever
-- first advances `next_run_at` (a conditional UPDATE) enqueues the run, so several
-- workers never enqueue the same tick twice.
CREATE TABLE job_schedules (
    name          text PRIMARY KEY,
    kind          text NOT NULL,
    payload       jsonb NOT NULL DEFAULT '{}',
    interval_secs integer NOT NULL CHECK (interval_secs >= 1),
    next_run_at   timestamptz NOT NULL DEFAULT now(),
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now()
);

CREATE TRIGGER job_schedules_updated_at BEFORE UPDATE ON job_schedules
FOR EACH ROW EXECUTE FUNCTION set_updated_at();
