# 0011: Search evaluation, spelling vocabulary and thumbnails

- Status: accepted · 2026-10-09

## Context
Step 3 needs a way to tell whether a change to retrieval, fusion or ranking made
search better or worse, a "did you mean" for misspelt keyword queries (legacy
parity), and step 2 left thumbnails open. All three must respect the existing
constraints: Postgres is the only stateful service (ADR 0002), tests never download
models, and another user's data must never leak.

## Decision

### Evaluation (`akasha eval`, `crates/app/src/eval`)
- A committed benchmark in `eval/`: a corpus of short self-written text/Markdown/CSV/
  JSON documents (no models needed for extraction) and `queries.json` with the files
  that answer each query, grouped by kind (keyword, paraphrase, filename, multi,
  legacy). The legacy `eval-benchmark.json` queries are ported into it.
- The corpus goes through the **real pipeline** (upload sniffing, `store::save`, the
  extract and embed jobs, run one at a time so chunk ids are reproducible) for a
  throwaway user, then every query runs through `akasha_search::search_files` in
  each mode (keyword, semantic, hybrid, hybrid + rerank). Metrics: Recall@1/5/10,
  MRR, nDCG@10 (binary relevance over file names), p50/p95 latency.
- The CLI runs in a **scratch database** created next to `DATABASE_URL` and dropped
  afterwards (`akasha_db::scratch`), so it is safe against a live server's Postgres
  and the job worker never sees real jobs.
- **Two tiers.** Deterministic (`hash-384` + `overlap`): runs inside `cargo test`
  (`crates/app/tests/eval.rs`) against `eval/baselines/hash-384+overlap.json`, so
  every CI run guards the fusion/ranking code. Real models (`--real-models`, the
  configured models): run manually or by the nightly/on-demand
  `.github/workflows/eval.yml`; baselines per model pair
  (`eval/baselines/<embed>+<rerank>.json`).
- **Gate:** any quality metric more than the tolerance (default 0.02) below the
  baseline fails. Latency is reported, never gated. Baselines are re-recorded
  deliberately with `--update-baseline` and reviewed like code.

### Spelling suggestions (`user_terms`, migration 0008)
- A **per-owner vocabulary table** (`owner_id, term, chunk_count`), with the words
  of each chunk (`to_tsvector('simple')` lexemes of 3-32 letters), kept exact by
  **statement-level triggers with transition tables** on `file_chunks` insert and
  delete. Every path that adds or removes chunks (extraction, reindex, file and
  account deletion) updates it without application code that could be forgotten.
- Alternatives rejected: `ts_stat` over the owner's chunks per query (scans every
  chunk: too slow for large libraries); a global vocabulary (would leak words across
  users); a job-maintained table (lags and needs its own delete bookkeeping).
- Lookups use `pg_trgm` similarity (≥ 0.4, legacy value) through a GIN index on
  `(owner_id, term gin_trgm_ops)` (`btree_gin`), always filtered by owner. A word is
  "known" if it, or a word with the same English stem, is in the vocabulary.
  Suggestions are made only when keyword + file-name retrieval found < 3 chunks, and
  returned as `suggestion` on the search response.
- Triggers serialise vocabulary updates per owner with a transaction-level advisory
  lock (two-int key space, distinct from the per-hash locks), which rules out
  deadlocks between concurrent extractions and deletions of one owner's files.

### Thumbnails
- A `make_thumbnail` job keyed by **content hash** (files sharing bytes share one
  thumbnail), enqueued on image upload and reindex. Pure rendering lives in
  `akasha_ingest::thumbnail` (`image` crate): header first, refuse > 20 000 px per side
  or > 50 MP before decoding, decoder allocation cap, panics caught, EXIF orientation
  applied, 256 px JPEG (PNG when the image has alpha), never upscaled.
- Stored as derived objects `thumbs/ab/cd/<hash>/<size>` in the blob store, written
  under the per-hash lock after re-checking a file still references the blob, and
  deleted together with the blob by `delete_blob_if_unreferenced`.
- `GET /files/{id}/thumbnail`: owner-checked (404 otherwise), `ETag` = hash + size,
  `Cache-Control: private, max-age=604800`, `If-None-Match` → 304. PDFs, text and media
  get 404 (the client shows a type icon): rendering PDF pages needs a native renderer
  (pdfium/poppler), not worth it for previews now.

## Consequences
- Fusion or retrieval changes that hurt ranking fail `cargo test` unless the baseline
  is re-recorded on purpose; real-model quality drifts are caught nightly.
- The deterministic baseline measures plumbing, not semantic quality (paraphrase
  queries score low with the hash embedder); only the real-model tier says how good
  search is.
- Chunk inserts cost one extra tokenisation per chunk and serialise per owner at
  commit; fine for ingestion volumes, revisit if bulk imports get slow.
- Thumbnails of images uploaded before this change appear after a reindex.
