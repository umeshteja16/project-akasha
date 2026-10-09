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
crates/app    the `akasha` binary: axum routes (src/routes/*), auth/, state, telemetry
.sqlx/        offline cache of checked SQL queries (`just sqlx-prepare`)
web/          React + TS frontend (Vite, Biome, Vitest)
legacy/       old TypeScript implementation: read-only reference, do not edit
docs/adr/     architecture decisions
openapi.json  generated API contract (`just openapi`), checked in CI
```
New crates planned (create them when their step starts, not before): `ingest`, `ml`, `search`.

## Commands
`just` lists everything. Common: `just serve`, `just web`, `just check`, `just fmt`,
`just openapi`, `just migration <name>`, `just migrate`.

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
- **Tests**: DB tests use `#[sqlx::test]` (fresh database per test). HTTP tests drive the router
  with `tower::ServiceExt::oneshot` (see `crates/app/tests/http.rs`).
- **Small files**: split a file once it passes ~300 lines.
- **Frontend**: TS strict, no `any`, server state via TanStack Query, generated API types only.
- **Dependencies**: add to `[workspace.dependencies]` in the root `Cargo.toml`; `cargo-deny`
  rejects non-permissive licenses.
- Record any significant architecture choice as a new ADR in `docs/adr/`.
