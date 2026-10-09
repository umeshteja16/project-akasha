# Search eval

A fixed benchmark for search quality (ADR 0011), run by `akasha eval`.

- `corpus/`: 32 short self-written documents (Markdown, text, CSV, JSON), one topic each.
  The three `*_eval.txt` files are the fixtures of the legacy benchmark.
- `queries.json`: 56 queries with the files that answer them, grouped by `kind`:
  `keyword` (20, shares the document's words), `paraphrase` (15, same meaning, other
  words), `multi` (5, several relevant files), `filename` (3), `legacy` (3, ported from
  `legacy/apps/api/benchmark_queries.json` / `eval-benchmark.json`), `precision` (4, short
  queries where unrelated files must not show up) and `negative` (6, about nothing in the
  corpus: a good search returns nothing; they list no relevant files).
- `gate.json`: 18 questions the corpus answers and 12 it does not, for the chat refusal
  gate (below).
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
relevance by file), Precision@10 (share of the returned files that are relevant; search
hides loosely related results, ADR 0014), `neg ok` (share of `negative` queries that
returned nothing) and p50/p95 latency, and writes the full JSON (per query, per kind)
to `target/eval/<embed>+<rerank>.json`. Any quality metric more than `--tolerance`
(default 0.02) below the baseline fails the run; latency is never gated.

The deterministic tier also runs in `cargo test` (`crates/app/tests/eval.rs`), so every
CI run guards fusion and ranking. The real-model tier runs nightly and on demand in
`.github/workflows/eval.yml`.

## Baselines (2026-10-09, with the relevance floor of ADR 0014)

Deterministic: `hash-384` + `overlap` (measures plumbing, not semantics). The relevance
floor (hash 0.15 cosine, overlap 0.25) trades a little recall for much less noise
(Precision@10 was ~0.10 in every semantic mode before it):

| mode          | R@1   | R@5   | R@10  | MRR   | nDCG@10 | P@10  | neg ok |
|---------------|-------|-------|-------|-------|---------|-------|--------|
| keyword       | 0.600 | 0.600 | 0.600 | 0.600 | 0.600   | 0.600 | 1.000  |
| semantic      | 0.530 | 0.570 | 0.630 | 0.599 | 0.589   | 0.417 | 0.333  |
| hybrid        | 0.770 | 0.790 | 0.830 | 0.827 | 0.811   | 0.571 | 0.333  |
| hybrid_rerank | 0.870 | 0.960 | 0.960 | 0.931 | 0.935   | 0.752 | 0.833  |

Real model: `all-minilm-l6-v2`, no reranker, floor 0.25 (local run, debug build):

| mode     | R@1   | R@5   | R@10  | MRR   | nDCG@10 | P@10  | neg ok | p50 ms |
|----------|-------|-------|-------|-------|---------|-------|--------|--------|
| keyword  | 0.600 | 0.600 | 0.600 | 0.600 | 0.600   | 0.600 | 1.000  | 9.4    |
| semantic | 0.890 | 0.960 | 0.960 | 0.950 | 0.953   | 0.853 | 1.000  | 15.5   |
| hybrid   | 0.930 | 1.000 | 1.000 | 0.990 | 0.993   | 0.883 | 1.000  | 23.7   |

Without the floor the same run had hybrid Precision@10 0.110 and returned files for every
off-topic query; 0.30 starts to cost recall (hybrid R@10 0.99), 0.35 clearly (0.94).
To tune a floor for another model, run the real-model eval with
`AKASHA_SEARCH_MIN_SIMILARITY` / `AKASHA_SEARCH_MIN_RERANK_SCORE` set to a few values and
put the best into the model's catalog entry (`crates/ml/src/catalog.rs`).

Keyword search ANDs every query word, so long natural-language queries often find
nothing by keyword alone; that is why keyword trails the other modes.

The defaults (`multilingual-e5-small` + `jina-reranker-v1-turbo-en`) have no baseline
yet: record one from the first `eval.yml` artifact.

## Refusal-gate calibration

With a reranker, every run also scores the `gate.json` questions the way chat retrieves
(hybrid, rerank, top 20) and prints the top reranker score distribution of answerable vs
unanswerable questions, the accuracy of the current threshold
(`AKASHA_CHAT_MIN_RERANK_SCORE`, else the per-reranker default in `chat::evidence`) and
the lowest threshold with the best accuracy on this set (`gate` in the JSON report; never
gated, not in baselines). Deterministic tier (`overlap`, 2026-10-09):

| questions    | n  | min   | p10   | median | p90   | max   |
|--------------|----|-------|-------|--------|-------|-------|
| answerable   | 18 | 0.333 | 0.400 | 0.800  | 1.000 | 1.000 |
| unanswerable | 12 | 0.000 | 0.000 | 0.250  | 0.500 | 0.500 |

Threshold 0.5: accuracy 0.867; best on this set 0.367 (0.900). `overlap` is lexical, so this
only checks the plumbing. For a real cross-encoder: run `akasha eval --real-models` with
`AKASHA_RERANK_MODEL` set (or the `eval.yml` artifact), pick a threshold a little below the
suggested one (refusing a real question is worse than a weak answer, which the model can
still decline), set it as the default for that model in `chat::evidence::default_min_score`
and note the numbers here.

Adding corpus files or queries changes every number: re-record all baselines in the
same commit and update the tables above.
