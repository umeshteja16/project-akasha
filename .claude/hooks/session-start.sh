#!/usr/bin/env bash
# Prepares a Claude Code cloud session so `just check` works immediately.
# Local machines are left alone (use `mise install && just setup`).
set -euo pipefail
[ "${CLAUDE_CODE_REMOTE:-}" = "true" ] || exit 0

cd "$CLAUDE_PROJECT_DIR"
command -v just >/dev/null || cargo install just --locked -q
./scripts/local-postgres.sh >/dev/null || echo "warning: could not start local postgres" >&2
cargo fetch -q
(cd web && pnpm install --frozen-lockfile --silent)
echo "export DATABASE_URL=postgres://akasha:akasha@localhost:5432/akasha" >> "${CLAUDE_ENV_FILE:-/dev/null}"
echo "Session ready. Read PROGRESS.md first."
