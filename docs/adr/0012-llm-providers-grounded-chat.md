# 0012: LLM providers and grounded chat

- Status: accepted · 2026-10-09

## Context
Step 4 adds grounded chat (legacy `chat.routes.ts`): answer questions from the user's own
files with citations, in conversations. ADR 0003 keeps generative models out of process
behind a provider trait, with Ollama as the offline option. Legacy only spoke to Gemini
(with the API key in the URL query string) and fell back to a "local synthesizer" that
stitched sentences together.

## Decision

### Provider layer: new crate `crates/llm` (`akasha-llm`)
- A separate crate rather than `crates/ml`: `ml` is blocking, in-process ONNX inference;
  chat models are async network clients with different dependencies (streaming `reqwest`,
  SSE/NDJSON parsing) and failure modes (retries, timeouts, keys). Keeping them apart keeps
  `ml` free of HTTP-client concerns and lets each be tested on its own.
- One trait, `ChatModel`: `stream(&ChatRequest) -> ChatStream` (token deltas, then `Done`
  with stop reason and token usage) and a provided `complete` that collects it. Cancelling
  is dropping the stream, which closes the HTTP connection.
- Thin `reqwest` (rustls) implementations, no vendor SDKs: Ollama `/api/chat` (NDJSON),
  Anthropic Messages API (SSE, `x-api-key`, `anthropic-version: 2023-06-01`), Gemini
  `streamGenerateContent?alt=sse` (key in the `x-goog-api-key` header, never in a URL),
  OpenAI-compatible `/chat/completions` (SSE; covers vLLM, LM Studio, llama.cpp server,
  OpenRouter). A shared line splitter and SSE parser handle chunks split anywhere,
  including inside UTF-8 characters.
- Anthropic: default model `claude-opus-5-5`; current Claude models reject sampling
  parameters, so `temperature` is never sent; `output_config.effort` defaults to `low`
  (grounded Q&A needs little thinking); thinking deltas are ignored; models that support
  it opt into server-side refusal fallbacks (`fallbacks: "default"`). A `refusal` stop
  reason becomes a refused answer.
- Retries with exponential backoff and jitter (or `Retry-After`) on connection errors,
  429 and 5xx, only before output starts. Connect timeout plus a read (silence) timeout,
  no overall deadline, so long answers can stream.
- `AKASHA_STRICT_OFFLINE=true` refuses to start with Anthropic or Gemini, or with an
  Ollama/OpenAI-compatible URL that is not loopback, private, link-local, `*.local`-style or
  a single-label (Docker service) host. API keys are redacted in `Debug` and never logged.
- A deterministic `FakeChatModel` (`AKASHA_LLM_PROVIDER=fake`) cites every numbered source
  it is given; tests use it and test providers against local mock servers.

### Grounded chat (`crates/app/src/chat`, `routes/chat`)
- Tables `conversations` and `messages` (migration 0009), owner-scoped with cascades;
  messages store status (`answered`, `refused`, `no_llm`, `cancelled`, `error`), citations
  (jsonb), model, token usage and latency.
- A question is stored, then answered in a spawned task that feeds the SSE response
  through a channel: `sources` (numbered passages), `delta`s, then `done` (stored id,
  status, final text, citations, usage) or `error`. When the client disconnects the channel
  closes, generation stops and the partial answer is stored as `cancelled`.
- Retrieval uses `akasha_search::search_chunks` (hybrid + rerank, top 20, optional
  `file_ids`/tags/type scope); up to `AKASHA_CHAT_CONTEXT_CHUNKS` passages, at most three per
  file, become sources. Follow-ups are rewritten into a standalone query by the model
  (falls back to "previous question + question"); the last few turns go to the model too.
- **Refusal gate**: only a cross-encoder score says how well a passage answers a question,
  so the gate compares the top reranker score with a per-reranker threshold
  (`AKASHA_CHAT_MIN_RERANK_SCORE` overrides). Below it, the fixed "I couldn't find this in
  your files." is returned without calling the model. Without a reranker score the model
  decides (it is told to give the same sentence). The `overlap` test reranker now ignores
  stopwords and plural `s` so its scores separate answerable from off-topic questions
  (calibrated in `tests/chat_gate.rs`: ≥ 0.75 vs ≤ 0.25, threshold 0.5); this also improved
  the deterministic eval, whose baseline was re-recorded. The ONNX cross-encoder default
  (-3, on logits) is a lenient guess until real-model data exists.
- No model configured: the sources are returned with status `no_llm` (never a 500).
- Citations are parsed from `[n]` / `[1, 2]` markers; numbers without a source are dropped.
- Conversation titles are the first question, shortened (no model call).
- Per-user rate limit (`AKASHA_CHAT_RATE_PER_MINUTE`, default 20, legacy parity).

## Consequences
- Offline installs work with Ollama alone; cloud providers are opt-in and blockable.
- Answers can only cite what retrieval found; answer quality depends on the search
  pipeline and the configured model. The real-reranker refusal threshold still needs
  calibration against `akasha eval --real-models` data with unanswerable questions.
- Generated titles and per-file summaries (step 4.4) can reuse `ChatModel::complete`.
