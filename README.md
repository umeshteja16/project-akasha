# Akasha

A self-hosted, private knowledge retrieval server: upload your documents, images and notes,
then search and ask questions over them with grounded, cited answers. It can run fully offline.

> **Status: rewrite in progress.** The project is being rebuilt in Rust. See
> [`PROGRESS.md`](PROGRESS.md) for what works today and what is next. The previous TypeScript
> implementation is kept in [`legacy/`](legacy) for reference.

## Architecture

```
React + TypeScript UI  ──REST (OpenAPI)──▶  akasha (single Rust binary)  ──▶  Postgres + pgvector
                                            serve · worker · migrate · mcp      (+ file storage)
```

- **Backend:** Rust, Axum, Tokio, SQLx, Postgres 17 + pgvector
- **ML:** embeddings and reranking in-process via ONNX Runtime (`fastembed`, [ADR 0009](docs/adr/0009-embeddings-onnx-runtime.md)); LLMs via Ollama / Claude / Gemini (planned)
- **Frontend:** React 19, TypeScript, Vite, Tailwind CSS 4, Radix (shadcn/ui-style), TanStack Router + Query, Biome, Vitest, Playwright ([`web/DESIGN.md`](web/DESIGN.md))

Design decisions are recorded in [`docs/adr/`](docs/adr).

## Quick start

```bash
mise install                 # toolchain (or install Rust, Node 22, pnpm, just yourself)
cp .env.example .env
just setup                   # cargo fetch + pnpm install
just db-up                   # Postgres in Docker (or: just db-local)
just serve                   # API on http://localhost:8080
just web                     # UI on http://localhost:5173
```

Production-style run of the whole stack: `docker compose --profile app up --build`.
The image is one binary serving both the API and the web UI (embedded at build time
with the `embed-ui` cargo feature). Natively: `just build-ui`, then
`target/debug/akasha serve` serves the UI on http://localhost:8080.

## ML models and offline installs

Embeddings and reranking run inside the binary on ONNX Runtime. The Docker image ships the
runtime library; for a native build run `just onnxruntime` (or install ONNX Runtime ≥ 1.24)
and set `AKASHA_ORT_DYLIB_PATH`. Models are downloaded into `AKASHA_MODELS_DIR` on first use:

| Setting | Default | Notes |
|---|---|---|
| `AKASHA_EMBED_MODEL` | `multilingual-e5-small` (384 d) | also `bge-small-en-v1.5`, `bge-base-en-v1.5`, `nomic-embed-text-v1.5`, `bge-m3` |
| `AKASHA_RERANK_MODEL` | `jina-reranker-v1-turbo-en` | also `bge-reranker-base`, `bge-reranker-v2-m3`, `none` |
| `AKASHA_ML_MODELS_URL` | `https://huggingface.co` | any Hugging Face mirror; empty = never download |

**Air-gapped hosts:** on a connected machine with the same settings run
`akasha models download` (OCR, embedding and rerank models), copy the models directory to the
target, set `AKASHA_ML_MODELS_URL=` and `AKASHA_OCR_MODELS_URL=` (empty), and verify with
`akasha models check`. With Docker:
`docker run --rm -v akasha_models:/var/lib/akasha/models <image> models download`.

**Changing the embedding model** invalidates every stored vector, so the server refuses to
start with a model other than the one the index was built with. To switch: stop the
workers, set `AKASHA_EMBED_MODEL`, run `akasha reembed` (resizes the vector column and
queues every file), then start again. Files stay searchable by keyword meanwhile.

## Chat & LLM providers

`POST /api/v1/conversations/{id}/messages` answers a question from your files: it searches
them, refuses ("I couldn't find this in your files.") when the best passage is too weak,
otherwise streams an answer over Server-Sent Events (`sources`, `delta`, `done`/`error`) that
cites numbered sources as `[n]`. Generation runs on an external model (ADR 0012):

```sh
# Offline (default): Ollama on this machine. `ollama pull llama3.1:8b` first.
AKASHA_LLM_PROVIDER=ollama
AKASHA_OLLAMA_URL=http://localhost:11434
AKASHA_LLM_MODEL=llama3.1:8b
AKASHA_STRICT_OFFLINE=true        # refuse to start with any non-local provider

# Anthropic Claude (default model claude-sonnet-5-5; set AKASHA_LLM_MODEL=claude-opus-5-5 for Opus)
AKASHA_LLM_PROVIDER=anthropic
AKASHA_ANTHROPIC_API_KEY=sk-ant-...   # or ANTHROPIC_API_KEY

# Google Gemini (default model gemini-2.5-flash; the key is sent as a header)
AKASHA_LLM_PROVIDER=gemini
AKASHA_GEMINI_API_KEY=...             # or GEMINI_API_KEY

# Any OpenAI-compatible server: LM Studio, vLLM, llama.cpp server, OpenRouter, OpenAI
AKASHA_LLM_PROVIDER=openai
AKASHA_OPENAI_BASE_URL=http://localhost:1234/v1
AKASHA_LLM_MODEL=qwen2.5-7b-instruct
# AKASHA_OPENAI_API_KEY=...           # if the server needs one

# No model: chat returns the matching passages (status `no_llm`)
AKASHA_LLM_PROVIDER=none
```

With strict offline mode, Ollama and OpenAI-compatible servers are allowed only on loopback,
private-network or single-label (Docker service) addresses. Requests are retried on
connection errors, 429 and 5xx before output starts; `AKASHA_LLM_READ_TIMEOUT_SECS` bounds
silence while streaming. Chat is limited per user (`AKASHA_CHAT_RATE_PER_MINUTE`, default 20);
the refusal threshold is `AKASHA_CHAT_MIN_RERANK_SCORE` (default per reranker; `akasha eval`
prints a calibration report for it). See `.env.example` for every setting.

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

### Ollama with Docker Compose

The `akasha` container reaches Ollama **on the host** at `http://host.docker.internal:11434`
(`extra_hosts: host-gateway` makes that name work on Linux too). Ollama only listens on
`127.0.0.1` by default, so start it with `OLLAMA_HOST=0.0.0.0 ollama serve` (or
`systemctl edit ollama` → `Environment="OLLAMA_HOST=0.0.0.0"`) and keep port 11434
firewalled from the network. Or run Ollama as a container next to Akasha:

```sh
AKASHA_OLLAMA_URL=http://ollama:11434 docker compose --profile app --profile ollama up -d --build
docker compose exec ollama ollama pull llama3.1:8b   # once; models live in the `ollama` volume
```

Both work with `AKASHA_STRICT_OFFLINE=true` (`ollama` is a single-label Docker service name).
The container runs on CPU; for a GPU, add a `deploy.resources.reservations.devices` entry
(see the Ollama image docs) or keep using Ollama on the host.

## Use Akasha from Claude / AI agents (MCP)

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

## Development

`just check` runs exactly what CI runs. Contributor and agent conventions: [`CLAUDE.md`](CLAUDE.md).
