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
| **Current step** | Step 4: Grounded chat (4.1-4.3 done; auto tags/summary next) |
| **Last updated** | 2026-10-09 |
| **`just check`** | passing (223 Rust tests + 3 ignored OCR + 2 ignored real-model tests, 1 web test) |
| **Old code** | `legacy/` (read-only reference; deleted in step 7) |

## Next up

**Step 4.4: Auto tags and summary per file** (replaces the legacy Gemini calls in the worker).

1. Job `summarize_file` enqueued by `embed_file` when a file becomes `ready` (same
   transaction), only if a chat model is configured (`crate::llm::build`); idempotent
   (skip when the stored summary was made from the same `file_extractions` row/extractor
   version). Needs `JobContext` to carry the `ChatModel` (build it like `AppState`).
2. Migration: `files.summary text`, `files.auto_tags text[]` (keep user `tags` separate),
   `summary_model`. Prompt with the first N chunks (bounded characters), ask for a 2-3
   sentence summary + up to 5 tags as JSON; parse defensively, normalise with
   `normalize_tags`; use `ChatModel::complete` with low `max_tokens`. Strict offline:
   nothing to do (the provider is already local or none).
3. Expose on `GET /files/{id}` (+ list), allow filtering search by auto tags; tests with
   the fake model (extend `FakeChatModel` with a JSON mode, or a scripted test model).
4. Then: calibrate the real-reranker refusal threshold (`chat::evidence::default_min_score`,
   currently a lenient -3 guess on logits) by adding unanswerable questions to `eval/` and
   recording top rerank scores in `akasha eval --real-models` (the `eval.yml` workflow).

Chat follow-ups (not blocking): LLM-generated conversation titles (currently the first
question, shortened) via a job; per-conversation default file scope; a `fallback` from
`no_llm` to a provider outage (currently `error` with `llm_unavailable`).

Search follow-ups (not blocking): keyword search ANDs every word (`websearch_to_tsquery`),
so long natural-language queries find nothing by keyword (eval: keyword MRR 0.57 vs hybrid
0.82); consider an OR fallback when the AND query finds few chunks, and check it with
`akasha eval`. Commit a real-model baseline for the default models (e5-small + jina) from the
first `eval.yml` artifact (MiniLM's is committed and gated there).

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
- [x] Hourly expired-session pruning (a periodic job since step 2.4)

### Step 2: Files and ingestion ✅
- [x] Add `pgvector` (also teach `scripts/local-postgres.sh` to install it; the Docker image already has it)
- [x] Upload (streaming multipart, size limit, magic-byte check via `infer`, filename sanitising), per-user storage quota
- [x] Content-addressed storage (SHA-256, dedupe) behind `object_store` (local disk / S3), `crates/storage`
- [x] Blob ref-counting in SQL (with the `files` table; per-hash advisory lock)
- [x] Postgres job queue (`SELECT … FOR UPDATE SKIP LOCKED`; retries, backoff, idempotent jobs), `akasha worker`
- [x] New crate `crates/ingest`: PDF text (`pdf-extract`, ADR 0008), OCR (`ocrs`), plain text/markdown, chunking (`text-splitter`)
- [x] New crate `crates/ml`: embeddings via `fastembed` (multilingual-e5-small default, model + dimension recorded in the DB, `akasha reembed`)
- [x] File CRUD: list, get, rename, tags, pin, download, bulk delete, reindex, job status, extraction view, thumbnail (images; ADR 0011)

### Step 3: Search ✅
- [x] New crate `crates/search`: Postgres FTS (+ file names) + pgvector HNSW, fused with RRF (k=60); keyword/semantic/hybrid, degrade to keyword without a model
- [x] Cross-encoder rerank of the top 30 (`Reranker`), warnings instead of failures
- [x] Filters (type, date, tags, pinned, file ids; collection seam in `ChunkFilter`), snippets/highlights, pagination, "similar files", per-user rate limit
- [x] Spelling suggestion ("did you mean"): per-owner `user_terms` vocabulary kept by triggers, `pg_trgm` lookup, `suggestion` on search responses (ADR 0011)
- [x] `akasha eval` (corpus + queries in `eval/`, Recall@k/MRR/nDCG@10/latency, baselines), deterministic gate in `cargo test`, real-model tier in `eval.yml` (ADR 0011)

### Step 4: Grounded chat
- [x] LLM provider trait in new `crates/llm` (ADR 0012): Ollama (offline), Claude, Gemini, OpenAI-compatible, fake; strict offline = local providers only
- [x] Conversations + messages tables, streaming answers over SSE, inline citations
- [x] Refusal when evidence is weak (reranker score threshold, calibrated for `overlap`), tested
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
0006 Server first, desktop later · 0007 Job queue design ·
0008 Pure-Rust extraction (pdf-extract, ocrs, text-splitter) ·
0009 Embeddings via fastembed on runtime-loaded ONNX Runtime ·
0010 Hybrid search (FTS + pgvector, RRF, rerank) in `crates/search` ·
0011 Search eval tiers, spelling vocabulary, thumbnails ·
0012 LLM providers (`crates/llm`) and grounded chat.

## Known issues and gotchas

- The workspace has no license yet. Pick one before the first public release (`cargo-deny`
  already restricts *dependencies* to permissive licenses).
- `scripts/local-postgres.sh` uses the system Postgres (16 in cloud sessions) and installs
  pgvector for it (apt `postgresql-16-pgvector`, 0.6.x; falls back to building v0.8.0 from
  source). Docker/CI run pgvector 0.8 on Postgres 17, so do not rely on features newer than 0.6
  (HNSW is fine) without bumping the local install.
- Storage: `Storage` is a concrete struct over `Arc<dyn ObjectStore>` (object_store already is
  the backend trait; tests use `Storage::in_memory()`). Keys: `blobs/ab/cd/<sha256>` and
  `staging/<uuid>`. Commit = `head` + `rename` (copy+delete on S3); overwriting identical content
  is harmless, so racing uploads of the same file are safe without conditional writes.
- `object_store` `aws` feature pulls in `aws-lc-rs` (C build via `cc`, no cmake needed). Docker
  image stores blobs in `/var/lib/akasha/storage` (volume `storage` in `compose.yaml`).
- S3 multipart parts are 5 MiB, so a streaming upload buffers up to ~5 parts (25 MiB) in memory per upload.
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

- Files: uploads are streamed (multipart → `StagedBlob`), sniffed from the first 8 KiB with
  `infer` against an allow-list (`crates/app/src/files/sniff.rs`); text types have no magic
  bytes, so they need a text extension (or none) and must be UTF-8 without NUL bytes over the
  whole stream. HTML/Office/archives are rejected (415); add types to the allow-list deliberately.
- Blob lifecycle: upload and delete both take `pg_advisory_xact_lock` on the content hash
  (`akasha_db::files::lock_hash`) and do their storage work inside that transaction (upload uses
  `StagedBlob::finish` → `FinishedBlob::commit` so the blob only appears under the lock). See the
  module doc in `crates/app/src/files/store.rs` for the race analysis. Account deletion cascades
  the rows, then releases each blob.
- Re-uploading identical bytes returns the existing file with **200** (new: 201) and keeps the
  original name. Quota (`users.storage_quota_bytes`, NULL = unlimited, no API to set it yet) is
  checked after streaming, under a lock on the user row, so duplicates are free; exceeded →
  413 `quota_exceeded`. Size limit (`AKASHA_MAX_UPLOAD_MB`, default 512) → 413 `payload_too_large`.
- The 30 s request timeout no longer applies to upload/download routes (they get 1 h, see
  `routes.rs`). Uploads are not rate-limited yet; add a per-user limiter if abuse shows up.
- Another user's file is always 404 (never 403). Every `akasha_db::files` query takes `owner_id`.

- Jobs (`crates/jobs`, ADR 0007): delivery is at least once, so handlers must be idempotent.
  Workers only claim kinds they have a handler for; others wait in `queued` (logged once at
  start). Status flow: queued → running → succeeded | failed (retry at `run_at`) | dead.
  Dedupe (`Job::dedupe_key`) only blocks a second *queued* job; running/retrying ones do not.
  `make_interval(secs => ...)` takes `float8`: pass `Duration::as_secs_f64()`.
- Periodic jobs live in `job_schedules` (`jobs::schedules()` in the app): prune-sessions and
  prune-staging hourly (staging max age 2 h, above the 1 h upload timeout), sweep-orphan-blobs
  and prune-jobs daily (succeeded kept 7 d, dead 30 d). Without a running worker none of this
  happens: `serve` warns when started without `--with-worker`.
- File deletion (single, bulk, account) never touches storage: it enqueues
  `delete_blob_if_unreferenced` in the same transaction; the handler re-checks under the hash
  lock. HTTP tests must call `app.run_jobs().await` before asserting a blob is gone.
- The worker needs `concurrency + 2` pool connections (claims, heartbeats, `LISTEN`); with
  `serve --with-worker` they share `AKASHA_DB_MAX_CONNECTIONS` with the API.

- Extraction (ADR 0008): offsets (`char_start/char_end`, page spans, `?offset=&limit=`) are
  Unicode *characters* into `file_extractions.text` (Postgres `substr` semantics), not bytes
  and not JS UTF-16 units. Chunks never cross PDF pages; `page` is NULL for unpaged formats.
- `extract_file` reads the whole blob into memory (PDF parsing needs it; text is capped at
  10M chars). Media/unsupported types skip the read entirely and end `ready` with extractor
  `none` and a note. A retryable failure leaves the file `processing` until the job's last
  attempt (`akasha_jobs::current_attempt()`), then `failed` with a user-safe message; the
  technical detail goes to `jobs.last_error`.
- OCR models are fetched on first use into `AKASHA_MODELS_DIR` (checksums pinned in
  `crates/ingest/src/models.rs`; update both digests when bumping `ocrs`). Tests use
  `ocr_enabled = false` (`support::test_config()`); real-model tests are `#[ignore]`:
  `cargo test -p akasha-ingest --test ocr -- --ignored` (downloads into `target/ocr-models`).
  Text PDFs never load OCR; only images and PDFs with text-less image pages do.
- pdf-extract panics on some malformed PDFs; every parser call in `crates/ingest` goes
  through `error::guard` (`catch_unwind`). Panics still print to stderr via the default hook.
- `file_chunks.tsv` uses the `english` config (legacy parity). Changing it means a new
  generated column + reindex, decide in the search step.

- ML (ADR 0009): ONNX Runtime is **loaded at run time** (`ort` load-dynamic), never linked
  or downloaded at build time. Real models need `AKASHA_ORT_DYLIB_PATH` (`just onnxruntime`
  installs the pinned 1.28.0 into `./models/onnxruntime/`); the Docker image has it in
  `/usr/local/lib`. Without it embed jobs fail retryably with a clear message. Tests use the
  `hash-384` embedder / `overlap` reranker (`support::test_config()`); real-model tests:
  `ORT_DYLIB_PATH=... cargo test -p akasha-ml --test real_models -- --ignored`
  (`AKASHA_TEST_EMBED_MODEL`, `AKASHA_TEST_MODELS_DIR`, `AKASHA_TEST_MODELS_URL`).
- Cloud sessions: the agent proxy blocks `huggingface.co` and `cdn.pyke.io` (403), so real
  models cannot be downloaded here; GitHub release downloads and the chroma S3 bucket work
  (all-MiniLM-L6-v2 from `chroma-onnx-models.s3.amazonaws.com/all-MiniLM-L6-v2/onnx.tar.gz`
  unpacked into `<models>/hf/Qdrant--all-MiniLM-L6-v2-onnx/` runs offline).
- `file_chunks.embedding` is `vector(384)` from migration 0007, but `akasha reembed` can
  re-type it to another dimension: never assume 384 in later migrations. The column dim is
  read from `pg_attribute.atttypmod` (`akasha_db::embeddings::column_dim`). Startup refuses
  a configured model different from `embedding_model`; tests insert there to simulate it.
- Pass vectors to SQL as `real[]` and cast (`$1::real[]::vector`); the checked macros do
  not know the `vector` type. Read them back with `embedding::real[]`.
- File status now stays `processing` until `embed_file` finishes; `run_jobs()` returns 2 for
  a text upload (extract + embed). `GET /files/{id}` `processing.stage` is `extract`|`embed`.

- Search (ADR 0010): SQL in `akasha_db::search` (every query filters `file_chunks.owner_id`
  *and* joins `files.owner_id`), pipeline in `crates/search`, HTTP in `routes/search`.
  `GET /search` = file-grouped (UI), `GET /search/chunks` = chunk-level with full text (agents,
  RAG uses the crate directly). Offset paging over the fused list, capped at 200 results.
- Vector search scans exactly when the owner has ≤ 10 000 embedded chunks (or `file_ids` is
  set); above that it uses HNSW with `ef_search` ≥ 100 and, on pgvector ≥ 0.8 only,
  `hnsw.iterative_scan` (0.6 errors on that setting once the extension is loaded, so it is
  version-gated). The 10k-chunk test drops the HNSW index, bulk-inserts, then rebuilds it
  (~4 s instead of ~20 s: inserting into a live HNSW index row by row is slow).
- Snippets: `ts_headline` with U+E000/U+E001 markers, parsed into plain text + highlight
  offsets in Unicode characters (never HTML). Fragment mode may drop leading words of a
  short chunk ("The aardvark…" → "aardvark…").
- Search never waits more than 5 s for a model (`MlProvider::embedder_within`); while it
  loads or if it cannot load, searches run as keyword with `degraded: true`. `serve` now warms
  both search models at start. Warnings shown to users are generic; details are logged.
- Per-user search limit `AKASHA_SEARCH_RATE_PER_MINUTE` (default 30, 0 = off), governor
  keyed by user id, checked in the handler (`rate_limit::check_user`). Tests share one limiter
  per `TestApp`; keep a single test under 30 searches per user.
- Search queries are not logged (only their length).

- Eval (ADR 0011): `cargo test` runs the `eval/` benchmark with `hash-384`/`overlap` and
  fails if any quality metric drops > 0.02 below `eval/baselines/hash-384+overlap.json`.
  After an intended ranking change: `cargo run -p akasha -- eval --update-baseline`, review
  the diff, commit. `akasha eval` creates and drops a scratch database (`akasha_scratch_*`,
  needs CREATEDB) next to `DATABASE_URL`. Adding corpus files or queries changes every
  number: re-record the baselines (both tiers) in the same commit and update `eval/README.md`.
- Real-model eval here: `just onnxruntime`, put all-MiniLM-L6-v2 from the chroma S3 bucket
  into `models/hf/Qdrant--all-MiniLM-L6-v2-onnx/`, then `AKASHA_ORT_DYLIB_PATH=$PWD/models/onnxruntime/libonnxruntime.so
  AKASHA_EMBED_MODEL=all-minilm-l6-v2 AKASHA_RERANK_MODEL=none AKASHA_ML_MODELS_URL=
  cargo run --release -p akasha -- eval --real-models`.
- Spelling: `user_terms` is maintained only by triggers on `file_chunks` (migration 0008);
  never write it from code. Chunks must not be UPDATEd in place (no UPDATE trigger):
  re-extraction deletes and re-inserts. The triggers take a per-owner advisory lock
  (`pg_advisory_xact_lock(1433, hashtext(owner))`), so one owner's chunk writes serialise.
- `scripts/local-postgres.sh` now creates UTF-8 clusters. An older SQL_ASCII cluster
  (it warns) splits non-ASCII words ("résumé" → "sum"): `pg_ctl -D /var/lib/postgresql/akasha-dev stop`,
  delete the directory, rerun the script. `pg_trgm`/`btree_gin` come from contrib.
- Thumbnails: `make_thumbnail` is keyed by content hash and only enqueued on image upload
  and reindex; images uploaded before 2026-10-09 get one after a reindex. Objects live at
  `thumbs/ab/cd/<hash>/256` and are deleted with the blob. `run_jobs()` now returns one
  more job per new image upload.

- Chat (ADR 0012): providers live in `crates/llm` (async, `reqwest` streaming), not
  `crates/ml` (blocking ONNX). `serve` refuses to start on an unusable provider config
  (`llm::build`: strict offline violation, missing key); `AppState::new` logs it and runs
  without a model. Tests use `AKASHA_LLM_PROVIDER=fake` (`support::test_config()`) and
  `TestApp::with_llm` to inject a model; provider wire tests run against local axum mock
  servers (`crates/llm/tests/mock`). Never call a real provider from tests.
- The chat answer runs in a spawned task feeding the SSE body through a channel; the 30 s
  request timeout only covers the response head. Disconnect = channel closed = model
  stream dropped, answer stored as `cancelled`. Clients should use `done.content` as the
  final text (a refusal replaces streamed partial output).
- Refusal gate thresholds are per reranker (`chat::evidence::default_min_score`). The
  `overlap` test reranker now ignores stopwords and plural `s` (changed in step 4; the
  deterministic eval baseline was re-recorded, hybrid_rerank MRR 0.88 → 0.94). Keep
  `tests/chat_gate.rs` passing when touching it. The ONNX default (-3) is uncalibrated.
- Anthropic: current models reject `temperature`, so it is never sent; `fallbacks:
  "default"` + beta header only for the models in `anthropic::FALLBACK_MODELS`. Gemini keys
  go in `x-goog-api-key`, never the URL. OpenAI-compatible base URLs include `/v1`.
- `Config` is no longer `Eq` (it has `f32` fields). Config types live in `core/src/config/types.rs`.

## Session log

Newest first. One line per session: date · who · what changed · anything left half-done.

- 2026-10-09 · Claude (cloud) · Step 4.1-4.3: new crate `crates/llm` (async streaming `ChatModel`; Ollama NDJSON, Anthropic Messages SSE, Gemini SSE with header key, OpenAI-compatible SSE; retries/backoff, read timeouts, strict offline check, redacted keys, deterministic fake, mock-server wire tests), migration 0009 (conversations, messages), conversation CRUD + `POST /conversations/{id}/messages` streaming SSE (`sources`/`delta`/`done`/`error`) with `[n]` citations, follow-up rewriting, refusal gate (calibrated `overlap` threshold; overlap reranker now ignores stopwords, eval baseline re-recorded), `no_llm` mode, per-user chat rate limit, cancellation on disconnect, ADR 0012, README "Chat & LLM providers". Auto tags/summary (4.4) not started.

- 2026-10-09 · Claude (cloud) · Step 3 done + step 2 thumbnails. `akasha eval` (32-doc corpus, 46 queries incl. the ported legacy benchmark, Recall@1/5/10, MRR, nDCG@10, latency, scratch database, baselines with tolerance), deterministic gate in `cargo test`, nightly/on-demand `eval.yml` for real models (MiniLM baseline committed, `eval/README.md`); spelling suggestions (`user_terms` + triggers, `pg_trgm`/`btree_gin`, migration 0008, `suggestion` on search responses); thumbnails (`make_thumbnail` job, `akasha_ingest::thumbnail` with decode-bomb limits, `thumbs/` objects, `GET /files/{id}/thumbnail` with ETag/304); 10k-chunk test 20 s → 4 s; local Postgres now UTF-8; ADR 0011. (Finished in a second session after a usage-limit cut: sqlx cache, openapi, MiniLM baseline, eval README.)

- 2026-10-09 · Claude (cloud) · Step 3.1–3.3: search core. `crates/search` (keyword + file-name FTS, pgvector semantic with exact/HNSW switch and ef_search/iterative scan, RRF k=60, rerank top 30, file grouping, `ts_headline` snippets with highlight offsets, per-stage timings, degrade-to-keyword), `akasha_db::search`, `GET /search`, `GET /search/chunks`, `GET /files/{id}/similar`, per-user search rate limit, ADR 0010. Eval harness, spelling suggestions and thumbnails still open.

- 2026-10-09 · Claude (cloud) · Step 2.6: embeddings. `crates/ml` (Embedder/Reranker traits, catalog with per-model prefixes, HF downloader, hash/overlap fakes, fastembed on runtime-loaded ONNX Runtime), `0007_embeddings` (`embedding vector(384)` + HNSW, `embedding_model`), `embed_file` job (batched, resumable, model-guarded), startup model guard, `akasha reembed`, `akasha models download|check`, ORT 1.28 in the Docker image + CI smoke test, `just onnxruntime`, ADR 0009. Real-model test run locally with all-MiniLM-L6-v2 (HF blocked here; e5/jina defaults unverified locally, CI smoke test covers MiniLM).

- 2026-10-09 · Claude (cloud) · Step 2.5: extraction. `crates/ingest` (text/Markdown/CSV/JSON normalisation, per-page PDF text via pdf-extract with panic guards, scanned-page detection + OCR of embedded JPEG/bitmap scans, image OCR with ocrs and checksummed download-on-first-use models, character chunking with overlap and page/char offsets), `0006_extraction`, `extract_file` handler (idempotent replace, last-attempt failure via new `akasha_jobs::current_attempt()`), `GET /files/{id}/extraction`, ADR 0008, `AKASHA_OCR_ENABLED`/`AKASHA_MODELS_DIR`/`AKASHA_OCR_MODELS_URL`, models volume in Docker.
- 2026-10-09 · Claude (cloud) · Step 2.4: job queue. `0005_jobs` (jobs + job_schedules, NOTIFY trigger), `crates/jobs` (claim with SKIP LOCKED, heartbeat/visibility timeout, backoff with jitter, dead-lettering, typed jobs, LISTEN wake-up, graceful drain), `akasha worker` + `serve --with-worker` (Docker default), periodic session/staging/job pruning + orphan-blob sweep, blob deletion moved into a job enqueued in the delete transaction, `extract_file` enqueued on upload (no handler until 2.5), `GET /files/{id}` shows `processing`, `POST /files/{id}/reindex`.

- 2026-10-09 · Claude (cloud) · Step 2.3: `0004_files` (+ `users.storage_quota_bytes`), streaming upload with magic-byte allow-list, filename sanitising, dedupe, quota, blob ref-counting with per-hash advisory locks, file list/get/patch/delete/bulk-delete/download, legacy attack cases ported. New files stay `pending` until the job queue (2.4).
- 2026-10-09 · Claude (cloud) · Step 2.2: `crates/storage` (content-addressed, local/S3, streaming staging, prune), `AKASHA_STORAGE_*` config with redacted secrets, `AppState.storage`.
- 2026-10-09 · Claude (cloud) · Step 2.1: pgvector in `local-postgres.sh`, migration `0003_vector`, vector column test.

- 2026-10-09 · Claude (cloud) · Step 1 complete: auth, sessions, profile routes, rate limiting, sqlx offline cache. Merged step 0 to `master`.
- 2026-10-09 · Claude (cloud) · Step 0 complete: workspace, tooling, CI, docs. Legacy moved to `legacy/`.
