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
| **Current step** | Step 6: Beyond parity (6a MCP + API tokens, 6b collections + activity + audit log, 6c trusted proxies + audio/video transcription done) |
| **Last updated** | 2026-10-10 |
| **`just check`** | passing (332 Rust tests + 6 ignored OCR/real-model/Whisper tests, 104 web tests, bundle budget 174/180 kB); `just e2e` 11 Playwright tests (incl. axe on every screen, tokens + `/mcp`, collections + sessions, recordings) |
| **Old code** | `legacy/` (read-only reference; deleted in step 7) |

## Next up

**Step 6: beyond parity.** Step 5 (web UI) is complete. In order; each ends with
`just check` green, `just e2e` green and a PROGRESS.md update:

Done in step 6 so far: **MCP server + personal API tokens** (6a, ADR 0015);
**collections, activity timeline, security log, sessions and open tracking** with their
screens (6b, ADR 0016); **trusted reverse proxies** and **audio/video transcription**
(whisper.cpp, timestamps in search and citations, transcript player; 6c, ADR 0017).

1. **Observability**: Prometheus `/metrics` (search latency, job queue depth, model load
   state from `system::status`, transcription progress), OpenTelemetry export behind a feature.
2. **Watched folders / connectors** (Obsidian vault, Downloads) via the job queue.

Transcription follow-ups: run a real Whisper model on real speech (only the fake model ran
here: Hugging Face is blocked in cloud sessions; CI's Docker smoke test runs `tiny` on a
tone) and measure speed per model; consider word-level timestamps for finer seeking,
speaker turns, and an optional ffmpeg fallback for AC-3/AVI/WMA.

Open follow-ups:
- Tune the unmeasured relevance floors (e5-small 0.80, bge 0.55, nomic/bge-m3 0.45, ONNX
  rerankers -2.0) from the nightly `eval.yml` real-model artifact (it now reports P@10 and
  `negative_clean`); calibrate the real-reranker refusal threshold the same way.
- Commit a real-model eval baseline for the default models; consider an OR fallback for
  keyword search (it ANDs every word).
- MCP follow-ups: try the HTTP endpoint with real Claude Code / Claude Desktop (only raw
  JSON-RPC and the stdio bridge against a local server were exercised here).
- Chat: `no_llm` fallback on a provider outage. PWA offline shell (service worker) was
  deferred: the app is installable (manifest + icons) but needs the server to work.

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

### Step 4: Grounded chat ✅
- [x] LLM provider trait in new `crates/llm` (ADR 0012): Ollama (offline), Claude, Gemini, OpenAI-compatible, fake; strict offline = local providers only
- [x] Conversations + messages tables, streaming answers over SSE, inline citations
- [x] Refusal when evidence is weak (reranker score threshold, calibrated for `overlap`), tested
- [x] Auto tags/summary per file (`enrich_file`, separate `auto_tags`; replaces the legacy Gemini calls), model-written conversation titles, refusal-gate calibration report in `akasha eval` (ADR 0013)

### Step 5: New web UI (redesign) ✅
- [x] Design system first (tokens, type scale, light + dark), documented in `web/DESIGN.md`
- [x] TanStack Router + Query, shadcn/ui on Tailwind 4, typed client from `openapi.json`
- [x] Screens: auth, settings (+ System status), 404, per-screen error boundaries, app
      shell, first-run onboarding, library, file detail, search (relevance floor with a
      "Loosely related" section) and chat (stored file scope, edit and ask again).
      Collections and activity moved to step 6 with their APIs (none exist yet)
- [x] Polish (5d): axe-core in Playwright over every screen in light + dark (contrast
      fixes), skip link/focus/landmarks/live regions, `?` shortcuts sheet, page titles,
      consistent toasts, optimistic tags/pins/scope, gzip/brotli UI assets, favicon + PWA
      manifest, initial JS budget 180 kB gzip checked on every build (171 kB)
- [x] Route-level code splitting (`lazyRouteComponent`) and long-lived vendor chunks
- [x] Rust binary serves the built UI (`rust-embed`, feature `embed-ui`), so production is a single binary
- [x] Playwright end-to-end tests in CI (auth, library, search, chat, accessibility)

### Step 6: Beyond parity
- [x] MCP server (`rmcp`, Streamable HTTP at `/mcp` + `akasha mcp` stdio bridge) and personal API tokens (scopes, expiry, revocation, Settings → Access tokens)
- [x] Audio/video transcription (whisper.cpp via `whisper-rs`, pure-Rust decoding, timestamps in search/citations/MCP, transcript player; ADR 0017)
- [x] Trusted reverse proxies (`AKASHA_TRUSTED_PROXIES`: client IP for rate limits, security log, sessions; `Secure` cookies via `X-Forwarded-Proto`)
- [x] CPU-portable, multi-arch image: amd64 runs on any SSE4.2 CPU and switches to an AVX2 build at start-up; linux/arm64 (Raspberry Pi 4/5); release workflow to ghcr.io
- [x] Watched folders (`AKASHA_WATCH_ROOTS`, Settings → Sources, scan job + `notify` watcher, renames, front-matter tags; ADR 0018). Other connectors (IMAP, WebDAV, cloud drives) deferred
- [x] Collections, activity timeline, security audit log, sessions, open tracking (legacy parity; API + screens, ADR 0016)
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
0012 LLM providers (`crates/llm`) and grounded chat ·
0013 Model-written file summaries, suggested tags and conversation titles ·
0014 Relevance floor for results found by meaning alone ·
0015 MCP server (stateless Streamable HTTP, stdio bridge) and personal API tokens ·
0016 Collections, activity timeline and security audit log ·
0017 Audio/video transcription (whisper.cpp, Symphonia + pure-Rust Opus, timestamps).

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
- Client addresses (`crates/app/src/client_ip.rs`): one middleware in `app()` resolves the
  client once per request into a `ClientAddr` extension; the credential limiter
  (`ClientIpKeyExtractor`), `ClientMeta` (activity/security log, sessions) read it. Proxy
  headers count only from peers in `AKASHA_TRUSTED_PROXIES` (CIDRs/addresses, empty =
  nobody): rightmost untrusted `X-Forwarded-For` hop; `Forwarded` only without XFF (a proxy
  that sets only XFF cannot be bypassed with a forged `Forwarded`). `X-Forwarded-Proto: https`
  from a trusted proxy makes session cookies `Secure` even with `AKASHA_COOKIE_SECURE=false`.
  Never read `ConnectInfo` or forwarding headers directly in handlers.
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
- Anthropic default model is `claude-sonnet-5-5` (since step 4.4; cheaper and faster for
  short grounded answers); `AKASHA_LLM_MODEL=claude-opus-5-5` selects Opus.

- Enrichment (ADR 0013): `enrich_file` is queued by `embed_file` only when the file goes
  `processing → ready` (`Finish::Done { became_ready }`) and a model is configured, so with
  the default test config (fake model) a text upload now runs **3** jobs (extract, embed,
  enrich); a re-embed adds an enrich job that skips (no model call). `TestApp::run_jobs`
  uses the app's chat model (`TestApp::with_llm(.., None)` → no enrichment). Tests that
  count model calls must reset their counter after seeding files.
- Never write `files.tags` from a job: those are the user's. Model output goes to
  `auto_tags`; tag filters use `tags || auto_tags`. `enriched_from` must equal
  `file_extractions.created_at` (an upsert that keeps `created_at` would break staleness
  detection, so extraction sets `created_at = now()` on conflict; keep it that way).
- `ChatRequest.json`: Ollama/Gemini JSON mode only; not every OpenAI-compatible server accepts
  `response_format: json_object`, and Anthropic structured outputs need a schema, so those
  rely on the prompt and `enrich::parse` (defensive). `FakeChatModel` in JSON mode skips the
  first prompt line (the file-name header) and answers first sentence + top-3 words.
- Conversation titles: `conversations.title_source` (`user|question|model`); only a
  `question` title is replaced by `title_conversation`. Renames set `user`.
- `akasha eval` forces `llm_provider = none` and prints a refusal-gate report from
  `eval/gate.json` when a reranker is loaded. To calibrate a real reranker: get the model
  (`akasha models download`, needs Hugging Face), run `AKASHA_RERANK_MODEL=<model> cargo run
  --release -p akasha -- eval --real-models`, read "best on this set", choose a value a bit
  lower (refusing answerable questions is worse), put it in
  `chat::evidence::default_min_score` for that model (or `AKASHA_CHAT_MIN_RERANK_SCORE`),
  keep `tests/chat_gate.rs` green and record the numbers in `eval/README.md`. The ONNX
  default (-3 logits) is still a guess.
- Compose: the app container reaches host Ollama via `host.docker.internal`
  (`extra_hosts: host-gateway`); host Ollama must listen on `0.0.0.0`. `--profile ollama`
  runs `ollama/ollama:0.12.3` as service `ollama` (set `AKASHA_OLLAMA_URL=http://ollama:11434`).

- Web UI (step 5): `web/DESIGN.md` is the source of truth; colours are CSS variables in
  `src/styles/tokens.css` named without the `--color-` prefix (`--bg`, `--fg-muted`, ...) and
  mapped to Tailwind in `app.css` (`@theme inline`), because Tailwind owns the `--color-*`,
  `--shadow-*`, `--radius-*` namespaces. Use `bg-surface`, `text-fg-muted`, etc.
- The server's CSP forbids inline scripts: the pre-paint theme script is the external
  `web/public/theme-init.js`. `style-src` allows `'unsafe-inline'` (Radix injects styles).
  The e2e test fails on any CSP console error, so new libraries that need more show up there.
- The UI is embedded only with `cargo build --features embed-ui` (needs `web/dist`, i.e.
  `pnpm build` first; rust-embed with `debug-embed`, so debug builds embed too). Without
  the feature, non-API paths are JSON 404s (CI rust job, `cargo test`). Serving logic is
  tested with `WebAssets::from_files` (`crates/app/tests/web.rs`). Unknown `/api/*` paths
  never fall back to `index.html`; missing files with an extension are 404 (stale chunks).
- `GET /api/v1/me` 401 is "not signed in": `meQuery` returns `null`. Route guards
  (`router.tsx`) read it; sign-in/out set it (`lib/session.ts`). The client's
  `onUnauthorized` (any other 401) only acts when a user was cached, so the bootstrap 401 is
  quiet; wrong-password 401s (login, password change, account delete) are excluded.
- Accessibility (5d): `e2e/a11y.spec.ts` runs axe (WCAG 2.2 AA + best practices) on every
  main screen in light and dark with one account (credential calls are rate-limited). New
  screens: add them there. `@axe-core/playwright` is MPL-2.0 (dev-only, web). Chat answers
  announce progress and the final text through one polite live region (`announcement()`
  in `answer.tsx`), never per streamed delta.
- Bundle budget: `pnpm build` ends with `scripts/check-bundle.mjs` (entry + preloaded
  chunks, gzip, 180 kB; `BUNDLE_BUDGET_KB` overrides). Load new dialogs lazily
  (`React.lazy`), like the palette and the shortcuts sheet.
- Page titles: `useDocumentTitle()` in each screen ("Library · Akasha"). Screen errors
  render `RouteError` inside the shell (`defaultErrorComponent`); layouts keep `ErrorPage`.
- Radix toasts duplicate their text into a live region: in Playwright use
  `getByText(.., { exact: true })`.
- Playwright is pinned to 1.56.1 to match the preinstalled Chromium (revision 1194 in
  `/opt/pw-browsers`); CI installs its own browser. Bump both together. `just e2e` starts
  `target/debug/akasha` on port 8091 (`AKASHA_E2E_PORT`) against `DATABASE_URL`, with a
  temp storage dir. Credential endpoints are rate-limited per IP (burst 10, then 1/6 s): keep
  the e2e suite under ~10 sign-in/register/password calls or tests start getting 429s.
- Library (step 5.2): `GET /files` takes `sort` (`newest|oldest|name|size`); the cursor
  embeds the sort (`n|o|a|s:<id>:<key>`) and is rejected for another sort. Name order is
  `lower(original_name), id`. `GET /api/v1/tags` lists tags with own/suggested counts.
  Conversation/message cursors moved to `routes/cursor.rs`. Every route now has a unique
  `operationId` (test in `tests/http.rs`): openapi-typescript keys operations by it.
- `GET /files/{id}/download?inline=true` (a bool: `inline=1` is a 400) is honoured only
  for PDFs, raster images, audio, video and text/plain; inline responses get
  `frame-ancestors 'self'` + `X-Frame-Options: SAMEORIGIN`, and PDFs drop `sandbox`
  (Chrome's PDF viewer refuses sandboxed documents). `nosniff` and the stored type stay.
- Uploads go through XHR (`features/upload/xhr-upload.ts`) for progress + abort; the queue
  (`upload-queue.ts`, 3 at a time, size pre-check against `meta.max_upload_bytes`) is a
  plain class read via `useSyncExternalStore`. New files are put into every cached list
  page (`features/files/cache.ts`), then lists poll every 2 s only while a loaded file is
  `pending`/`processing` (`pollWhileProcessing`). The file page also polls ~90 s after
  `ready` for the summary.
- Markdown previews use our own tiny renderer (`lib/markdown.tsx`: headings, lists, code,
  quotes, emphasis, links with http(s)/mailto only); it never emits raw HTML.
- Library view and sort are per-browser preferences (`localStorage`); filters live in the
  URL (`?category=&tag=&pinned=`).
- The Docker image builds `web/` in a `node:22` stage and embeds it; the Docker build could
  not be run in the cloud sandbox (no daemon), CI's docker job covers it.

- Search UI (5c): all state is in the URL (`features/search/search-params.ts`,
  `validateSearch`); the box debounces 300 ms and *replaces* the history entry while typing,
  filters/page/suggestion *push*. `searchQuery` passes the AbortSignal (stale requests are
  cancelled), keeps previous results (`keepPreviousData`), never retries 4xx and retries a
  `rate_limited` search after the server's "retry in Ns". Highlight offsets are code points:
  `lib/highlight.ts` converts them (emoji/astral characters count once on the server).
- Passage links: `/files/<id>?at=<start>-<end>&page=<n>` (`features/files/passage.ts`), from
  search results and citations. The file page opens the Text tab, loads further extraction
  windows until the passage is loaded (max 20 × 50k chars), marks it (`mark[data-passage]`)
  and scrolls to it; PDFs open at `#page=n`.
- Chat UI (5c): answers stream via `fetch` + `lib/sse.ts` (EventSource cannot POST);
  `openapi-fetch` with `parseAs: "stream"` keeps the typed client and session-expiry
  middleware. A stream that ends without `done`/`error` is `stream_interrupted`. Live answers
  live in `ChatSessions` (`features/chat/chat-session.ts`, held by the `/chat` layout route),
  so a new chat keeps streaming across `/chat` → `/chat/<id>`; leaving the chat area or
  pressing stop aborts (server stores `cancelled`). After an answer settles the messages are
  refetched, the live turn is dropped, and the list is refreshed at 2.5 s/6 s for the
  model-written title. Stored messages with the live turn's ids are hidden (no duplicates).
- `/chat` routes render full-bleed: `AppShell` drops the page padding for `/chat*` and the
  chat layout sizes itself (`100dvh` minus the phone header and tab bar).
- Conversation scope (5d, migration 0011): `conversations.file_ids` (≤ 100; ids are
  filtered to the owner's files on write, a later-deleted file just stops matching).
  `PATCH /conversations/{id}` takes `title` and/or `file_ids` (`[]` = all files). A question
  without `file_ids` uses the conversation's; one with `file_ids` overrides it for that turn.
- Relevance floor (ADR 0014): semantic-only results below the model's floor
  (`EmbedModel::min_similarity`, or `RerankModel::min_score` when reranked) are dropped and
  counted in `loosely_related`; `include_weak=true` returns them last with
  `loosely_related: true`. Tests that relied on "semantic returns every file" now pass
  `include_weak`. Chat retrieves without them; the gate calibration in `akasha eval` keeps
  them. Eval queries of kind `negative` list no relevant files; `negative_clean` and
  `precision@10` are gated like recall.
- `GET /api/v1/system/status` never loads a model (`MlProvider::{embedder,reranker}_state`
  reports ready / not loaded / last load error). Queue numbers are server-wide counts only.
- UI assets are compressed (gzip/brotli via `tower-http` `CompressionLayer`, only on the
  UI fallback; API responses, downloads and SSE are not compressed).
- Textless files (images without OCR text, media) now get `enrichment_status = skipped` in
  the extraction transaction (`enrichment::skip_current`), so the UI never shows "Writing a
  summary" for them; the timeline also treats ready files whose last job is `extract` as
  textless (older data).
- Screenshots in cloud sessions: a tiny OpenAI-compatible mock server (scratch, not
  committed) with `AKASHA_LLM_PROVIDER=openai AKASHA_OPENAI_BASE_URL=http://127.0.0.1:8093/v1`
  gives realistic streamed answers; Playwright scripts must live under `web/` to resolve
  `@playwright/test`. Never `pkill -f` a pattern that also matches your own shell command.

- API tokens (6a, migration 0012): `akasha_pat_` + 43 base64url chars, SHA-256 at rest,
  prefix shown in the list. `AuthUser` takes `Authorization: Bearer` before the cookie; a
  read-only token gets 403 on any non-GET/HEAD in the extractor (so `POST` chat needs a write
  token; MCP `ask` does not). `SessionUser` (cookie only) guards `/me/tokens`, `PATCH /me`,
  password, account deletion and logout. `AuthUser.session_id` is gone: use `SessionUser`
  when a handler needs the session.
- MCP (6a, ADR 0015, `crates/app/src/mcp/`): `rmcp` pinned `=3.5.1`, stateless + JSON
  responses; rmcp rejects requests without a `Host` header (400), so tests set one
  (`tests/support/mcp.rs`). The handler gets the user from
  `ctx.extensions::<http::request::Parts>().extensions::<AuthUser>()`, put there by the
  `/mcp` middleware. `/mcp` has its own 150 s timeout (outside the 30 s layer). Tool errors
  are `isError` results; unknown tools are JSON-RPC errors. `akasha mcp` is handled before
  `Config::load` and writes only protocol to stdout (logs to stderr). Tool argument schemas
  come from `schemars` 1 (`uuid1`, `chrono04` features) via `schema_for_type`.

- Collections + activity (6b, ADR 0016, migrations 0013/0014): `collection_files` has
  composite FKs `(collection_id, owner_id)` / `(file_id, owner_id)` (new unique keys
  `files (id, owner_id)`, `collections (id, owner_id)`), so cross-owner rows are impossible.
  `ChunkFilter.collection_id` / `ListFilter.collection_id` / `?collection_id=`; routes 404 on
  a collection that is not the caller's (`routes::collections::ensure_owned`).
- `activity_events` is the timeline *and* the audit log (`category = 'security'`). Record
  with `crate::activity::{event, record}` inside the action's transaction, or
  `record_best_effort` outside one. New kinds: add an `ActivityKind` variant (with its `serde`
  name, `as_str` and `category` arms) and a UI label (`features/activity/describe.tsx`). `created_at` defaults to `clock_timestamp()`
  (ordering within a transaction). Failed sign-ins are written from a spawned task (no
  timing oracle); tests poll for them. Opens are deduped per file per 30 min, rate-limit
  hits per 10 min (`rate_limit::check_user_audited`), searches merge prefix-refinements
  within 2 min and only page 1 of `GET /search` is recorded (not `/search/chunks`, MCP).
- `/activity` and `/me/sessions` take `SessionUser`: tokens get 403. `PATCH /me` fields are
  now optional independently (`display_name` absent = keep, `null` = clear; helper
  `extract::double_option`); `{}` is a 400. `users.record_search_history` (default on).
- `files.last_opened_at/open_count` do not bump `updated_at` (trigger has a `WHEN`).
  `sort=opened` (cursor tag `r`) lists only opened files. `store::save/delete*` and
  `files::edit::update` take an `activity::Actor` (`via` session/token); MCP callers are
  `Caller::actor()`.
- 6b UI: `/collections`, `/collections/$id` (reuses the library's `FileCollection`,
  `SelectionBar` with an `actions` slot), `/activity` (`?category=`), Settings → Security
  (`?tab=security`). Collection colours are `--swatch-*` tokens (DESIGN.md); the sidebar
  loads the collection dialog lazily (bundle budget: 174 of 180 kB). File pages call
  `POST /files/{id}/open` once per visit (`useMarkOpened`). Search and chat take
  `?collection=<id>`. The e2e suite now makes 2 more credential calls (one register, one
  login for the second-device session test); `register()` in `e2e/support.ts` now waits
  and resubmits when the per-IP credential limit answers 429.
- `ActivityKind` is written out by hand (not a macro): utoipa ignores `serde(rename)` on
  macro-generated variants, which put Rust names into the OpenAPI enum.
- Transcription (ADR 0017, `crates/media`, migration 0015): tests use
  `AKASHA_WHISPER_MODEL=fake` (`support::test_config()`, e2e server too): every second of
  a sine wave becomes "tone N hertz", so generated WAVs give deterministic, timed text.
  `TestApp::run_jobs()` builds its worker from `test_config()`, not the app's config:
  tests that change transcription settings must call `run_jobs_with(&config)`. A media
  upload now runs extract (+ embed + enrich when there is speech); undecodable media ends
  `failed` (permanent), a missing model is retryable (file stays `processing`).
  Real model: `cargo test -p akasha-media --release --test whisper -- --ignored` (needs
  Hugging Face). whisper.cpp builds with cmake (~2 min, ~250 MB in `target/`); the build
  script turns any `GGML_*`/`WHISPER_*`/`CMAKE_*` environment variable into a cmake define,
  so do not export such variables for unrelated reasons. `opus-decoder` has debug overflow
  checks off (root `Cargo.toml`). `jobs.progress` + `akasha_jobs::report_progress` are
  generic (any long job may report); `Attempt` now carries `job_id`. Citations stored in
  messages gained optional `start_ms/end_ms` (`serde(default)` for old rows).
  whisper-rs 0.16's `set_abort_callback_safe` mis-casts its closure (stores
  `Box<dyn FnMut>`, calls it as `F`): only ever pass it a `Box<dyn FnMut() -> bool>`,
  otherwise whisper aborts with "failed to encode" (-6). Only the Docker smoke test
  (`models check` with `tiny`) runs real Whisper in CI.
- Watched folders (ADR 0018, `crates/app/src/sources/`, migration 0016): scans skip files
  modified in the last 2 s, so tests set mtimes in the past (`File::set_modified`) and
  must run jobs with a config that has `watch_roots` (`run_jobs_with(&t.config)`): the
  job re-checks the folder against the roots and fails the source otherwise. The upload
  receiver moved to `files/receive.rs` (stream-based, shared with imports);
  `store::save_in`/`replace_in` work inside the caller's transaction. Deletions only
  happen after a complete pass; an emptied folder is an error, not a mass delete. The e2e
  server watches `$TMPDIR/akasha-e2e-watch` (vault written by `playwright.config.ts`) and
  polls jobs every second. `notify` is CC0 (deny.toml exception).
- CPU portability (Dockerfile, `crates/app/src/cpu.rs`): whisper-rs-sys 0.15 links ggml
  statically, so ggml's own runtime dispatch (`GGML_BACKEND_DL` + `GGML_CPU_ALL_VARIANTS`,
  shared libraries only) is unavailable. The amd64 image builds the binary twice instead:
  `akasha` (ggml SSE4.2 only) and `akasha-avx2`; `cpu::dispatch()` runs first in `main`
  and `exec`s the AVX2 build when AVX/AVX2/FMA/F16C/BMI2 are present
  (`AKASHA_CPU_VARIANT=baseline` forces the portable one, `AKASHA_CPU_DISPATCHED` stops
  loops). The build script ignores `GGML_*` changes, so the Dockerfile runs
  `cargo clean --release -p whisper-rs-sys` before each variant. `AKASHA_BUILD_CPU_VARIANT`
  (compile time) names the build (`avx2`/`baseline`/`armv8`, local builds `native`); it is
  logged at start-up and printed by `models check`. arm64 builds once for plain ARMv8-A.
  CI's docker job is a matrix on native runners (`ubuntu-24.04-arm` is free for public
  repos) and runs the portable amd64 build under `qemu-x86_64-static -cpu Nehalem`.
  `release.yml` pushes the multi-arch image to ghcr.io on `v*` tags (per-arch digests,
  then `imagetools create`); it has not run yet.
- `just check` here ran out of disk while linking the ~25 test binaries (linker "Bus
  error" = disk full): `cargo clean -p akasha` and `CARGO_INCREMENTAL=0` keep it under ~12 GB.

## Session log

Newest first. One line per session: date · who · what changed · anything left half-done.

- 2026-10-10 · Claude (cloud) · Watched folders: migration 0016 (`sources`, `source_files`), `/api/v1/sources` CRUD + rescan, `scan_source`/`scan_all_sources` jobs (batched, resumable, rename detection, quota/unreadable retries, empty-folder guard), debounced `notify` watcher, O_NOFOLLOW + inode-checked opens, globs, Obsidian front-matter tags, `source.*` activity events; Settings → Sources UI; tests: db (5), unit (paths, filter, walk, front matter, debounce), HTTP (import, change, rename, delete, keep, symlink escape, roots, owner isolation, token scope, pause, remove), Vitest, e2e step with axe.
- 2026-10-10 · Claude (cloud) · CPU portability: portable (SSE4.2) + AVX2 builds of the binary in the amd64 image with start-up dispatch (`cpu.rs`), arm64 image, CI docker matrix on native arm runners + no-AVX QEMU smoke test, `release.yml` multi-arch push to ghcr.io, README "Supported platforms". The Docker build cannot run here (no daemon); CI verifies it.
- 2026-10-10 · Claude (cloud) · Step 6c transcription: new `crates/media` (Symphonia + `opus-decoder` decoding to 16 kHz mono with a streaming windowed-sinc resampler, windowed transcription with quiet-point cuts, cancellation, progress, Whisper model catalog with pinned SHA-256 and streamed download, whisper.cpp via `whisper-rs` 0.16, deterministic tone model), migration 0015 (`file_extractions.segments/duration_ms`, `file_chunks.start_ms/end_ms`, `jobs.progress`), `extract_file` transcribes audio/video (`AKASHA_TRANSCRIBE_*`, `AKASHA_WHISPER_MODEL`), times in search results, chat citations/prompt and MCP, transcription in system status and `models download/check`; UI: lazily loaded player + timestamped transcript (seek, active line, `?t=`), times on search results/citations/palette, "Transcribing N%" in the timeline. Docker installs cmake and builds portable whisper.cpp; CI smoke test runs Whisper `tiny`. Tests: media unit/integration (formats incl. MP4/WebM with video, resampling, windowing, cap, cancel), ingest transcript chunking, job progress, HTTP (upload WAV → timed chunks → search/chat), Vitest, e2e recording flow with axe. Also made the chat e2e move the mouse off a citation before asking again (hover popover flake). CI's Docker smoke test then caught a whisper-rs abort-callback bug (real Whisper aborted every run); fixed with a correctly typed callback.
- 2026-10-10 · Claude (cloud) · Trusted reverse proxies: `AKASHA_TRUSTED_PROXIES`, one client-IP resolver (rightmost untrusted XFF hop / `Forwarded`, `X-Forwarded-Proto` → `Secure` cookies) used by the auth rate limiter, security log and sessions; tests for spoofing, chains and per-client limits; README Caddy/nginx notes.
- 2026-10-10 · Claude (cloud) · Step 6b screens: Collections in the sidebar (+ phone account menu, palette, `g o`/`g a`), collections list and collection page (header with swatch mark, add-files picker, remove from collection, edit/delete, search in it, ask), "Add to collection" in library multi-select and on file pages (membership checkboxes), collection search chip and chat collection scope bar, `/activity` timeline (day groups, filters, links, clear history), Settings → Security (sessions with sign-out, sign out others, recent sign-ins, search-history toggle), "Recently opened" sort, open tracking; fixed a mobile overflow in the library/search filter rows. Vitest 99, e2e 10 (collections → search within → activity → revoke a session; axe over the new screens). Screenshots `p6-*` in the scratchpad.
- 2026-10-10 · Claude (cloud) · Step 6b API: migrations 0013 (collections, collection_files, conversation collection scope, open tracking) and 0014 (activity_events, search-history preference, session IP); collections CRUD + add/remove files, `collection_id` on files/search/chat, MCP `collection` filters + `list_collections`; activity recording for files/search/chat/collections/tokens/sign-ins/password/sessions/rate limits, `GET/DELETE /activity`, `/me/sessions` list/revoke/revoke-others, `POST /files/{id}/open`, `sort=opened`, daily `prune_activity` (`AKASHA_ACTIVITY_RETENTION_DAYS`), ADR 0016. Tests: db (collections/activity/sessions) and HTTP (collections, activity, sessions) incl. owner isolation, scopes, retention. Screens follow in the next commit.

- 2026-10-10 · Claude (cloud) · Step 6a: personal API tokens (migration 0012, `/me/tokens` GET/POST/DELETE, Bearer auth in `AuthUser` with read/write scopes, cookie-only `SessionUser` for account and token management, Settings → Access tokens with copy-once dialog and Claude Code command) and the MCP server (`rmcp` 3.5.1, stateless Streamable HTTP at `/mcp`: search, get_file, read_file, list_files, ask, add_note, tag_file, `akasha://file/{id}` resources; `akasha mcp` stdio bridge), ADR 0015, README "Use Akasha from Claude / AI agents". Tests: Rust token + MCP suites (handshake, tools, scopes, owner isolation, 401), Vitest tokens panel, e2e creates a token in the UI and calls `/mcp`. Resumed twice after interruptions (container restart, usage limit).

- 2026-10-09 · Claude (cloud) · Step 5d, Step 5 complete. Search relevance floor (ADR 0014): semantic-only results must clear a per-model threshold (rerank score or cosine), keyword/file-name hits always count, the rest are "loosely related" (`include_weak`, collapsed UI section); eval gains Precision@10 and `negative_clean` (baselines re-recorded as an intended change, MiniLM floor calibrated). Conversations store their file scope (migration 0011). `GET /system/status` + Settings → System. UI: first-run onboarding, axe in e2e over every screen light/dark (fixed `fg-subtle` contrast, heading order, dl markup, target sizes, landmarks), answer live-region announcements, `?` shortcuts sheet, page titles, per-screen error boundaries, edit and ask again, toasts, optimistic tags, PWA manifest + icons, compressed assets, bundle budget script. Deferred: offline shell (service worker).

- 2026-10-09 · Claude (cloud) · Step 5c: search + chat screens. Search: debounced as-you-type with URL state (q, mode in "Advanced", type, date range, tags, pinned, page), cancelled stale requests, rate-limit notice with auto retry, file-grouped results with summaries, page numbers and safe `<mark>` highlights (code-point offsets), "did you mean", degraded notice, empty/no-result guidance, timing disclosure; passage links open the file's text marked and scrolled (PDF at the page); ⌘K live results + "Search for", new chat action. Chat: conversation list (rail / phone sheet) with rename/delete, new chat with suggestions, composer (Enter/Shift+Enter, stop), SSE streaming via fetch, safe Markdown answers with citation chips (hover/tap quote, click opens passage), sources panel, refusal/no_llm/error/cancelled states, copy, ask again, jump to latest, library "Ask about these" scope. Textless files get enrichment `skipped` at once (fix from review). Vitest 89 (SSE parser, stream, citations, highlights, URL state, search page, answers), e2e 8 (search + chat flows, refusal).

- 2026-10-09 · Claude (cloud) · Step 5.2: library + file detail. API: `sort` + per-sort keyset cursors on `GET /files`, `GET /tags`, `meta.max_upload_bytes`, inline download for viewable types, unique operationIds. UI: global drop zone + Upload button (`u`), XHR upload queue with per-file progress, cancel, retry, mapped API errors and duplicate links; library grid/list (persisted), thumbnails with type-icon fallback, sort, category/tag/pinned filters, infinite scroll, multi-select bulk delete, pin, arrow-key grid navigation, `/` search focus, Delete with confirmation, teaching empty state; file page with image/PDF/text/Markdown/audio/video preview, summary, tags editor (keep/dismiss suggestions), rename, download, delete, reindex, regenerate summary, similar files, extracted-text viewer with page markers, processing timeline; lazy routes + vendor chunks. Vitest 41, e2e 6 (upload text + PNG → ready → rename/tag/pin → delete). Resumed after a usage-limit cut (the first session wrote most of it; the second finished tests, e2e, screenshots).

- 2026-10-09 · Claude (cloud) · Step 5.1: UI foundation. `web/DESIGN.md` (paper/ink palette with one verdigris accent, Newsreader + Inter + JetBrains Mono self-hosted, verified AA contrast, 4px spacing, radii, elevation, motion with reduced-motion), Tailwind 4 tokens, owned Radix primitives (button, input, field, dialog, dropdown, toast, tooltip, tabs, segmented, skeleton, card, badge, kbd), TanStack Router (typed, code-based, guarded layouts) + Query, `openapi-fetch` client with `ApiError` mapping and session-expiry redirect, app shell (sidebar / phone tab bar, ⌘K palette, theme toggle, user menu, global drop zone placeholder), sign-in, register (honours `allow_registration` via new public `GET /api/v1/meta`), settings (profile, theme, password, delete account), 404, error boundary, `/design` reference page, placeholders for library/search/chat; UI embedded in the binary (`embed-ui` feature, SPA fallback, immutable hashed assets, ETag, CSP and security headers on every response), Dockerfile web stage, Vitest (17) + Playwright e2e (4) with `just e2e` and a CI `e2e` job. Library/search/chat screens next.

- 2026-10-09 · Claude (cloud) · Step 4 done (4.4): default Claude model → `claude-sonnet-5-5`; migration 0010 (`files.summary/auto_tags/enrichment_*`, `conversations.title_source`); `enrich_file` job (first 8k chars + 3 later samples, JSON mode for Ollama/Gemini, defensive parsing/normalisation, idempotent per extraction, failures never touch file status), `auto_tags` in file/search responses and tag filters, `PATCH auto_tags`, `POST /files/{id}/enrich` (10/min/user); `title_conversation` job after the first answer; refusal-gate calibration report in `akasha eval` (`eval/gate.json`); compose `--profile ollama` + host-Ollama docs; ADR 0013. Real-reranker threshold still uncalibrated (HF blocked here).

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
