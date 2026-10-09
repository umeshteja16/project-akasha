# 0005: OpenAPI generated from Rust is the API contract

- Status: accepted · 2026-10-09

## Context
Legacy frontend and backend types drifted because both were written by hand.

## Decision
Annotate handlers with `utoipa`. `akasha openapi` writes `openapi.json` (checked in). The web
client's types are generated from it with `openapi-typescript`. CI fails if `openapi.json` is stale.

## Consequences
Changing an endpoint changes the TS types, so the frontend breaks at compile time, not at runtime.
