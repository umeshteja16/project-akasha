# Akasha rewrite: progress

> **New session? Start here.** This file is the single source of truth for where the
> rewrite stands. Read "Next up", do it, then update this file in the same commit.

## How to resume (any machine, any dev, any agent)

1. `mise install` (pins Node, pnpm, just, sqlx-cli; Rust is pinned by `rust-toolchain.toml`)
2. `cp .env.example .env && just setup`
3. `just db-up` (Docker) or `just db-local` (no Docker, e.g. cloud agent sessions)
4. `just check` must pass before you start and before every commit
5. Read [`CLAUDE.md`](CLAUDE.md) for conventions, then pick up **Next up** below

Claude Code cloud sessions run steps 2–3 automatically (`.claude/hooks/session-start.sh`).

## Status

| | |
|---|---|
| **Current step** | Step 2: Files and ingestion |
| **Last updated** | 2026-10-09 |
| **`just check`** | passing (26 Rust tests, 1 web test) |
| **Old code** | `legacy/` (read-only reference; deleted in step 7) |

## Next up

**Step 2: Files and ingestion.** Do it in this order, one commit per bullet, `just check` green each time:

1. ~~**pgvector everywhere.**~~ Done: migration `0003_vector`, `local-postgres.sh` installs it.
2. **Storage.** Add `object_store` behind a small `Storage` trait in a new `crates/storage`
   (local disk default, `AKASHA_STORAGE_*` config). Content-addressed by SHA-256, deduplicated.
3. **Upload.** `POST /api/v1/files` (streaming multipart, `AKASHA_MAX_UPLOAD_MB`, magic-byte
   check with `infer`, filename sanitising), `files` table, ownership checks on every query.
   Port the attack cases from `legacy/scratch/test_magic_bytes.sh` into Rust tests.
4. **Job queue.** `jobs` table + `FOR UPDATE SKIP LOCKED` worker loop, retries with backoff,
   idempotent handlers, `akasha worker` subcommand (and `serve --with-worker` for single-box).
   Move session pruning onto it.
5. **Extraction.** New `crates/ingest`: plain text/markdown first, then PDF (`pdfium-render`),
   then OCR (`ocrs`); chunking with `text-splitter`. Store chunks in `file_chunks`.
6. **Embeddings.** New `crates/ml` with `fastembed`; record model name + dimension in the DB.
7. File CRUD routes (list/get/rename/tags/pin/download/delete/bulk-delete/reindex/status).

## Roadmap

Legend: `[x]` done, `[~]` in progress, `[ ]` not started. Each step ends with `just check` green.

### Step 0: Foundation ✅
- [x] Old TypeScript stack moved to `legacy/`; junk removed (`__pycache__`, 5 MB tessdata)
- [x] Cargo workspace: `crates/core` (config, errors), `crates/db` (pool, migrations), `crates/app` (binary)
- [x] `akasha serve | migrate | openapi`; `/healthz`, `/readyz`, `/api/openapi.json`
- [x] Request IDs, timeouts, panic catching, JSON logs, graceful shutdown
- [x] Error shape `{ "error": { "code", "message" } }` everywhere, including 404s
- [x] `web/` scaffold: Vite + React 19 + TS strict + Biome + Vitest + openapi-typescript
- [x] `justfile`, `mise.toml`, `compose.yaml`, distroless `Dockerfile`, `deny.toml`
- [x] CI: fmt, clippy, tests with Postgres, openapi drift, web checks, cargo-deny, Docker build
- [x] `PROGRESS.md`, `CLAUDE.md`, ADRs, cloud-session hook

### Step 1: Auth and users ✅
- [x] `users` + `sessions` migration (citext emails, SHA-256 token hashes, cascade delete)
- [x] register / login / logout / me; argon2id on the blocking pool; HttpOnly SameSite=Lax cookie
- [x] `AuthUser` extractor; per-IP rate limit on credential endpoints (burst 10, then 1 per 6 s)
- [x] Profile: display name, change password (revokes other sessions), delete account
- [x] Timing-safe login (dummy hash for unknown emails), identical errors for wrong email/password
- [x] `AKASHA_ALLOW_REGISTRATION`, `AKASHA_COOKIE_SECURE`, `AKASHA_SESSION_TTL_DAYS`
- [x] Compile-time-checked SQL with an offline cache (`.sqlx/`), checked in CI
- [x] Hourly expired-session pruning (moves to the job queue in step 2)

### Step 2: Files and ingestion
- [x] Add `pgvector` (also teach `scripts/local-postgres.sh` to install it; the Docker image already has it)
- [ ] Upload (streaming multipart, size limit, magic-byte check via `infer`, filename sanitising)
- [ ] Content-addressed storage (SHA-256, dedupe, ref-counting) behind `object_store` (local disk / S3)
- [ ] Postgres job queue (`SELECT … FOR UPDATE SKIP LOCKED`; retries, backoff, idempotent jobs), `akasha worker`
- [ ] New crate `crates/ingest`: PDF text (`pdfium-render`), OCR (`ocrs`), plain text/markdown, chunking (`text-splitter`)
- [ ] New crate `crates/ml`: embeddings via `fastembed` (bge-m3 or nomic-embed; dimension recorded in the DB)
- [ ] File CRUD: list, get, rename, tags, pin, download, thumbnail, bulk delete, reindex, status

### Step 3: Search
- [ ] New crate `crates/search`: Postgres FTS + pgvector HNSW, fused with RRF (k=60)
- [ ] Cross-encoder rerank (`fastembed` reranker)
- [ ] Filters (type, date, collection), snippets/highlights, pagination, "similar files"
- [ ] Port `legacy/apps/api/benchmark_queries.json` into an eval command (`akasha eval`) and a CI quality gate

### Step 4: Grounded chat
- [ ] LLM provider trait in `crates/ml`: Ollama (offline), Claude, Gemini; offline mode = Ollama only
- [ ] Conversations + messages tables, streaming answers over SSE, inline citations
- [ ] Refusal when evidence is weak (score threshold), tested
- [ ] Auto tags/summary per file (replaces the legacy Gemini calls in the worker)

### Step 5: New web UI (redesign)
- [ ] Design system first (tokens, type scale, light + dark), documented in `web/DESIGN.md`
- [ ] TanStack Router + Query, shadcn/ui on Tailwind 4, typed client from `openapi.json`
- [ ] Screens: auth, library, file detail, search, chat, collections, activity, settings
- [ ] Rust binary serves the built UI (`rust-embed`), so production is a single binary
- [ ] Playwright end-to-end tests in CI

### Step 6: Beyond parity
- [ ] MCP server (`akasha mcp`, `rmcp`) exposing search/read to AI agents
- [ ] Audio/video transcription (`whisper-rs`)
- [ ] Watched folders / connectors (Obsidian vault, Downloads)
- [ ] Collections, activity timeline, audit log (legacy parity)
- [ ] OpenTelemetry export + Prometheus `/metrics`

### Step 7: Release v0.1
- [ ] Feature parity with `legacy/` confirmed against the list below; delete `legacy/`
- [ ] Backup/restore docs, upgrade notes, tagged release with Docker image

### Step 8: Desktop (after server is stable)
- [ ] Tauri 2 app reusing the crates; storage backend decision (embedded Postgres vs SQLite) in an ADR

## Legacy feature inventory (parity checklist for step 7)

Auth (register, login, refresh, logout) · profile/display name · storage limit setting ·
file upload incl. "capture" · list/get/rename/delete/bulk-delete · download · thumbnail ·
extraction view · tags · pin · open tracking · reindex · similar files · collections CRUD ·
keyword/semantic/hybrid search with filters and spelling suggestion · grounded chat with
citations and conversations · activity timeline · audit log · strict offline mode.

## Decisions

See [`docs/adr/`](docs/adr). Summary:
0001 Rust rewrite, TS frontend · 0002 Postgres is the only stateful service ·
0003 ML runs in-process · 0004 Cookie sessions, not JWT · 0005 OpenAPI is the API contract ·
0006 Server first, desktop later.

## Known issues and gotchas

- The workspace has no license yet. Pick one before the first public release (`cargo-deny`
  already restricts *dependencies* to permissive licenses).
- `scripts/local-postgres.sh` uses the system Postgres (16 in cloud sessions) and installs
  pgvector for it (apt `postgresql-16-pgvector`, 0.6.x; falls back to building v0.8.0 from
  source). Docker/CI run pgvector 0.8 on Postgres 17, so do not rely on features newer than 0.6
  (HNSW is fine) without bumping the local install.
- Never edit an applied migration (even a comment): sqlx checksums it and refuses to run.
- First `cargo build` takes ~3 minutes; dependencies are compiled with `opt-level = 2`.
- Changing any `sqlx::query!` needs a running database and then `just sqlx-prepare`; commit
  the `.sqlx/` changes. Without `DATABASE_URL`, builds use the cache (that is how Docker builds).
- The rate limiter keys on the TCP peer IP. Behind a reverse proxy all clients share one bucket.
  Add a `trust_proxy` option (use `SmartIpKeyExtractor`) before deploying behind a proxy.
- The router must be served with `into_make_service_with_connect_info::<SocketAddr>()`
  (`run_serve` does this); without it rate-limited routes return 500. Tests inject
  `Extension(ConnectInfo(..))`, see `crates/app/tests/auth.rs`.
- No CSRF token: protection relies on SameSite=Lax plus JSON-only bodies (forms cannot send
  `application/json` cross-site). Revisit if any endpoint ever accepts form data.

## Session log

Newest first. One line per session: date · who · what changed · anything left half-done.

- 2026-10-09 · Claude (cloud) · Step 2.1: pgvector in `local-postgres.sh`, migration `0003_vector`, vector column test.

- 2026-10-09 · Claude (cloud) · Step 1 complete: auth, sessions, profile routes, rate limiting, sqlx offline cache. Merged step 0 to `master`.
- 2026-10-09 · Claude (cloud) · Step 0 complete: workspace, tooling, CI, docs. Legacy moved to `legacy/`.
