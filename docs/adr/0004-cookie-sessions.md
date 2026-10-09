# 0004: Server-side cookie sessions instead of JWT

- Status: accepted · 2026-10-09

## Context
The UI is first-party and served from the same origin. Legacy hand-rolled JWT access + refresh
rotation in ~450 lines.

## Decision
Use `tower-sessions` with sessions stored in Postgres, an HttpOnly, SameSite=Lax cookie (Secure
in production) and argon2id password hashing. OIDC login can be added later. Machine clients
(e.g. MCP) will get separate, revocable API tokens.

## Consequences
Logout and revocation are immediate; no token refresh logic in the UI.
