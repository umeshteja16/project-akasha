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
- **Frontend:** React 19, TypeScript, Vite, Biome, Vitest

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

# Anthropic Claude (default model claude-opus-5-5)
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
the refusal threshold is `AKASHA_CHAT_MIN_RERANK_SCORE` (default per reranker). See
`.env.example` for every setting.

## Development

`just check` runs exactly what CI runs. Contributor and agent conventions: [`CLAUDE.md`](CLAUDE.md).
