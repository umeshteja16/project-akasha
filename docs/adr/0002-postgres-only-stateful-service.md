# 0002: Postgres is the only stateful service

- Status: accepted · 2026-10-09

## Context
Legacy used Redis only for BullMQ and an occasional cache. Every extra service is more ops
for a self-hosted, local-first product.

## Decision
Use Postgres for data, vectors (pgvector), full-text search and the job queue
(`FOR UPDATE SKIP LOCKED`). No Redis. Files live in object storage via the `object_store` crate
(local disk by default, S3-compatible optional).

## Consequences
Deployment is the binary plus Postgres. If queue throughput ever outgrows Postgres the queue
sits behind a small interface and can be swapped.
