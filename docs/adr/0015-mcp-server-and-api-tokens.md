# 0015: MCP server and personal API tokens

- Status: accepted · 2026-10-10

## Context
Akasha's headline feature for step 6 is being usable by AI agents (Claude Code, Claude
Desktop, IDE assistants) through the Model Context Protocol. Agents are machine clients:
they cannot hold a browser session cookie, and ADR 0004 promised separate, revocable API
tokens for them. Agents run on laptops while Akasha typically runs on a home server, and
some clients only speak MCP over stdio (they launch a local process).

## Decision
- **Personal API tokens** (`api_tokens`, migration 0012): `akasha_pat_` + 32 random bytes
  (base64url), shown once, stored as SHA-256 like sessions, with a visible prefix for
  recognition, a name, scopes (`read`, or `read` + `write`), optional expiry (1–3650 days),
  `last_used_at` (refreshed at most once a minute) and soft revocation (`revoked_at`).
  At most 50 active tokens per user.
- `AuthUser` accepts `Authorization: Bearer <token>` wherever the cookie works (the header
  wins when both are present). **Scope rule in the extractor**: a read-only token may only
  make `GET`/`HEAD` requests (403 otherwise), so no handler can forget the check. Account and
  token management take a cookie-only `SessionUser`: a token can never mint tokens, change
  the password, log out sessions or delete the account. Rate limits stay per user/IP.
- **MCP over Streamable HTTP at `/mcp`**, served by the official Rust SDK (`rmcp`, pinned
  `=3.5.1`) inside the main router, so it shares state, pool and models with the API.
  - Bearer token required (cookies ignored); an axum middleware resolves it and passes the
    `AuthUser` to the handler through request extensions; 401 + `WWW-Authenticate` otherwise.
  - **Stateless** (`legacy_session_mode = false`, JSON responses, `NeverSessionManager`): one
    POST per JSON-RPC message, nothing kept between requests, any number of processes can
    serve it. Host/Origin allow-lists are disabled: they protect unauthenticated local
    servers from DNS rebinding, and here every request carries a secret a web page cannot
    know.
  - Tools, owner-scoped and bounded for model context: `search`, `get_file`, `read_file`
    (windows of ≤ 20k characters, by page or offset), `list_files` (cursor), `ask` (grounded
    answer with citations using the chat pipeline without storing a conversation; passages
    when no model is configured) and, with `write` scope only, `add_note` (through the normal
    upload path: dedupe, quota, ingest jobs) and `tag_file`. Read-only tokens do not see the
    write tools. Failures are MCP tool errors (`isError`) the model can read; only unknown
    tools are protocol errors. Every result is capped (~32k characters).
  - Resources: `akasha://file/{id}` (first 50k characters of the extracted text); the 50
    newest ready files are listed.
- **stdio = a bridge, not a second server**: `akasha mcp` (`AKASHA_URL`, `AKASHA_TOKEN`)
  reads JSON-RPC lines from stdin, POSTs them to the server's `/mcp` and writes the replies
  to stdout. It needs no database, config or models, works from a laptop against a home
  server and on the server itself (`http://127.0.0.1:8080`), and cannot drift from the HTTP
  tools. Running tools directly against the database from the CLI was rejected: it would
  load models into a second process and duplicate auth and rate limits.

## Consequences
- A token is full read access to a library: the UI recommends read-only tokens with an
  expiry, and the README says to use HTTPS for remote access.
- Read-only tokens cannot ask questions through the REST chat endpoint (it is a `POST` that
  stores a conversation); the MCP `ask` tool is read-only and works with them.
- Stateless MCP has no server-to-client notifications or progress; none of the tools need
  them. Protocol upgrades come with `rmcp` bumps (pinned exactly; bump deliberately).
