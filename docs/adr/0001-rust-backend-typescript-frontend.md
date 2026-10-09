# 0001: Rewrite the backend in Rust, keep a TypeScript frontend

- Status: accepted · 2026-10-09

## Context
The original stack had a Node API, a Node worker, a Python embedding service and Redis: four
runtimes to deploy and keep in sync, no tests and no CI. The workload is CPU-heavy (parsing,
OCR, embeddings, ranking) and must run well on modest self-hosted hardware.

## Decision
Rewrite the backend as one Rust workspace producing a single `akasha` binary (Axum, Tokio,
SQLx). Keep the UI in React + TypeScript, rebuilt from scratch with a new design.

## Consequences
- One deployable artifact, low memory, and the compiler and compile-checked SQL catch most
  mistakes. That fast feedback suits agent-driven development.
- Slower compiles than TS; mitigated by optimised dependency builds and small crates.
- Rust's document-parsing ecosystem is thinner than Python's. A Python sidecar (e.g. Docling)
  stays an option for complex layouts, behind an interface, if ever needed.
- Rust UI frameworks were rejected: smaller ecosystem and less agent familiarity than React.
