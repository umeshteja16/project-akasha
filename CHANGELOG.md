# Changelog

All notable changes to Akasha. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and versions follow [Semantic Versioning](https://semver.org/) (before 1.0, minor versions may
break compatibility; upgrade notes are in [`docs/operations.md`](docs/operations.md)).

## [0.1.0] - 2026-10-10

First release of the rewrite: a single Rust binary with the web UI embedded, backed by
Postgres + pgvector. Self-hosting guide: [`docs/self-hosting.md`](docs/self-hosting.md).

### Added
- **Accounts**: registration, sign-in with argon2id passwords, server-side sessions you can
  list and revoke, per-IP rate limiting on credential routes, security log.
- **Files**: streaming upload, content-addressed storage (local directory or S3), rename,
  tags, pins, bulk delete, download, thumbnails, notes written in the app.
- **Ingestion**: text, Markdown, PDF and image OCR extraction, chunking and embeddings in a
  Postgres-backed job queue; audio and video transcription with whisper.cpp and a
  timestamped transcript player.
- **Search**: hybrid keyword + semantic search (Postgres full-text + pgvector, reciprocal
  rank fusion, reranking, relevance floor), filters, spelling suggestions, similar files,
  and a search-quality benchmark gate in CI.
- **Chat**: grounded answers with citations over your files, streamed over SSE, with
  Ollama, Anthropic, Gemini and OpenAI-compatible providers; model-written file summaries,
  suggested tags and conversation titles.
- **Organising**: collections, activity timeline, watched folders that import and keep
  files in sync.
- **Integrations**: MCP server at `/mcp` (plus an `akasha mcp` stdio bridge) and personal
  API tokens with read or write scope; OpenAPI contract in `openapi.json`.
- **Operations**: Prometheus metrics and OpenTelemetry traces, Grafana dashboard, backup
  and restore scripts with a round-trip test, strict offline mode, trusted reverse
  proxies.
- **Packaging**: multi-arch Docker image (`linux/amd64`, `linux/arm64`) on
  `ghcr.io/umeshteja16/project-akasha`; the amd64 image runs on any x86-64 CPU and picks
  an AVX2 build when the CPU supports it.
- **UI**: redesigned React interface (library, file detail, search, chat, collections,
  activity, settings), keyboard accessible and checked with axe.

### Removed
- The previous TypeScript implementation. Refresh tokens are replaced by server-side
  sessions; URL capture and the storage-usage screen are not in this release.

[0.1.0]: https://github.com/umeshteja16/project-akasha/releases/tag/v0.1.0
