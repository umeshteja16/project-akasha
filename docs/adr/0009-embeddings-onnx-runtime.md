# 0009: Embeddings and reranking: fastembed on a runtime-loaded ONNX Runtime

- Status: accepted · 2026-10-09

## Context
Step 2.6 adds chunk embeddings for semantic search (and the reranker step 3 will use),
in process as ADR 0003 decided. Constraints: CPU-only self-hosted boxes, `cargo build`
must work offline-ish on dev machines, in CI and in the distroless image, models must be
fetchable ahead of time for air-gapped installs, and vectors from two models must never
be mixed in one index.

## Decision
- **New crate `crates/ml` (`akasha-ml`)**: blocking `Embedder` and `Reranker` traits, a
  curated model catalog (config name → Hugging Face repo, ONNX file, pooling, dimension,
  query/document prefixes), our own downloader, and two deterministic built-ins:
  `hash-384` (hashed bag of words) and `overlap` (word-overlap reranker). Tests and
  model-less dev setups use those; they need no files and no native code.
- **fastembed 7 without its Hugging Face client**, models loaded through its
  "user-defined" constructors from files we download ourselves (`reqwest`, rustls, the
  same stack as the OCR downloader). Layout: `<AKASHA_MODELS_DIR>/hf/<owner>--<repo>/…`.
  `AKASHA_ML_MODELS_URL` (default `https://huggingface.co`, any HF mirror works) — empty
  means never download. Digests are not pinned (upstream repos are mutable and there are
  many models); integrity is TLS plus a length check, and writes are atomic. We use our
  own catalog because fastembed's has mistakes (e.g. its `MultilingualE5Base` repo).
- **ONNX Runtime is loaded at run time** (`ort` `load-dynamic`), not linked. `ort`'s
  default downloads prebuilt binaries from its CDN *at build time*, which fails behind
  restrictive proxies and couples the binary to that CDN. Instead the library is found
  via `AKASHA_ORT_DYLIB_PATH`, else `ORT_DYLIB_PATH`, else `libonnxruntime.so` on the
  loader path. The Docker image copies the pinned, SHA-256-checked official release
  (1.28.0, glibc, needs only libstdc++ which `distroless/cc` has) to
  `/usr/local/lib`. Dev machines run `just onnxruntime`. A missing library is a clear,
  retryable error, never a panic (we call `ort::init_from` ourselves first). The `onnx`
  cargo feature (on by default) gates all of this; without it only the built-ins exist.
- **Default embedding model: `multilingual-e5-small`** (384 d, ~100 languages, 12 small
  layers: fast on CPU; the ONNX file is ~470 MB because of its multilingual vocabulary).
  Uses `query: ` / `passage: ` prefixes. Alternatives in the catalog: `bge-small-en-v1.5`
  (384 d, English, smaller file, slightly better English retrieval), `bge-base-en-v1.5`
  and `nomic-embed-text-v1.5` (768 d, `search_query:`/`search_document:` prefixes),
  `bge-m3` (1024 d, best multilingual quality but ~2.3 GB and several times slower),
  `all-minilm-l6-v2` (tiny, for smoke tests). A personal library is often multilingual,
  so multilingual-by-default at small-model cost won.
- **Default reranker: `jina-reranker-v1-turbo-en`** (Apache-2.0, ~38 M params, fast).
  English-only; `bge-reranker-v2-m3` is the multilingual option (much slower), and
  `none` disables reranking. (jina-reranker-v2-multilingual is CC-BY-NC: not offered.)
- **Inference**: one model instance per process (lazy `OnceCell`, `Arc`, shared by the
  API and worker), serialised by a mutex (ONNX Runtime already parallelises one call over
  all cores, `AKASHA_ML_THREADS` caps it); called on the blocking pool; 64 chunks per
  database batch, 16 texts per ONNX call, inputs truncated to 512 tokens.
- **Schema: one fixed-dimension column** `file_chunks.embedding vector(384)` with an HNSW
  `vector_cosine_ops` index, and a single-row `embedding_model (name, dim)` table.
  - Startup (`serve`, `worker`) records the configured model on a fresh database and
    **refuses to start** when it differs from the recorded one or the column dimension.
  - The embed job checks the same before writing, and its `UPDATE` only writes while
    the recorded model matches, so a stale worker cannot mix vectors.
  - **`akasha reembed`** switches models: in one transaction it drops the index,
    `ALTER`s the column to the new dimension (`USING NULL`), recreates the index,
    records the model, sets files with chunks to `processing` and queues `embed_file`
    for each. This is the one schema change made outside migrations; later
    migrations must not assume the column is still 384-dimensional.
  - Rejected: one column per model, or an untyped `vector` column with per-dimension
    expression indexes. Both need app-created indexes anyway and make every query
    model-aware, for a rare operation.
- **Pipeline**: extraction enqueues `embed_file` in its storing transaction (files with
  no chunks go straight to `ready`); the handler embeds chunks with `embedding IS NULL`
  in batches (each committed, so retries resume), then marks the file `ready` under a
  row lock only if no chunk is left without a vector. Retryable failures fail the file
  only on the last attempt, like extraction.
- **`akasha models download`** fetches OCR, embedding and rerank models into
  `AKASHA_MODELS_DIR` (copy the directory to air-gapped hosts and set the URLs empty);
  `akasha models check` loads both models and runs one inference (CI runs it against the
  built image).

## Consequences
- Builds never download native code; the binary links only glibc. Running real models
  needs ONNX Runtime ≥ 1.24 on the host (the image has it).
- Switching embedding models costs a full re-embed, made explicit and safe.
- `fastembed` brings `tokenizers` (with the `onig` C library, built by `cc`) and the
  unmaintained `paste` proc-macro (ignored in `deny.toml`).
- Chunking stays character-based (2000 chars ≈ 400–500 tokens), under the 512-token
  window; a token-based splitter would need `tokenizers` in `crates/ingest` too.
