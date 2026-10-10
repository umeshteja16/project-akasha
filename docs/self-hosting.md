# Self-hosting Akasha

From nothing to a running, private Akasha you can upload files to, search and chat with.
Day-two topics (backup, upgrades, every setting) are in [operations.md](operations.md).

- [Requirements](#requirements)
- [Quick start: Docker Compose](#quick-start-docker-compose)
- [Without Docker: build from source](#without-docker-build-from-source)
- [First run](#first-run)
- [Choosing an LLM for chat](#choosing-an-llm-for-chat)
- [Reverse proxy and HTTPS](#reverse-proxy-and-https)
- [Optional features](#optional-features)
- [Backups and upgrades](#backups-and-upgrades)
- [Troubleshooting](#troubleshooting)

## Requirements

| | |
|---|---|
| **CPU** | `linux/amd64` (any x86-64 with SSE4.2; the image switches to an AVX2 build by itself when the CPU has it) or `linux/arm64` (Raspberry Pi 4/5 on a 64-bit OS, Ampere, Graviton, Apple Silicon hosts). 32-bit ARM is not supported. |
| **Disk** | About 0.6 GB for the default search models (embedding model ~470 MB), 12 MB for OCR and 142 MB for the default `base` speech model, plus Postgres and your files. The image itself is a few hundred MB. |
| **RAM** | The embedding and rerank models stay loaded; plan for at least 4 GB. Not benchmarked: bigger models (`bge-m3`, Whisper `medium`/`large-v3-turbo`) need considerably more and a fast CPU. |
| **Docker route** | Docker Engine with the Compose plugin (v2). Postgres comes with the compose file. |
| **No-Docker route** | Postgres **17** (what CI and the compose file use; 16 is not tested) with the **pgvector** extension (0.5 or newer, for HNSW) and the contrib extensions `citext`, `pg_trgm` and `btree_gin`. The role in `DATABASE_URL` must be allowed to `CREATE EXTENSION` (migrations do it). |
| **Network** | Outbound HTTPS to `huggingface.co` and `ocrs-models.s3-accelerate.amazonaws.com` on first start for the models, unless you [install them offline](#offline-and-air-gapped-installs). A cloud chat provider needs its API endpoint. |

Search, OCR and transcription run inside Akasha. Only chat answers need a language model
([choose one](#choosing-an-llm-for-chat)); without one, chat still returns matching passages.

## Quick start: Docker Compose

The image is `ghcr.io/umeshteja16/project-akasha` (multi-arch, published by the release
workflow for `v*` tags; `latest` follows the newest release, `0.1.0`-style tags pin one).

1. **Get the compose file** into an empty directory:

   ```sh
   mkdir akasha && cd akasha
   curl -fsSLO https://raw.githubusercontent.com/umeshteja16/project-akasha/master/compose.yaml
   ```

   (or `git clone https://github.com/umeshteja16/project-akasha && cd project-akasha`; the
   [monitoring profile](#optional-features) needs the repo's `deploy/` folder next to it).

2. **Create `.env`** next to it. Compose reads it automatically:

   ```sh
   cat > .env <<EOF
   COMPOSE_PROFILES=app
   POSTGRES_PASSWORD=$(openssl rand -hex 24)
   EOF
   chmod 600 .env
   ```

   `COMPOSE_PROFILES=app` is what starts Akasha itself (the compose file keeps it behind a
   profile so `just db-up` can start Postgres alone in development). Set
   `POSTGRES_PASSWORD` **before the first start**: Postgres only reads it when it creates
   its data volume, so changing it later does not change the stored password. Every other
   setting is optional; the ones the compose file passes through are listed in
   [Settings in `.env`](#settings-in-env).

3. **Start it:**

   ```sh
   docker compose pull        # the published image; skip to build from a checkout (below)
   docker compose up -d
   docker compose logs -f akasha
   ```

   Wait for `listening` in the log, then open **http://localhost:8080** and
   [create the first account](#first-run). The port is published on `127.0.0.1` only.

   To build from source instead (inside a repository checkout), run
   `docker compose up -d --build`. A clean directory with only `compose.yaml` cannot build.

4. **Verify the install** (optional; loads the embedding and rerank models and Whisper and
   runs each once):

   ```sh
   docker compose run --rm akasha models check
   ```

Everything persists in named volumes: `pgdata` (database), `storage` (your files), `models`
(downloaded models). `docker compose down` keeps them; `down -v` **deletes your data**.

### Settings in `.env`

Compose passes these through (defaults in brackets); all the rest of the
[configuration reference](operations.md#configuration-reference) can be added to the
`environment:` block of `compose.yaml` the same way.

| Variable | Purpose |
|---|---|
| `AKASHA_VERSION` (`latest`) | Image tag, e.g. `0.1.0` to stay on a release. |
| `AKASHA_HTTP_BIND` (`127.0.0.1`) | Host address the port is published on. `0.0.0.0` exposes plain HTTP on your network; see [HTTPS](#reverse-proxy-and-https). |
| `AKASHA_ALLOW_REGISTRATION` (`true`) | Let anyone who can reach the server create an account. |
| `AKASHA_MAX_UPLOAD_MB` (`512`) | Largest accepted upload. |
| `AKASHA_LLM_PROVIDER`, `AKASHA_LLM_MODEL`, `AKASHA_OLLAMA_URL`, `AKASHA_ANTHROPIC_API_KEY`, `AKASHA_GEMINI_API_KEY`, `AKASHA_OPENAI_BASE_URL`, `AKASHA_OPENAI_API_KEY`, `AKASHA_STRICT_OFFLINE` | [Chat model](#choosing-an-llm-for-chat). |
| `AKASHA_EMBED_MODEL`, `AKASHA_RERANK_MODEL` | Search models. Changing the embedding model needs `akasha reembed` ([operations](operations.md#upgrading)). |
| `AKASHA_TRANSCRIBE_ENABLED`, `AKASHA_WHISPER_MODEL` | Audio/video transcription. |
| `AKASHA_ML_MODELS_URL`, `AKASHA_OCR_MODELS_URL` | Model download sources; empty = never download. |
| `AKASHA_COOKIE_SECURE`, `AKASHA_TRUSTED_PROXIES` | [Reverse proxy](#reverse-proxy-and-https). |
| `AKASHA_WATCH_ROOTS`, `AKASHA_METRICS_ENABLED`, `OTEL_EXPORTER_OTLP_ENDPOINT` | [Optional features](#optional-features). |
| `POSTGRES_PASSWORD`, `GRAFANA_PASSWORD` | Database and Grafana passwords. |

## Without Docker: build from source

You need Postgres with pgvector ([requirements](#requirements)), Rust (pinned by
`rust-toolchain.toml`; install with [rustup](https://rustup.rs)), Node 22 with pnpm 10 (the
web UI), `cmake` and a C++ compiler (whisper.cpp). [`mise install`](../mise.toml) sets up
Node, pnpm and `just`.

```sh
git clone https://github.com/umeshteja16/project-akasha && cd project-akasha
cp .env.example .env                      # then edit; see below
./scripts/install-onnxruntime.sh          # ONNX Runtime -> ./models/onnxruntime (embeddings, rerank)
(cd web && pnpm install --frozen-lockfile && pnpm build)   # the UI, embedded into the binary
SQLX_OFFLINE=true cargo build --release -p akasha --features embed-ui
# -> target/release/akasha  (one binary: API + web UI + worker)
```

`SQLX_OFFLINE=true` uses the checked-in query cache, so no database is needed to build. The
Docker build also sets `WHISPER_DONT_GENERATE_BINDINGS=1`, which avoids needing `libclang`.
Build on the kind of machine you will run on: whisper.cpp is compiled for the build host's
CPU unless you set the `GGML_*` options as the [Dockerfile](../Dockerfile) does.

The binary reads **environment variables** (and an optional `akasha.toml`), not `.env`:
only `just` loads `.env`. Run it by hand with
`set -a; . ./.env; set +a; target/release/akasha serve --with-worker`, or use systemd. The
essentials (the `.env.example` defaults are meant for development; point the paths somewhere
permanent):

```sh
DATABASE_URL=postgres://akasha:PASSWORD@localhost:5432/akasha
AKASHA_BIND_ADDR=127.0.0.1:8080
AKASHA_STORAGE_DIR=/var/lib/akasha/storage
AKASHA_MODELS_DIR=/var/lib/akasha/models
AKASHA_ORT_DYLIB_PATH=/opt/akasha/libonnxruntime.so   # where you put the ONNX Runtime library
AKASHA_LOG_FORMAT=json
```

`serve` applies database migrations itself on every start. `--with-worker` (or
`AKASHA_SERVE_WITH_WORKER=true`) runs the background worker (extraction, OCR, embedding,
transcription) in the same process; without it nothing gets indexed unless you also run
`akasha worker`.

### systemd

`/etc/systemd/system/akasha.service`, with the binary in `/opt/akasha/akasha`, the settings
above in `/etc/akasha/akasha.env` (mode `0600`) and a service account:
`useradd --system --home /var/lib/akasha --create-home akasha`.

```ini
[Unit]
Description=Akasha
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
User=akasha
EnvironmentFile=/etc/akasha/akasha.env
ExecStart=/opt/akasha/akasha serve --with-worker
Restart=on-failure
TimeoutStopSec=60
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
ReadWritePaths=/var/lib/akasha

[Install]
WantedBy=multi-user.target
```

`systemctl daemon-reload && systemctl enable --now akasha`, then
`journalctl -u akasha -f`. (`TimeoutStopSec` should exceed
`AKASHA_WORKER_SHUTDOWN_GRACE_SECS`, 30 by default, which in-flight jobs get on shutdown.)
Watched folders outside `/var/lib/akasha` need to be readable by the service user.

## First run

1. **Models download in the background.** At start Akasha loads the embedding model and
   reranker, downloading them the first time (about 0.6 GB together) into `AKASHA_MODELS_DIR`
   (`/var/lib/akasha/models`, the `models` volume, in the image). The OCR models (~12 MB) and
   the Whisper model (`base`, 142 MB) are fetched the first time they are needed.
   Until the embedding model is ready, search falls back to keywords and indexing jobs retry;
   nothing needs a restart. Follow progress with `docker compose logs -f akasha`. **Settings →
   System** shows which models are active and ready, ONNX Runtime, OCR and the job queue.
   `akasha models check` verifies the whole setup up front (see [Quick start](#quick-start-docker-compose)).
   To fetch everything before the first start: `akasha models download`.
2. **Create the account.** Open the UI; the sign-in screen links to *Create an account* (email,
   a password of 8 to 128 characters, optional name). All accounts are equal and own
   separate libraries; there is no special admin. Once everyone who needs an account has
   one, set `AKASHA_ALLOW_REGISTRATION=false` and restart so nobody else can sign up.
3. **Upload.** Drag files onto **Library**, or add a note with *New note*. Supported:
   plain text, Markdown, CSV, JSON, PDF (scanned pages are OCRed), images (OCR) and
   audio/video (transcribed). Each file shows *pending* until its extraction and embedding
   jobs have run; the first ones are slower while models load.
4. **Search.** The **Search** screen combines keyword and semantic search with reranking.
   Results that only match loosely are tucked under "Loosely related".
5. **Chat.** **Chat** answers from your files with numbered citations, or says it could not
   find the answer. It needs a language model: Akasha defaults to Ollama on the same machine,
   so [pick and configure one](#choosing-an-llm-for-chat) first.

### Offline and air-gapped installs

Two separate switches:

- **Never download models:** set `AKASHA_ML_MODELS_URL=` and `AKASHA_OCR_MODELS_URL=` (empty).
  On a connected machine with the same settings run `akasha models download` (OCR,
  embedding, rerank and Whisper models), copy the whole models directory to the server, and
  check with `akasha models check`. With Docker:

  ```sh
  docker run --rm -v akasha_models:/var/lib/akasha/models ghcr.io/umeshteja16/project-akasha models download
  ```

  (the volume is named `<project>_models`, e.g. `akasha_models`; see `docker volume ls`).
  Or point `AKASHA_ML_MODELS_URL` at a Hugging Face mirror.
- **Only local chat models:** `AKASHA_STRICT_OFFLINE=true` makes Akasha refuse to start with
  a cloud provider, and allows Ollama and OpenAI-compatible servers only on loopback,
  private-network or single-label (Docker service name) addresses.

## Choosing an LLM for chat

Pick one provider in `.env` (Compose) or the environment. All keys are secrets: keep `.env`
private. The full list of tuning settings is in the
[reference](operations.md#chat-and-language-models); how chat behaves is in the
[README](../README.md#chat--llm-providers).

| Provider | Settings |
|---|---|
| **Ollama** (local, default) | `AKASHA_LLM_PROVIDER=ollama`, `AKASHA_OLLAMA_URL=http://localhost:11434`, `AKASHA_LLM_MODEL=llama3.1:8b` (the default when empty). `ollama pull llama3.1:8b` first. |
| **Anthropic Claude** | `AKASHA_LLM_PROVIDER=anthropic`, `AKASHA_ANTHROPIC_API_KEY=sk-ant-...` (or `ANTHROPIC_API_KEY`). Model defaults to `claude-sonnet-5-5`; set `AKASHA_LLM_MODEL` to change it. |
| **Google Gemini** | `AKASHA_LLM_PROVIDER=gemini`, `AKASHA_GEMINI_API_KEY=...` (or `GEMINI_API_KEY`). Defaults to `gemini-2.5-flash`. |
| **OpenAI-compatible** (LM Studio, vLLM, llama.cpp server, OpenRouter, OpenAI) | `AKASHA_LLM_PROVIDER=openai`, `AKASHA_OPENAI_BASE_URL=http://host:1234/v1` (include `/v1`; default is OpenAI), `AKASHA_LLM_MODEL=<model id>` (**required**), `AKASHA_OPENAI_API_KEY=...` if the server wants one. |
| **None** | `AKASHA_LLM_PROVIDER=none`: chat returns the matching passages only. |

A missing key, or a cloud provider together with `AKASHA_STRICT_OFFLINE=true`, stops the
server at start with a message saying so.

**Ollama and Docker.** Inside the container `localhost` is the container itself. The compose
file already defaults `AKASHA_OLLAMA_URL` to `http://host.docker.internal:11434` (and maps
that name to the host on Linux), so run Ollama on the host listening on all interfaces
(`OLLAMA_HOST=0.0.0.0 ollama serve`, or `Environment="OLLAMA_HOST=0.0.0.0"` through
`systemctl edit ollama`) and keep port 11434 firewalled. Or run it as a container next to
Akasha:

```sh
# in .env: COMPOSE_PROFILES=app,ollama   and   AKASHA_OLLAMA_URL=http://ollama:11434
docker compose up -d
docker compose exec ollama ollama pull llama3.1:8b   # once; models live in the `ollama` volume
```

The container runs on CPU; for a GPU add a `deploy.resources.reservations.devices` entry
(see the Ollama image docs) or keep Ollama on the host. Both variants work with
`AKASHA_STRICT_OFFLINE=true`.

## Reverse proxy and HTTPS

Do not expose the plain-HTTP port to the internet: passwords and session cookies would
travel in the clear. Keep Akasha on a private address and terminate TLS in a proxy.

Akasha settings that go with a proxy:

```sh
AKASHA_TRUSTED_PROXIES=127.0.0.1,::1   # addresses/CIDRs whose X-Forwarded-* headers are believed
AKASHA_COOKIE_SECURE=true              # Secure session cookies (requests the proxy reports as https get them anyway)
```

Without `AKASHA_TRUSTED_PROXIES` every request appears to come from the proxy: the per-IP
sign-in rate limit becomes one shared bucket, and the security log and session list show the
proxy's address. With Docker Compose, a proxy on the host reaches the published port through
the Docker bridge, so trust the Docker address range instead (`AKASHA_TRUSTED_PROXIES=172.16.0.0/12`),
or the Compose network if the proxy is a container there. Only requests whose TCP peer is in
the list get their headers trusted.

Three things every proxy must get right:

- **Upload size:** the largest upload is `AKASHA_MAX_UPLOAD_MB` (512). Set the proxy limit
  slightly above it, or uploads fail with `413` at the proxy.
- **No buffering of streamed responses:** chat answers arrive as Server-Sent Events; a
  buffering proxy shows them only at the end.
- **Long timeouts** for big uploads and downloads.

**Caddy** (certificates automatically; sets `X-Forwarded-*` itself):

```caddyfile
akasha.example.com {
    request_body {
        max_size 520MB
    }
    reverse_proxy 127.0.0.1:8080 {
        flush_interval -1   # stream chat answers (SSE) as they are written
    }
}
```

**nginx:**

```nginx
server {
    listen 443 ssl;
    server_name akasha.example.com;
    # ssl_certificate ...; ssl_certificate_key ...;
    client_max_body_size 520m;            # AKASHA_MAX_UPLOAD_MB plus a little
    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_buffering off;              # stream chat answers (SSE)
        proxy_request_buffering off;
        proxy_read_timeout 3600s;         # long uploads and downloads
    }
}
```

Traefik sets the same headers by default; list its address as trusted. Chain several proxies
(CDN, then nginx) by listing all of them. If the proxy runs on another machine, set
`AKASHA_HTTP_BIND=0.0.0.0` (Compose) or `AKASHA_BIND_ADDR` accordingly and firewall the port
so only the proxy can reach it.

## Optional features

Each is off or harmless by default. Detail lives in the README and ADRs.

- **Audio and video transcription** is on by default (whisper.cpp; `AKASHA_WHISPER_MODEL`:
  `tiny`, `base`, `small`, `medium`, `large-v3-turbo`). Turn it off with
  `AKASHA_TRANSCRIBE_ENABLED=false`: recordings stay playable without text. See the
  [README](../README.md#transcription) and [ADR 0017](adr/0017-audio-video-transcription.md).
- **Watched folders** (Obsidian vaults, Documents): set `AKASHA_WATCH_ROOTS=/watch`, mount the
  folder read-only (in `compose.yaml`: `- ~/Documents/Vault:/watch/Vault:ro`), then add
  `/watch/Vault` under **Settings → Sources**. See the [README](../README.md#watched-folders-obsidian-vaults-documents-downloads)
  and [ADR 0018](adr/0018-watched-folders.md).
- **MCP and API tokens:** create a token in **Settings → Access tokens** and connect Claude
  Code, Claude Desktop or any MCP client to `https://your-host/mcp`; the same tokens work
  for the REST API. Use HTTPS. See the [README](../README.md#use-akasha-from-claude--ai-agents-mcp)
  and [ADR 0015](adr/0015-mcp-server-and-api-tokens.md).
- **Monitoring:** `AKASHA_METRICS_ENABLED=true` and
  `docker compose --profile monitoring up -d` (add it to `COMPOSE_PROFILES`) start
  Prometheus on `127.0.0.1:9091` and Grafana on `127.0.0.1:3000` (user `admin`, password
  `GRAFANA_PASSWORD`, default `admin`: change it). Needs the repo's `deploy/` folder. See the
  [README](../README.md#monitoring) and [ADR 0019](adr/0019-observability.md).
- **S3 storage:** `AKASHA_STORAGE_BACKEND=s3` with the `AKASHA_STORAGE_S3_*` settings
  ([reference](operations.md#storage)).

## Backups and upgrades

Back up **both** the database and the file store, database first, with `scripts/backup.sh`
(or the Docker commands) and test a restore once. Upgrade by pulling the new image
(`docker compose pull && docker compose up -d`) or installing the new binary; migrations run
on start and cannot be undone, so back up first. Details, the restore procedure and the
`akasha reembed` flow for changing the embedding model:
[operations.md](operations.md).

## Troubleshooting

Start with `docker compose logs akasha` (or `journalctl -u akasha`), **Settings → System**
in the UI, and `akasha models check`.

| Symptom | Cause and fix |
|---|---|
| `Illegal instruction` / crash at start or when transcribing (bare binary) | whisper.cpp was built for a newer CPU than the one running it (many NAS Celerons/Atoms have no AVX). Rebuild on that machine, or set `AKASHA_TRANSCRIBE_ENABLED=false`. The Docker image avoids this on amd64 by choosing the AVX2 build only when the CPU has it; `AKASHA_CPU_VARIANT=baseline` forces the portable build. `models check` prints which build ran. |
| Log warns `embedding model unavailable` / `reranker unavailable`; files stay *pending* | The model download failed or `huggingface.co` is blocked. Fix the network, set a mirror in `AKASHA_ML_MODELS_URL`, or [install the models offline](#offline-and-air-gapped-installs). Indexing retries by itself. Meanwhile search works by keyword. |
| `ONNX Runtime unavailable ... (install ONNX Runtime >= 1.24 or set AKASHA_ORT_DYLIB_PATH)` | Bare binary only: run `scripts/install-onnxruntime.sh` and point `AKASHA_ORT_DYLIB_PATH` at the library. The image ships it. |
| Server exits with a database connection error | `DATABASE_URL` wrong, Postgres not up yet, or (Compose) you changed `POSTGRES_PASSWORD` after the volume existed: it only applies to a new volume; set it back, or `ALTER USER akasha PASSWORD '...'` inside Postgres. |
| `extension "vector" is not available` | Postgres without pgvector. Use `pgvector/pgvector:pg17` (the compose file does) or install the extension for your Postgres. |
| `AKASHA_EMBED_MODEL is X, but the search index was built with Y` (server refuses to start) | Guard against mixing vectors from two models. Set `AKASHA_EMBED_MODEL` back to the recorded one, or switch deliberately: stop everything, `akasha reembed`, start again ([operations](operations.md#upgrading)). Also seen after restoring a backup made with another model. |
| Chat says no answer model / connection refused to Ollama | The provider is not reachable from the container: see [Ollama and Docker](#choosing-an-llm-for-chat). Check **Settings → System** for the active provider. |
| Server will not start: key missing or "strict offline" error | Set the provider's API key, or use a local provider; see [Choosing an LLM](#choosing-an-llm-for-chat). |
| Sign-up says registration is disabled (`403`) | `AKASHA_ALLOW_REGISTRATION=false`. Turn it on, create the account, turn it off. |
| Signed in but immediately signed out | `AKASHA_COOKIE_SECURE=true` over plain HTTP: browsers drop `Secure` cookies there. Use HTTPS, or set it to `false` for local testing. |
| Uploads fail with `413` | Proxy `client_max_body_size` / `max_size` below `AKASHA_MAX_UPLOAD_MB`. |
| Chat answers appear all at once | The proxy buffers; see [Reverse proxy](#reverse-proxy-and-https). |
| Too many sign-in attempts from "everyone" | No `AKASHA_TRUSTED_PROXIES`, so all clients share the proxy's address. |
| Disk filling up | Check `docker system df -v`: `storage` holds your files (identical uploads are stored once), `models` the models (switching models leaves the old files behind), `pgdata` text and vectors. Big transcription models (`medium` 1.5 GB, `large-v3-turbo` 1.6 GB) are the usual culprit. |
| Port 8080 already in use | Change the left side of the `ports:` entry in `compose.yaml`, or `AKASHA_BIND_ADDR` for the bare binary. |
