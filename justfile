# Task runner. `just` lists recipes. Every agent/dev runs `just check` before committing.
set dotenv-load := true

default:
    @just --list

# Install all dependencies (Rust crates + web packages).
setup:
    cargo fetch
    cd web && pnpm install --frozen-lockfile

# Start Postgres with Docker (normal dev machine).
db-up:
    docker compose up -d postgres

db-down:
    docker compose down

# Start Postgres without Docker (cloud agent sessions).
db-local:
    ./scripts/local-postgres.sh

# Run the API server and background worker in one process (applies migrations first).
serve:
    cargo run -p akasha -- serve --with-worker

# Run only the background job worker.
worker:
    cargo run -p akasha -- worker

# Download the ONNX Runtime library (embeddings/reranking) into ./models/onnxruntime.
onnxruntime:
    ./scripts/install-onnxruntime.sh

# Fetch every configured ML model into AKASHA_MODELS_DIR (offline installs).
models:
    cargo run -p akasha -- models download

# Run the web dev server (proxies /api to the Rust server).
web:
    cd web && pnpm dev

# Build the web UI and a server binary with it embedded (production is one binary).
build-ui:
    cd web && pnpm build
    cargo build -p akasha --features embed-ui

# Playwright end-to-end tests against the real stack (Postgres + akasha serve with
# fake models + the embedded UI). Not part of `check`; CI runs it as its own job.
e2e: build-ui
    cd web && pnpm e2e

# Apply migrations with sqlx-cli (no app build needed; the SQL macros require the schema).
db-migrate:
    sqlx migrate run --source crates/db/migrations

# Apply migrations only.
migrate:
    cargo run -p akasha -- migrate

# Create a reversible migration: just migration add_users
migration name:
    sqlx migrate add -r {{name}} --source crates/db/migrations

# Refresh the offline SQL query cache (.sqlx/). Run after changing any sqlx::query! macro.
sqlx-prepare:
    cargo sqlx prepare --workspace -- --all-targets

# Format everything.
fmt:
    cargo fmt --all
    cd web && pnpm fmt

# Regenerate openapi.json and the typed web client from the Rust routes.
openapi:
    cargo run -q -p akasha -- openapi > openapi.json
    cd web && pnpm gen:api

# The gate: everything CI runs. Needs Postgres (just db-up or just db-local).
check: db-migrate check-rust check-sqlx check-web check-openapi

check-rust:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace

# Fails if .sqlx/ is stale (run `just sqlx-prepare`).
check-sqlx:
    cargo sqlx prepare --workspace --check -- --all-targets

check-web:
    cd web && pnpm lint && pnpm typecheck && pnpm test && pnpm build

# Fails if openapi.json is stale (run `just openapi`).
check-openapi:
    cargo run -q -p akasha -- openapi | diff -u openapi.json - || (echo "openapi.json is stale: run 'just openapi'" && exit 1)
