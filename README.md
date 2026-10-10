# Akasha

A self-hosted, private knowledge base: upload your documents, scans, notes, audio and video,
then search them and ask questions that are answered from your own files, with citations. It
runs on your own hardware and can work fully offline.

- **Hybrid search**: keywords plus semantic search, reranked, with filters and "similar files".
- **Grounded chat**: answers cite numbered sources and say so when your files do not contain
  the answer. Bring Ollama (local), Claude, Gemini or any OpenAI-compatible server.
- **Reads everything**: text, Markdown, PDF, images (OCR) and recordings (speech-to-text).
- **Connected**: watched folders (Obsidian vaults), an MCP server and API tokens for AI agents.
- **One binary, one database**: Rust server with the web UI embedded; Postgres + pgvector.

> **Status:** pre-release; v0.1 is being prepared. See [`PROGRESS.md`](PROGRESS.md) for what works
> and what is next.

## Self-hosting

**[docs/self-hosting.md](docs/self-hosting.md)** takes you from nothing to a running server:
requirements, Docker Compose quick start (prebuilt multi-arch image
`ghcr.io/umeshteja16/project-akasha`), building from source with systemd, first run and
models, choosing a chat model, HTTPS behind Caddy or nginx, and troubleshooting.
**[docs/operations.md](docs/operations.md)** covers backup and restore, upgrades and the
reference of every setting. [`.env.example`](.env.example) lists the settings with defaults.

## Architecture

```
React + TypeScript UI  ──REST (OpenAPI)──▶  akasha (single Rust binary)  ──▶  Postgres + pgvector
                                            serve · worker · migrate · mcp      (+ file storage)
```

- **Backend:** Rust, Axum, Tokio, SQLx, Postgres 17 + pgvector
- **ML:** embeddings and reranking in-process via ONNX Runtime (`fastembed`, [ADR 0009](docs/adr/0009-embeddings-onnx-runtime.md)); chat via Ollama, Claude, Gemini or any OpenAI-compatible server
- **Frontend:** React 19, TypeScript, Vite, Tailwind CSS 4, Radix (shadcn/ui-style), TanStack Router + Query, Biome, Vitest, Playwright ([`web/DESIGN.md`](web/DESIGN.md))

Design decisions are recorded in [`docs/adr/`](docs/adr).

## Features and reference

Setup is in the self-hosting guide; this is how the features behave.

### Search models

Embeddings and reranking run inside the binary on ONNX Runtime (`fastembed`,
[ADR 0009](docs/adr/0009-embeddings-onnx-runtime.md)); models are downloaded on first start.
`AKASHA_EMBED_MODEL` (default `multilingual-e5-small`) and `AKASHA_RERANK_MODEL` (default
`jina-reranker-v1-turbo-en`) choose them; changing the embedding model needs `akasha reembed`
([operations](docs/operations.md#upgrading)). Offline installs: [self-hosting](docs/self-hosting.md#offline-and-air-gapped-installs).

### Transcription

**Audio and video** are transcribed on the server with whisper.cpp (ADR 0017): MP3, WAV,
M4A/AAC, FLAC, Ogg (Vorbis/Opus), MP4/MOV (AAC) and WebM (Opus/Vorbis); the file page shows
the transcript next to the player, and search results and chat citations point at the time
("1:05"). The speech model is downloaded on first use like the others.

| Setting | Default | Notes |
|---|---|---|
| `AKASHA_TRANSCRIBE_ENABLED` | `true` | off: recordings are stored and playable, without text |
| `AKASHA_WHISPER_MODEL` | `base` (142 MB) | `tiny`, `small` (466 MB, better), `medium`, `large-v3-turbo` (best, needs a fast CPU) |
| `AKASHA_TRANSCRIBE_THREADS` | `0` | CPU threads per transcription (0 = cores, max 8); one recording at a time |
| `AKASHA_TRANSCRIBE_MAX_MINUTES` | `120` | only the start of longer recordings is transcribed |
| `AKASHA_TRANSCRIBE_LANGUAGE` | empty | ISO 639-1 code (`en`, `de`, ...); empty detects it |

### Chat & LLM providers

`POST /api/v1/conversations/{id}/messages` answers a question from your files: it searches
them, refuses ("I couldn't find this in your files.") when the best passage is too weak,
otherwise streams an answer over Server-Sent Events (`sources`, `delta`, `done`/`error`) that
cites numbered sources as `[n]`. Generation runs on an external model (ADR 0012). Choose the provider and its settings in the [self-hosting guide](docs/self-hosting.md#choosing-an-llm-for-chat).

With strict offline mode, Ollama and OpenAI-compatible servers are allowed only on loopback,
private-network or single-label (Docker service) addresses. Requests are retried on
connection errors, 429 and 5xx before output starts; `AKASHA_LLM_READ_TIMEOUT_SECS` bounds
silence while streaming. Chat is limited per user (`AKASHA_CHAT_RATE_PER_MINUTE`, default 20);
the refusal threshold is `AKASHA_CHAT_MIN_RERANK_SCORE` (default per reranker; `akasha eval`
prints a calibration report for it). See [`docs/operations.md`](docs/operations.md) for every setting.

Search hides results that only vector similarity found and that score below a per-model
**relevance floor** (ADR 0014), so off-topic queries return nothing instead of every file;
`include_weak=true` (or "Loosely related" in the UI) shows them. Override the floors with
`AKASHA_SEARCH_MIN_SIMILARITY` / `AKASHA_SEARCH_MIN_RERANK_SCORE`. Settings → System (and
`GET /api/v1/system/status`) shows the active models, ONNX Runtime, OCR, strict offline mode
and the job queue.

With a model configured, every file also gets a short **summary and 3-5 suggested tags** once
it is indexed (`enrich_file` job, one model call per file, `AKASHA_LLM_ENRICH_FILES=false` to
turn off). Suggested tags (`auto_tags`) are kept apart from your own `tags`, which the model
never changes; tag filters and search match both, and `PATCH /api/v1/files/{id}` with
`auto_tags` drops wrong suggestions. `POST /api/v1/files/{id}/enrich` asks again. After the
first answer, a conversation is renamed by the model (`AKASHA_LLM_CONVERSATION_TITLES`) unless
you renamed it yourself.

### Use Akasha from Claude / AI agents (MCP)

Akasha is an [MCP](https://modelcontextprotocol.io) server: AI assistants can search your
library, read files, list them and ask grounded questions (and, with a write token, add notes
and tag files). Create a token in **Settings → Access tokens** (it is shown once), then:

**Claude Code** (remote, Streamable HTTP):

```sh
claude mcp add --transport http akasha https://akasha.example.com/mcp \
  --header "Authorization: Bearer akasha_pat_…"
```

**Claude Desktop and other clients that start a local program** (stdio): install the
`akasha` binary on your laptop and add to the client's MCP config (for Claude Desktop,
`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "akasha": {
      "command": "akasha",
      "args": ["mcp"],
      "env": { "AKASHA_URL": "https://akasha.example.com", "AKASHA_TOKEN": "akasha_pat_…" }
    }
  }
}
```

`akasha mcp` only forwards messages to the server's `/mcp` endpoint (it needs no database
or models); on the server itself use `AKASHA_URL=http://127.0.0.1:8080`. Any other MCP
client: Streamable HTTP at `<server>/mcp` with the `Authorization: Bearer` header.

Tools: `search`, `get_file`, `read_file`, `list_files`, `ask`; with the write scope also
`add_note` and `tag_file`. Files are also resources (`akasha://file/{id}`).

Security: a token can read **everything** in your library. Prefer read-only tokens with an
expiry, one per client, and revoke unused ones. Expose the server only over **HTTPS** (a
reverse proxy with TLS), never plain HTTP across the internet. Tokens can't manage your
account or other tokens. The same tokens work for the REST API
(`Authorization: Bearer …`; read-only tokens may only `GET`). Design: ADR 0015.

### Watched folders (Obsidian vaults, Documents, Downloads)

Akasha can import folders on the server and keep them in sync: new and changed files show
up within seconds, renamed files keep their tags and collections, deleted files are
removed (or kept, per folder). Markdown front-matter `tags:` become tags, and hidden
folders such as `.obsidian/` and `.trash/` are skipped. Akasha never writes to the folders.

The administrator decides which folders users may add:

```sh
AKASHA_WATCH_ROOTS=/watch                  # comma-separated; empty (default) = feature off
AKASHA_WATCH_ROOTS=/data/users/{email}     # or one subtree per user ({user_id} works too)
AKASHA_WATCH_SCAN_MINUTES=15               # full rescan interval (0 = only on file events)
AKASHA_WATCH_FS_EVENTS=true                # inotify/FSEvents for near-instant pickup
```

With Docker, mount folders read-only under the root, e.g. in `compose.yaml`:
`- ~/Documents/Vault:/watch/Vault:ro` and `AKASHA_WATCH_ROOTS=/watch`. Users then add
`/watch/Vault` under **Settings → Sources** (or `POST /api/v1/sources`), where they can
pause, rescan or remove a folder (optionally deleting the files it imported).

Safety: paths are resolved (`..`, symlinks) and must stay inside a root; symlinks inside
a folder are never followed; files go through the same type and size checks as uploads
and count against the user's quota. A folder that suddenly lists as empty (unmounted
volume) deletes nothing. Large trees are imported in batches; on Linux, raise
`fs.inotify.max_user_watches` for very large ones (periodic scans still cover them).
Design: ADR 0018.

### Monitoring

Prometheus metrics (HTTP requests and latency by route, job queue depth, wait, duration
and failures by kind, extraction/embedding/transcription time, search latency by stage,
language-model calls and tokens, model readiness):

```sh
AKASHA_METRICS_ENABLED=true
AKASHA_METRICS_BIND_ADDR=0.0.0.0:9090      # separate listener, no auth: keep it private
# or, on the main port with a token:
AKASHA_METRICS_TOKEN=some-long-random-string   # scrape with Authorization: Bearer <token>
```

`akasha worker` processes expose their (job) metrics only on `AKASHA_METRICS_BIND_ADDR`.
`docker compose --profile app --profile monitoring up` adds Prometheus
(http://localhost:9091) and Grafana (http://localhost:3000, user `admin`, password
`GRAFANA_PASSWORD` or `admin`) with the dashboard in
[`deploy/grafana/akasha.json`](deploy/grafana/akasha.json) (import it into any Grafana
too; it expects a Prometheus data source with uid `prometheus`).

Traces: set `OTEL_EXPORTER_OTLP_ENDPOINT` (e.g. `http://otel-collector:4318`) to export
OpenTelemetry spans over OTLP/HTTP; the other standard `OTEL_*` variables apply
(`OTEL_SERVICE_NAME` defaults to `akasha`). Request spans carry the `x-request-id` that
every response returns (and that JSON logs include), and continue an incoming
`traceparent`. Design: ADR 0019.

## Development

```bash
mise install                 # toolchain (or install Rust, Node 22, pnpm, just yourself)
cp .env.example .env
just setup                   # cargo fetch + pnpm install
just db-up                   # Postgres in Docker (or: just db-local)
just serve                   # API + worker on http://localhost:8080
just web                     # UI dev server on http://localhost:5173
```

Native runs need ONNX Runtime for embeddings (`just onnxruntime`), and cmake plus a C++
compiler for whisper.cpp (`cargo build --no-default-features --features onnx` leaves speech
out). `just build-ui` embeds the built UI in the server binary. `just check` runs exactly what
CI runs; `just` lists every recipe. Contributor and agent conventions: [`CLAUDE.md`](CLAUDE.md).
Design decisions are in [`docs/adr/`](docs/adr); the docs index is [`docs/README.md`](docs/README.md).
