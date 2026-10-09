# 0006: Server first, desktop app later

- Status: accepted · 2026-10-09

## Decision
Build a self-hosted multi-user server first (Postgres). After v0.1, ship a Tauri 2 desktop app
that reuses the same crates. Whether desktop embeds Postgres or uses SQLite is decided then, in
its own ADR.

## Consequences
Keep domain logic in library crates, not in HTTP handlers, so the desktop app can call it directly.
