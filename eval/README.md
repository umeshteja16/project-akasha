# Search eval

A fixed benchmark for search quality (ADR 0011), run by `akasha eval`.

- `corpus/`: 32 short self-written documents (Markdown, text, CSV, JSON), one topic each.
  The three `*_eval.txt` files are the fixtures of the legacy benchmark.
- `queries.json`: 46 queries with the files that answer them, grouped by `kind`:
  `keyword` (20, shares the document's words), `paraphrase` (15, same meaning, other
  words), `multi` (5, several relevant files), `filename` (3), `legacy` (3, ported from
  `legacy/apps/api/benchmark_queries.json` / `eval-benchmark.json`).
- `baselines/<embed>+<rerank>.json`: the committed numbers a run is compared with.

## Running

```sh
cargo run -p akasha -- eval                     # deterministic tier (hash-384 + overlap)
cargo run --release -p akasha -- eval --real-models   # the configured AKASHA_EMBED_MODEL / AKASHA_RERANK_MODEL
cargo run -p akasha -- eval --update-baseline   # re-record after an intended ranking change
```

The corpus goes through the real pipeline (upload checks, extract and embed jobs) in a
scratch database created next to `DATABASE_URL` (needs `CREATEDB`) and dropped afterwards.
Each query runs in every mode; the report prints Recall@1/5/10, MRR, nDCG@10 (binary
relevance by file) and p50/p95 latency, and writes the full JSON (per query, per kind)
to `target/eval/<embed>+<rerank>.json`. Any quality metric more than `--tolerance`
(default 0.02) below the baseline fails the run; latency is never gated.

The deterministic tier also runs in `cargo test` (`crates/app/tests/eval.rs`), so every
CI run guards fusion and ranking. The real-model tier runs nightly and on demand in
`.github/workflows/eval.yml`.

## Baselines (2026-10-09)

Deterministic: `hash-384` + `overlap` (measures plumbing, not semantics):

| mode          | R@1   | R@5   | R@10  | MRR   | nDCG@10 |
|---------------|-------|-------|-------|-------|---------|
| keyword       | 0.565 | 0.565 | 0.565 | 0.565 | 0.565   |
| semantic      | 0.598 | 0.728 | 0.848 | 0.713 | 0.728   |
| hybrid        | 0.750 | 0.772 | 0.870 | 0.817 | 0.813   |
| hybrid_rerank | 0.793 | 0.935 | 0.978 | 0.883 | 0.903   |

Real model: `all-minilm-l6-v2`, no reranker (local run, release build):

| mode     | R@1   | R@5   | R@10  | MRR   | nDCG@10 | p50 ms |
|----------|-------|-------|-------|-------|---------|--------|
| keyword  | 0.565 | 0.565 | 0.565 | 0.565 | 0.565   | 9.4    |
| semantic | 0.902 | 1.000 | 1.000 | 0.973 | 0.980   | 11.1   |
| hybrid   | 0.924 | 1.000 | 1.000 | 0.989 | 0.992   | 20.1   |

Keyword search ANDs every query word, so long natural-language queries often find
nothing by keyword alone; that is why keyword trails the other modes.

The defaults (`multilingual-e5-small` + `jina-reranker-v1-turbo-en`) have no baseline
yet: record one from the first `eval.yml` artifact.

Adding corpus files or queries changes every number: re-record all baselines in the
same commit and update the tables above.
