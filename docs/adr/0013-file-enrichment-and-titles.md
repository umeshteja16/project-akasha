# 0013: Model-written file summaries, suggested tags and conversation titles

- Status: accepted · 2026-10-09

## Context
The legacy worker asked Gemini for a one-sentence summary and up to three tags per file
(API key in the URL, 15k characters of input, tags written straight into the file's only
tag list) and fell back to "first two sentences" offline. Step 4 already has a provider
layer (ADR 0012), so the rewrite reuses it, but user-set tags must never be overwritten
and nothing may leave the machine in strict offline mode.

## Decision
- **Job `enrich_file { file_id, force }`**, enqueued by `embed_file` in the transaction
  that moves a file `processing → ready`, only when a chat model is configured and
  `AKASHA_LLM_ENRICH_FILES` (default on). `POST /api/v1/files/{id}/enrich` queues a forced
  run (owner-checked, file must be `ready`, 503 without a model, 10 per user per minute).
- **Input**: the first 8000 characters of the extracted text plus up to three evenly spaced
  later chunks (first chunk of each third, 1000 characters each), so long documents are
  described by more than their introduction at a bounded cost (~3k tokens).
- **Output**: one JSON object `{summary, tags}`. `ChatRequest.json` turns on the provider's
  JSON mode where every server honours it (Ollama `format: "json"`, Gemini
  `responseMimeType`); Anthropic and OpenAI-compatible servers rely on the prompt.
  Parsing is defensive (code fences, prose around the object, tags as one string); the
  summary is cut to three sentences / 600 characters; tags are lowercased, trimmed of
  `#`/punctuation, de-duplicated, ≤ 40 characters / 4 words, at most 5. Nothing usable →
  retry (3 attempts), then `enrichment_status = failed`.
- **Storage (migration 0010)**: `files.summary`, `files.auto_tags` (separate from the
  user's `tags`, which enrichment never writes), `enrichment_status`
  (`done|skipped|failed`), `enrichment_model`, `enriched_from` (the
  `file_extractions.created_at` described) and `enriched_at`. The API shows `tags` and
  `auto_tags` (minus any already in `tags`) separately; tag filters on the file list,
  search and chat match either. Users drop wrong suggestions with `PATCH auto_tags` and
  keep one for good by adding it to `tags`; re-running enrichment replaces `auto_tags` only.
- **Idempotency**: skip when `enriched_from` equals the current extraction and the status
  is `done`/`skipped` (unless forced); results are written with
  `WHERE EXISTS (extraction with created_at = enriched_from)`, so a slow model call never
  attaches an old summary to re-extracted text. Failures never change the file's `status`.
- **Conversation titles**: job `title_conversation`, queued with the first `answered`
  reply (same transaction) when a model is configured and `AKASHA_LLM_CONVERSATION_TITLES`
  is on. `conversations.title_source` (`user|question|model`) makes the rule explicit: only
  a `question` title (the shortened first question) is replaced, so a rename always wins.
- Strict offline needs no special case: `llm::build` only yields local providers there,
  and without a model neither job is queued (a job that runs anyway does nothing).
- `akasha eval` runs without a language model (it measures retrieval only).

## Consequences
- One extra short model call per indexed file and per conversation; both can be turned off.
- Reindexing a file re-runs enrichment (new extraction timestamp) even if the text did not
  change; acceptable since reindex is explicit.
- Summaries are searchable only through the file response for now, not indexed for FTS.
