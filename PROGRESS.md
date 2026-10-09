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
| **Current step** | Step 1: Auth and users |
| **Last updated** | 2026-10-09 |
| **`just check`** | passing |
| **Old code** | `legacy/` (read-only reference; deleted in step 7) |

## Next up

**Step 1: Auth and users.** Concretely:

1. Migration `0002_users_sessions`: `users` (uuid id, citext email unique, argon2 `password_hash`,
   `display_name`, timestamps with `set_updated_at` trigger) and a session table for
   `tower-sessions` (`tower-sessions-sqlx-store`).
2. Code goes in `crates/db/src/users.rs` (queries) and `crates/app/src/routes/auth.rs` (HTTP).
   Only split into a separate crate if it grows past ~300 lines.
3. Routes under `/api/v1/auth`: `POST register`, `POST login`, `POST logout`, `GET /api/v1/me`.
   Cookie sessions (HttpOnly, SameSite=Lax, Secure in prod), not JWT. See ADR 0004.
4. Rate-limit auth routes (`tower_governor`).
5. An `AuthUser` extractor that returns `unauthorized` in the standard error shape.
6. Tests: `#[sqlx::test]` for the repo functions; HTTP tests for register → login → me → logout,
   wrong password, duplicate email.
7. `just openapi`, tick the boxes below, add a session-log line.

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

### Step 1: Auth and users
- [ ] users + sessions migration
- [ ] register / login / logout / me, argon2, cookie sessions
- [ ] `AuthUser` extractor, rate limiting
- [ ] Profile: display name, change password, delete account

### Step 2: Files and ingestion
- [ ] Add `pgvector` (also teach `scripts/local-postgres.sh` to install it; the Docker image already has it)
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
- `scripts/local-postgres.sh` uses the system Postgres (16 in cloud sessions), which lacks
  pgvector. Fix that in step 2 (build pgvector or download it).
- First `cargo build` takes ~3 minutes; dependencies are compiled with `opt-level = 2`.

## Session log

Newest first. One line per session: date · who · what changed · anything left half-done.

- 2026-10-09 · Claude (cloud) · Step 0 complete: workspace, tooling, CI, docs. Legacy moved to `legacy/`.
