# 0004: Server-side cookie sessions instead of JWT

- Status: accepted · 2026-10-09

## Context
The UI is first-party and served from the same origin. Legacy hand-rolled JWT access + refresh
rotation in ~450 lines.

## Decision
Use server-side sessions stored in Postgres, an HttpOnly, SameSite=Lax cookie (Secure
in production) and argon2id password hashing. OIDC login can be added later. Machine clients
(e.g. MCP) will get separate, revocable API tokens.

## Consequences
Logout and revocation are immediate; no token refresh logic in the UI.

## Amendment (2026-10-09, implementation)
Implemented as a small `sessions` table of our own instead of `tower-sessions`: we only need
"token → user", and owning it gives typed queries, per-user revocation (used on password
change) and SHA-256-hashed tokens at rest in about 100 lines.
