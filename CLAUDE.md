# CLAUDE.md

Guidance for AI agents and humans working in this repo. Keep it short and true.

## First
- Read [`PROGRESS.md`](PROGRESS.md): current step, next task, gotchas.
- Before every commit run `just check` (fmt, clippy, tests, web checks, OpenAPI drift).
  It needs Postgres: `just db-up` (Docker) or `just db-local` (no Docker).
- When you finish a piece of work, update `PROGRESS.md` in the same commit (tick boxes,
  move "Next up", add a session-log line). This is how the next session knows what happened.

## Layout
```
crates/core   config + error types (no I/O frameworks)
crates/db     sqlx pool, migrations (crates/db/migrations), query functions
crates/storage content-addressed blob store (object_store: local dir or S3)
crates/jobs   Postgres job queue + worker runtime (domain-agnostic, ADR 0007)
crates/ingest text extraction (text/Markdown/PDF/OCR) + chunking; pure, blocking (ADR 0008)
crates/ml     embeddings + reranking (fastembed on runtime-loaded ONNX Runtime, ADR 0009);
              blocking traits, model catalog, downloader, deterministic fakes for tests
crates/search hybrid retrieval (FTS + pgvector, RRF, rerank, snippets, similar files;
              ADR 0010); SQL in crates/db/src/search*, HTTP in app routes/search
crates/llm    chat model providers (Ollama, Claude, Gemini, OpenAI-compatible, fake; ADR 0012)
crates/app    the `akasha` binary: axum routes (src/routes/*), auth/, jobs/ (job kinds +
              handlers), chat/ (grounded answers), enrich/ (file summaries + suggested
              tags, ADR 0013), eval/ (`akasha eval`), state, telemetry
eval/         search benchmark: corpus/, queries.json, gate.json, baselines/ (ADR 0011)
.sqlx/        offline cache of checked SQL queries (`just sqlx-prepare`)
web/          React + TS frontend (Vite, Biome, Vitest)
legacy/       old TypeScript implementation: read-only reference, do not edit
docs/adr/     architecture decisions
openapi.json  generated API contract (`just openapi`), checked in CI
```

## Commands
`just` lists everything. Common: `just serve`, `just web`, `just check`, `just fmt`,
`just openapi`, `just migration <name>`, `just migrate`. `just serve` runs the API and the
background worker in one process; `just worker` runs a worker alone. HTTP tests run queued
jobs with `TestApp::run_jobs()`.

## Rules
- **Schema** changes only via `just migration <name>` (reversible up/down files). Never alter
  the schema from application code.
- **SQL**: use the checked macros (`sqlx::query!`, `query_as!`) in `crates/db`. After changing
  one, run `just sqlx-prepare` and commit `.sqlx/`. CI fails if the cache is stale.
- **Auth**: take `AuthUser` as a handler argument to require sign-in, and always filter queries
  by `auth.user_id` (ownership). Credential-checking routes go in the rate-limited group in
  `routes.rs`. Request bodies use `crate::extract::Json` (errors in the standard shape).
- **Errors**: return `akasha_core::Error` (code + message); the HTTP layer renders
  `{ "error": { "code", "message" } }`. Success bodies are plain JSON objects.
- **Routes**: one module per feature in `crates/app/src/routes/`, annotated with
  `#[utoipa::path]` and registered in `ApiDoc`. Then run `just openapi` and commit `openapi.json`
  and `web/src/api/schema.d.ts`.
- **No `unwrap()`/`expect()` in non-test code** (clippy warns; CI denies warnings). `unsafe` is forbidden.
- **Background work** goes through the job queue: define a `Job` in
  `crates/app/src/jobs/kinds.rs`, register its handler in `jobs::registry()`, and enqueue it
  inside the same transaction as the change that needs it (`akasha_jobs::enqueue(&mut tx, ..)`).
  **Handlers must be idempotent**: delivery is at least once (crash, lost heartbeat,
  duplicate enqueue), so re-check state in the database and make repeat runs harmless. Return
  `JobError::permanent` when retrying cannot help. Never delete blobs in a request: enqueue
  `DeleteBlobIfUnreferenced`. Never rename a job kind that may still be queued.
- **Search quality**: `cargo test` fails if ranking gets worse on `eval/` (deterministic
  models). Re-record `eval/baselines/` with `akasha eval --update-baseline` only for an
  intended change, and say so in the commit.
- **Tests**: DB tests use `#[sqlx::test]` (fresh database per test). HTTP tests drive the router
  with `tower::ServiceExt::oneshot` (see `crates/app/tests/http.rs`). Tests never download
  models or call LLM providers: `support::test_config()` uses the `hash-384` embedder,
  `overlap` reranker and the `fake` chat model (provider HTTP is tested against local mocks).
- **Small files**: split a file once it passes ~300 lines.
- **Frontend**: TS strict, no `any`, server state via TanStack Query, generated API types only.
- **Dependencies**: add to `[workspace.dependencies]` in the root `Cargo.toml`; `cargo-deny`
  rejects non-permissive licenses.
- Record any significant architecture choice as a new ADR in `docs/adr/`.
