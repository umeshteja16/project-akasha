# 0007: Job queue design

- Status: accepted · 2026-10-09

## Context
ADR 0002 puts the job queue in Postgres. Ingestion, blob garbage collection and
housekeeping all need background work that survives crashes and restarts.

## Decision
- A separate crate `crates/jobs` (`akasha-jobs`) holds the domain-agnostic queue: SQL for
  the `jobs` / `job_schedules` tables, the typed `Job` trait, the handler `Registry` and
  the `Worker` runtime. It is not in `akasha-db` because it owns a runtime (tokio tasks,
  `LISTEN`, shutdown) and stays reusable; `akasha-db` stays plain query functions.
  Job types and handlers live in `crates/app/src/jobs`. The migration stays in
  `crates/db/migrations` (one migrator).
- **Transactional enqueue**: `enqueue(&mut tx, &job)` so a job exists iff the change that
  needs it commits. A statement trigger `NOTIFY`s `akasha_jobs` (delivered on commit); the
  worker `LISTEN`s and also polls (`AKASHA_WORKER_POLL_SECS`) for retries and schedules.
- **Claiming**: `UPDATE … FROM (SELECT … FOR UPDATE SKIP LOCKED LIMIT n)`, only for kinds
  the worker has handlers for. Unknown kinds stay `queued` (logged once at start), so a
  job can be enqueued before its handler ships without being lost or faked.
- **At-least-once**: heartbeats refresh `locked_at`; jobs past the visibility timeout are
  reclaimed. Handlers must therefore be idempotent. State changes are guarded by
  `(locked_by, attempts)` so a reclaimed job cannot be finished by its old owner.
- **Failures**: retryable errors back off exponentially (10 s doubling, cap 1 h, equal
  jitter) in status `failed`; permanent errors, undecodable payloads and exhausted
  attempts go to `dead`. A panicking handler is a retryable failure, not a crash.
- **Dedupe** applies only to `queued` jobs (partial unique index): a running or retrying
  job does not block a new one, because the new one may carry newer intent.
- **Periodic jobs** use `job_schedules`: a conditional `UPDATE … WHERE next_run_at <= now()`
  and the insert run in one statement, so any number of workers fire each tick once.
- **Blob deletion** never touches storage inside a request: the transaction that deletes
  file rows enqueues `delete_blob_if_unreferenced`, whose handler re-checks references under
  the per-hash advisory lock. A daily sweep queues blobs no row references.

## Consequences
One process can run API + worker (`serve --with-worker`, the Docker default) or they scale
separately (`akasha worker`). Throughput is bounded by Postgres; fine for a personal
knowledge base. Without any worker running, nothing is extracted, blobs are not freed
and sessions are not pruned.
