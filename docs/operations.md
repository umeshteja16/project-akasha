# Operations

Running Akasha for real: backups, upgrades and the configuration reference. For install
and feature setup see the [README](../README.md).

- [Backup and restore](#backup-and-restore)
- [Upgrading](#upgrading)
- [Configuration reference](#configuration-reference)

## Backup and restore

### What to back up

| What | Where it lives | Back it up? |
|---|---|---|
| **Postgres database** | Compose volume `pgdata`, or your own server | **Yes.** Users, file metadata, extracted text, chunks, **embeddings (pgvector)**, search vocabulary, conversations, tokens, job queue. |
| **Blob store** (the uploaded files) | `AKASHA_STORAGE_DIR` (Compose volume `storage`, `/var/lib/akasha/storage` in the image) or the S3 bucket | **Yes.** Without it the database points at files that no longer exist. |
| **Configuration** | `.env`, `akasha.toml`, Compose overrides, reverse-proxy config | **Yes**, kept somewhere safe: it holds `DATABASE_URL`, S3 keys and API keys (the scripts do not copy it). |
| **Models** (`AKASHA_MODELS_DIR`, Compose volume `models`) | Downloaded ONNX/OCR/Whisper files | Optional. Re-downloadable (`akasha models download`); worth keeping only for air-gapped hosts. |
| `ollama`, `prometheus`, `grafana` volumes | Optional services | Not Akasha data. |

The vectors are inside the database dump, so a restore needs no re-embedding. The
embedding model recorded in the database must match `AKASHA_EMBED_MODEL` after the
restore (the server refuses to start otherwise).

### Consistency: dump the database first, then copy the blobs

Blobs are content-addressed (`blobs/ab/cd/<sha256>`) and never modified. So:

- a blob uploaded **after** the dump but before the copy is an *extra* blob nothing
  references: harmless (it is simply unused);
- the opposite order could leave database rows whose blob is *missing*: not harmless.

`scripts/backup.sh` therefore always dumps first. No downtime is needed. Half-written
uploads (`staging/`) are skipped. Deleted files only disappear from the blob store through
the `DeleteBlobIfUnreferenced` job, which re-checks the database, so a restored database
and a slightly newer blob store are consistent. Restoring an older blob store with a newer
database is **not**: always restore the pair from one backup.

### Scripts

```sh
# Back up (needs pg_dump of at least your server's major version)
export DATABASE_URL=postgres://akasha:PASSWORD@localhost:5432/akasha
export AKASHA_STORAGE_DIR=/var/lib/akasha/storage
scripts/backup.sh -o /srv/backups            # -> /srv/backups/akasha-20261010T120000Z/

# Restore into an empty database and storage dir (stop Akasha first)
createdb -h localhost -U akasha akasha_new   # or any empty database
scripts/restore.sh -d postgres://akasha:PASSWORD@localhost:5432/akasha_new \
                   -s /var/lib/akasha/storage /srv/backups/akasha-20261010T120000Z
```

A backup directory holds `db.dump` (`pg_dump -Fc`, no owners/privileges, so it restores
under any role), `storage.tar`, `SHA256SUMS` and `backup.info`. `restore.sh` verifies the
checksums, refuses a non-empty database or storage directory unless you pass `-f`
(which drops and replaces what is there), and restores with `pg_restore --exit-on-error`.
The restoring role must be allowed to `CREATE EXTENSION vector` (the dump recreates it;
the `pgvector/pgvector` image and a database owner/superuser can). With the S3 backend
(`AKASHA_STORAGE_BACKEND=s3`) the scripts handle the database only: use bucket versioning,
replication or `aws s3 sync`/`rclone` for the bucket, taken **after** the dump.

`just backup-test` runs the full round trip against a scratch database (migrate, insert a
user, file, chunk with an embedding and a blob, back up, restore into a second database,
verify rows, the vector query and the blob).

Run it from cron or a systemd timer and copy the result off the machine; keep several
generations and **test a restore** occasionally.

### Docker Compose

Postgres is published on `127.0.0.1:5432`, so with `postgresql-client` (17) on the host the
scripts work for the database; the blob volume is easiest to handle with a helper container.
Or do everything through Docker:

```sh
d=backups/akasha-$(date -u +%Y%m%dT%H%M%SZ); mkdir -p "$d"

# 1. database first
docker compose exec -T postgres pg_dump -U akasha -Fc --no-owner --no-privileges akasha > "$d/db.dump"
# 2. then the blobs
vol=$(docker volume ls -q --filter label=com.docker.compose.volume=storage)
docker run --rm -v "$vol":/data:ro -v "$PWD/$d":/backup alpine \
  tar -C /data --exclude=./staging -cf /backup/storage.tar .
```

Restore into a fresh install (`docker compose down` the app first; keep `postgres` running):

```sh
docker compose up -d postgres
docker compose exec -T postgres pg_restore -U akasha -d akasha --no-owner --no-privileges \
  --exit-on-error < "$d/db.dump"            # add --clean --if-exists to replace existing data
vol=$(docker volume ls -q --filter label=com.docker.compose.volume=storage)
docker run --rm -v "$vol":/data -v "$PWD/$d":/backup alpine \
  sh -c 'tar -C /data -xf /backup/storage.tar && chown -R 65532:65532 /data'   # image runs as uid 65532
docker compose --profile app up -d
```

(If the volumes do not exist yet, run `docker compose --profile app create` first.)

### Bare binary

Run `scripts/backup.sh` / `scripts/restore.sh` as above with the same `DATABASE_URL` and
`AKASHA_STORAGE_DIR` the server uses (`akasha` reads them from the environment, `.env` is
only loaded by `just`). Equivalent manual steps:

```sh
pg_dump --dbname "$DATABASE_URL" -Fc --no-owner --no-privileges -f db.dump
tar -C "$AKASHA_STORAGE_DIR" --exclude=./staging -cf storage.tar .
# restore
pg_restore --dbname "$NEW_DATABASE_URL" --no-owner --no-privileges --exit-on-error db.dump
tar -C "$AKASHA_STORAGE_DIR" -xf storage.tar
```

Filesystem-level snapshots (ZFS, LVM, btrfs) of the storage directory are fine too; take the
database dump before the snapshot. Copying a *running* Postgres data directory is not a
backup; use `pg_dump` or `pg_basebackup`.

## Upgrading

1. **Back up** (above). Migrations change the schema and cannot be reliably undone.
2. Pull the new image (`docker compose --profile app pull`/`up -d --build`) or install the
   new binary, and restart. Read the release notes for config changes.
3. **Migrations run on start**: `akasha serve`, `akasha worker`, `akasha reembed` and
   `akasha migrate` all apply pending migrations (embedded in the binary, tracked in
   `_sqlx_migrations`) before doing anything else. Several processes starting at once is
   safe (sqlx takes a lock). `akasha migrate` applies them and exits, if you prefer a
   separate step. Startup fails loudly if a migration fails or an applied one was edited.
4. **Changing the embedding model** (`AKASHA_EMBED_MODEL`) invalidates every stored vector,
   so the server refuses to start with a model other than the one recorded in the database.
   To switch: stop all `serve`/`worker` processes, set the new `AKASHA_EMBED_MODEL`, run
   `akasha reembed` (drops all vectors, resizes the vector column, rebuilds the index and
   queues every file; `--force` does that even when the model is unchanged), then start again.
   Keyword search works while the queue drains. Models with more than 2000 dimensions are
   rejected. A backup taken before the switch restores the *old* model: set the old
   `AKASHA_EMBED_MODEL` again, or re-run `reembed`. Changing `AKASHA_RERANK_MODEL` needs
   nothing but a restart.

**Downgrades are not supported.** Every migration has a `down` file (`just migration`
creates them in pairs, and `sqlx migrate revert` can apply them) but they exist for
development, are not exercised against production data, and some drop data. An older binary
also refuses to start on a database that has migrations it does not know. To go back:
restore the backup taken before the upgrade and run the old version.

## Configuration reference

Settings come from, later winning: built-in defaults, an optional `akasha.toml` in the
working directory (keys without the prefix, lower case: `bind_addr = "0.0.0.0:8080"`),
environment variables `AKASHA_<KEY>` (upper case), and `DATABASE_URL`. Lists are
comma-separated (`AKASHA_TRUSTED_PROXIES` also accepts spaces). `.env.example` is a
ready-to-copy starting point (`just` loads `.env`; the binary itself does not). The Docker
image sets a few paths (marked **image**) and `compose.yaml` sets `AKASHA_SERVE_WITH_WORKER=true`.
Secrets are redacted in logs.

### Server and database

| Variable | Default | Meaning |
|---|---|---|
| `DATABASE_URL` | `postgres://akasha:akasha@localhost:5432/akasha` | Postgres connection string (needs the pgvector extension). |
| `AKASHA_BIND_ADDR` | `0.0.0.0:8080` | Address the HTTP server listens on. |
| `AKASHA_DB_MAX_CONNECTIONS` | `10` | Postgres pool size. |
| `AKASHA_LOG_FORMAT` | `pretty` (**image**: `json`) | `pretty` or `json`. |
| `RUST_LOG` | `info,tower_http=info,sqlx=warn` | Log filter (`tracing` syntax). |
| `AKASHA_COOKIE_SECURE` | `false` | `Secure` session cookie; set `true` behind HTTPS. |
| `AKASHA_TRUSTED_PROXIES` | empty | Reverse proxies (addresses/CIDRs) whose `X-Forwarded-*` headers are believed; empty trusts nobody. |
| `AKASHA_ALLOW_REGISTRATION` | `true` | Let anyone who can reach the server create an account. |
| `AKASHA_SESSION_TTL_DAYS` | `30` | Login session lifetime. |
| `AKASHA_ACTIVITY_RETENTION_DAYS` | `365` | Activity and security log entries older than this are deleted daily; `0` keeps them. |
| `AKASHA_MAX_UPLOAD_MB` | `512` | Largest accepted upload (MiB). |
| `AKASHA_SEARCH_RATE_PER_MINUTE` | `30` | Searches per user per minute; `0` unlimited. |
| `AKASHA_CHAT_RATE_PER_MINUTE` | `20` | Chat questions per user per minute; `0` unlimited. |

### Storage

| Variable | Default | Meaning |
|---|---|---|
| `AKASHA_STORAGE_BACKEND` | `local` | `local` or `s3`. |
| `AKASHA_STORAGE_DIR` | `./storage` (**image**: `/var/lib/akasha/storage`) | Blob root for `local`. |
| `AKASHA_STORAGE_S3_BUCKET` | unset | Bucket for `s3`. |
| `AKASHA_STORAGE_S3_REGION` | unset | Region (S3-compatible services accept any value). |
| `AKASHA_STORAGE_S3_ENDPOINT` | unset | Custom endpoint (MinIO, R2, Garage, ...). |
| `AKASHA_STORAGE_S3_ACCESS_KEY_ID` | unset | Access key; falls back to `AWS_ACCESS_KEY_ID`. |
| `AKASHA_STORAGE_S3_SECRET_ACCESS_KEY` | unset | Secret key (secret); falls back to `AWS_SECRET_ACCESS_KEY`. |
| `AKASHA_STORAGE_S3_ALLOW_HTTP` | `false` | Allow a plain-HTTP endpoint (local MinIO only). |

### Background worker

| Variable | Default | Meaning |
|---|---|---|
| `AKASHA_SERVE_WITH_WORKER` | `false` (Compose and image command: on) | `serve` also runs the worker (same as `serve --with-worker`). |
| `AKASHA_WORKER_CONCURRENCY` | `4` | Jobs run at once per worker. |
| `AKASHA_WORKER_POLL_SECS` | `5` | Fallback poll interval; new jobs wake workers immediately. |
| `AKASHA_WORKER_VISIBILITY_TIMEOUT_SECS` | `300` | A running job without a heartbeat this long is presumed lost and retried. |
| `AKASHA_WORKER_SHUTDOWN_GRACE_SECS` | `30` | Time in-flight jobs get to finish on shutdown. |

### Watched folders

| Variable | Default | Meaning |
|---|---|---|
| `AKASHA_WATCH_ROOTS` | empty (feature off) | Comma-separated directories users may watch; `{user_id}` / `{email}` give each user a subtree. |
| `AKASHA_WATCH_SCAN_MINUTES` | `15` | Full rescan interval; `0` only on file events. |
| `AKASHA_WATCH_FS_EVENTS` | `true` | Use inotify/FSEvents to pick up changes within seconds. |

### Extraction, OCR, transcription

| Variable | Default | Meaning |
|---|---|---|
| `AKASHA_OCR_ENABLED` | `true` | OCR for images and scanned PDF pages. |
| `AKASHA_OCR_MODELS_URL` | `https://ocrs-models.s3-accelerate.amazonaws.com` | OCR model download base; empty never downloads. |
| `AKASHA_MODELS_DIR` | `./models` (**image**: `/var/lib/akasha/models`) | Where OCR, embedding, rerank and Whisper models live. |
| `AKASHA_TRANSCRIBE_ENABLED` | `true` | Transcribe audio/video; off keeps recordings playable without text. |
| `AKASHA_WHISPER_MODEL` | `base` | `tiny`, `base`, `small`, `medium` or `large-v3-turbo`. |
| `AKASHA_TRANSCRIBE_THREADS` | `0` | CPU threads per transcription; `0` = cores, at most 8. |
| `AKASHA_TRANSCRIBE_MAX_MINUTES` | `120` | Only the start of longer recordings is transcribed. |
| `AKASHA_TRANSCRIBE_LANGUAGE` | empty | ISO 639-1 code (`en`, `de`, ...); empty detects it. |
| `AKASHA_CPU_VARIANT` | unset | `baseline` forces the portable (non-AVX2) build on amd64 images. |

### Embeddings, reranking, search

| Variable | Default | Meaning |
|---|---|---|
| `AKASHA_EMBED_MODEL` | `multilingual-e5-small` | Also `bge-small-en-v1.5`, `bge-base-en-v1.5`, `nomic-embed-text-v1.5`, `bge-m3`. Changing it needs `akasha reembed`. |
| `AKASHA_RERANK_MODEL` | `jina-reranker-v1-turbo-en` | Also `bge-reranker-base`, `bge-reranker-v2-m3`; `none` or empty disables. |
| `AKASHA_ML_MODELS_URL` | `https://huggingface.co` | Hugging Face-compatible download base; empty never downloads (air-gapped). |
| `AKASHA_ORT_DYLIB_PATH` | empty (**image**: `/usr/local/lib/libonnxruntime.so`) | ONNX Runtime library; empty uses `ORT_DYLIB_PATH`, then `libonnxruntime.so` next to the binary or on the library path. |
| `AKASHA_ML_THREADS` | `0` | Inference threads per model; `0` = all cores. |
| `AKASHA_SEARCH_MIN_SIMILARITY` | unset | Cosine floor for vector-only results; unset = per-model default. |
| `AKASHA_SEARCH_MIN_RERANK_SCORE` | unset | Same, by reranker score; unset = per-reranker default. |

### Chat and language models

| Variable | Default | Meaning |
|---|---|---|
| `AKASHA_LLM_PROVIDER` | `ollama` | `ollama`, `anthropic`, `gemini`, `openai` (any OpenAI-compatible server), `none`; `fake` is for tests. |
| `AKASHA_LLM_MODEL` | empty | Model id; empty = provider default (OpenAI-compatible servers need one). |
| `AKASHA_STRICT_OFFLINE` | `false` | Refuse to start with a cloud provider or non-local server. |
| `AKASHA_OLLAMA_URL` | `http://localhost:11434` | Ollama server. |
| `AKASHA_OLLAMA_NUM_CTX` | `8192` | Context window requested from Ollama (tokens). |
| `AKASHA_ANTHROPIC_API_KEY` | unset | Secret; falls back to `ANTHROPIC_API_KEY`. |
| `AKASHA_ANTHROPIC_BASE_URL` | `https://api.anthropic.com` | API base. |
| `AKASHA_ANTHROPIC_EFFORT` | `low` | Claude `output_config.effort`; empty = model default. |
| `AKASHA_GEMINI_API_KEY` | unset | Secret; falls back to `GEMINI_API_KEY`. |
| `AKASHA_GEMINI_BASE_URL` | `https://generativelanguage.googleapis.com` | API base. |
| `AKASHA_OPENAI_API_KEY` | unset | Secret, optional for local servers; falls back to `OPENAI_API_KEY`. |
| `AKASHA_OPENAI_BASE_URL` | `https://api.openai.com/v1` | Including the version path. |
| `AKASHA_LLM_CONNECT_TIMEOUT_SECS` | `10` | Connection timeout to the model server. |
| `AKASHA_LLM_READ_TIMEOUT_SECS` | `120` | Give up after this long without new output. |
| `AKASHA_LLM_MAX_RETRIES` | `2` | Retries on connection errors, 429 and 5xx (before output starts). |
| `AKASHA_LLM_MAX_TOKENS` | `1024` | Longest answer (tokens). |
| `AKASHA_LLM_TEMPERATURE` | `0.1` | Sampling temperature where the provider accepts one. |
| `AKASHA_LLM_ENRICH_FILES` | `true` | Summary and suggested tags per file after indexing (one model call per file). |
| `AKASHA_LLM_CONVERSATION_TITLES` | `true` | Model-written conversation titles after the first answer. |
| `AKASHA_CHAT_CONTEXT_CHUNKS` | `8` | Passages given to the model per answer. |
| `AKASHA_CHAT_HISTORY_MESSAGES` | `6` | Earlier messages sent along with a question. |
| `AKASHA_CHAT_CONDENSE_QUESTION` | `true` | Rewrite follow-ups into standalone queries (one extra model call). |
| `AKASHA_CHAT_MIN_RERANK_SCORE` | unset | Refusal threshold; unset = calibrated default per reranker. |

### Monitoring

| Variable | Default | Meaning |
|---|---|---|
| `AKASHA_METRICS_ENABLED` | `false` | Prometheus metrics. Needs `AKASHA_METRICS_BIND_ADDR` or `AKASHA_METRICS_TOKEN`. |
| `AKASHA_METRICS_BIND_ADDR` | empty | Separate unauthenticated listener, e.g. `0.0.0.0:9090` (keep private). Workers expose metrics only here. |
| `AKASHA_METRICS_TOKEN` | unset | Secret; bearer token for `/metrics` on the main port. |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | unset | Export traces over OTLP/HTTP (also `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT`, `OTEL_SERVICE_NAME`, default `akasha`). |

### Other variables

| Variable | Used by | Meaning |
|---|---|---|
| `AKASHA_URL` (default `http://127.0.0.1:8080`), `AKASHA_TOKEN` | `akasha mcp` | Server URL and personal API token for the stdio MCP bridge (also `--url`, `--token`). |
| `POSTGRES_PASSWORD` (default `akasha`) | `compose.yaml` | Password of the Compose Postgres and in the app's `DATABASE_URL`. |
| `GRAFANA_PASSWORD` (default `admin`) | `compose.yaml` | Grafana admin password (`monitoring` profile). |
| `AKASHA_BACKUP_DIR` (default `./backups`), `AKASHA_STORAGE_DIR`, `DATABASE_URL` | `scripts/backup.sh`, `restore.sh` | See [Backup and restore](#backup-and-restore). |
