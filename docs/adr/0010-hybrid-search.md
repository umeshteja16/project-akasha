# 0010: Hybrid search: Postgres FTS + pgvector, fused with RRF, in `crates/search`

- Status: accepted · 2026-10-09

## Context
Step 3 adds search over the chunks produced in step 2 (`file_chunks.tsv`, `embedding`).
It must serve two consumers: the search page (one entry per file with its best passages)
and grounded chat / agents (chunk-level passages with offsets for citations). It must
never return another user's chunks, must keep working when the embedding model is
missing (no ONNX Runtime, model still downloading), and should stay inside Postgres
(ADR 0002) with no extra search service.

## Decision
- **New crate `crates/search` (`akasha-search`)** owns the pipeline; the SQL lives in
  `akasha_db::search` (checked macros, like every other query). The crate is HTTP-free
  so chat (step 4) and the MCP server (step 6) call it directly.
- **Three retrievers**, each a ranked candidate list (≥ 50, more for deeper pages):
  keyword (`websearch_to_tsquery('english')` on `tsv`, `ts_rank_cd` normalised), file
  name (the same query against the name with punctuation as word breaks, represented by
  the file's first chunk) and semantic (query embedded with the model's query prefix,
  cosine distance `<=>`).
- **Reciprocal Rank Fusion** (k = 60) over the lists. RRF needs no score calibration
  between BM25-like and cosine scores and is robust to one list being empty. Modes:
  `keyword` (keyword + name), `semantic`, `hybrid` (default, all three).
- **Rerank** the top 30 fused chunks with the configured cross-encoder (if any); the rest
  keep their fused order, so paging stays stable. Failures only add a warning.
- **Owner isolation in SQL**: every query filters `file_chunks.owner_id` (denormalised)
  *and* joins `files` on `owner_id`. Filters (type, dates, tags, pinned, file ids) are
  SQL conditions; collections will be one more.
- **Vector scans**: owners with ≤ 10 000 embedded chunks, and file-restricted searches,
  get an exact scan (fast at that size, and immune to the HNSW post-filter problem
  where the owner filter discards most index candidates). Larger owners use HNSW with
  `SET LOCAL hnsw.ef_search` = max(limit, 100) and, on pgvector ≥ 0.8,
  `hnsw.iterative_scan = strict_order` (checked by version; 0.6 rejects the setting).
- **Graceful degradation**: the app hands the crate the models it has *now* (waiting at
  most 5 s for a load, which continues in the background). No embedder → keyword search
  with `degraded: true` and a warning, never a 500.
- **Snippets** come from `ts_headline` with private-use marker characters instead of
  HTML; Rust turns them into plain text plus highlight offsets (Unicode characters, the
  same unit as extraction offsets). Postgres does the stemming-aware matching.
- **Pagination** is offset-based over the fused list, capped at the first 200 results
  (RRF has no stable cursor; deeper pages are not useful for personal libraries).
- **Rate limit**: per user (governor keyed by user id), 30/min by default
  (`AKASHA_SEARCH_RATE_PER_MINUTE`), legacy parity.

## Consequences
- One Postgres round trip per retriever plus one to load rows; keyword and semantic run
  concurrently. Per-stage timings are returned for debugging.
- Keyword search is English-stemmed (legacy parity). Multilingual keyword search would
  need a second generated column (`simple` config) and is left for later; semantic
  search is multilingual already (e5).
- "Similar files" uses the mean of a file's stored vectors, so it works without loading
  a model.
