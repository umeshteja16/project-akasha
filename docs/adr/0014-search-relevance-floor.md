# 0014: A relevance floor for results found by meaning alone

- Status: accepted · 2026-10-09

## Context
Hybrid search (ADR 0010) fuses keyword, file-name and vector results. A k-nearest-neighbour
query always returns neighbours, however unrelated, so in a small library every file came
back for every query: "lunar module eagle" listed all seven files, including a bird log and
a budget CSV, ranked only below the moon-landing notes. RRF scores are rank-based and look
the same for any query, so they cannot tell a match from noise. The eval only measured
recall and ranking (Recall@k, MRR, nDCG), which noise does not hurt.

## Decision
- A result that **only the semantic retriever found** must clear a threshold to count as a
  match; **keyword and file-name hits always qualify** (the words are really there).
  - When it was reranked and the reranker has a floor: its **reranker score**
    (cross-encoders judge query–passage relevance far better than cosine similarity).
  - Otherwise: its **cosine similarity** to the query.
- Thresholds are **per model**, in the catalog (`EmbedModel::min_similarity`,
  `RerankModel::min_score`), because models spread scores very differently (e5 puts almost
  everything above 0.7, MiniLM unrelated text near 0). `AKASHA_SEARCH_MIN_SIMILARITY` and
  `AKASHA_SEARCH_MIN_RERANK_SCORE` override them (`RelevanceFloor` in `Models`).
- Results below the floor are **loosely related**: hidden by default and counted in
  `loosely_related` on the response; `include_weak=true` returns them after the matches
  (each hit carries `loosely_related: true`). The UI offers them in a collapsed
  "Loosely related" section.
- Grounded chat retrieves without loosely related passages (they only distract the model);
  the refusal gate (ADR 0012) still judges the best remaining passage, and with nothing left
  chat answers "not found" without calling the model. The gate calibration in `akasha eval`
  keeps them, to see the reranker's raw distribution.
- The eval measures precision: **Precision@10** (share of returned files that are
  relevant) per query and **negative_clean** (share of off-topic `negative` queries that
  return nothing). Both are gated like the other metrics.

## Defaults and calibration (2026-10-09)
- `all-minilm-l6-v2`: 0.25, measured with `akasha eval --real-models` (no reranker):
  hybrid Recall@10 stays 1.000, Precision@10 0.110 → 0.883, negative_clean 0 → 1.0;
  0.30 starts losing recall (0.99), 0.35 clearly (0.94).
- `hash-384` 0.15 and `overlap` 0.25 (test models): chosen on the deterministic eval;
  `overlap` 0.25 keeps paraphrase questions that share one of three or four content words.
- Not measured (models cannot be downloaded in the dev sandbox), deliberately lenient:
  `multilingual-e5-small` 0.80, `bge-*-en-v1.5` 0.55, `nomic-embed-text-v1.5` and `bge-m3`
  0.45, ONNX cross-encoders -2.0 (logit; sigmoid ≈ 0.12). The nightly real-model eval now
  reports precision and negative_clean, so these can be tuned from its artifact.

## Consequences
- Searches return fewer, better results; "no results" is now a real answer for off-topic
  queries. Recall on paraphrases can drop when a model's floor is too strict: lower it, or
  use `include_weak`.
- The deterministic baseline was re-recorded (intended change): hybrid Recall@10 0.88 →
  0.83, hybrid_rerank 0.98 → 0.96, while Precision@10 rose from ~0.10 to 0.57 / 0.75.
- Semantic-only mode is subject to the floor for every result.
