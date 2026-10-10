# 0016: Collections, the activity timeline and the security audit log

- Status: accepted · 2026-10-10

## Context
The legacy app had collections (one per file, `files.collection_id`), an `audit_log`
table written best-effort for a few file actions, an activity screen built on it, and
open tracking (`PATCH /files/:id/open`). Step 6 brings these back, with three
requirements the legacy code did not meet: strict owner isolation, a security log the
user can actually read (sign-ins with address and browser, sessions they can revoke), and
privacy for what people search for.

## Decision
- **Collections are many-to-many** (`collections` + `collection_files`, migration 0013): a
  file can be in several (a tax PDF in "Taxes 2025" and "House"). Names are unique per
  owner ignoring case. Colour and icon are names from a small UI palette (`sage`, `sky`,
  ... / `folder`, `book`, ...), checked by the database, never raw colours (DESIGN.md).
  Deleting a collection keeps its files.
- **Ownership in the schema**: `collection_files` carries `owner_id` and references
  `collections (id, owner_id)` and `files (id, owner_id)`; a cross-owner row cannot exist,
  whatever a handler does. Every query still filters by owner; other users' collections
  are 404, also as `collection_id` filters on `/files`, `/search` and chat.
- The search seam in `ChunkFilter` is filled: `collection_id` adds an `EXISTS` on
  `collection_files` (owner-checked) to the keyword, file-name and vector queries, and a
  collection-scoped vector search scans exactly (like `file_ids`). Conversations can store
  a `collection_id` scope (combined with `file_ids`); a question may override it.
- **One `activity_events` table** (migration 0014) holds both the timeline and the audit
  log; `category = 'security'` is the audit log. Separate tables would have duplicated
  the listing, pagination, retention and privacy rules for no gain; the category keeps
  them apart in the UI and in "clear history", which never deletes security rows.
  `kind` is `area.verb` (`file.renamed`, `auth.signed_in`), mirrored by the
  `ActivityKind` enum (one place lists kind → category). Rows keep a `subject` snapshot
  (file name, query, collection or token name) and nullable links that go `NULL` when the
  file, collection or conversation is deleted (`ON DELETE SET NULL (col)` on composite
  `(id, owner_id)` keys, so links are owner-checked too). `created_at` uses
  `clock_timestamp()` so events written in one transaction keep their order.
- **Write path**: inside the action's transaction where there is one (upload, rename/tag,
  delete, collection changes, token create/revoke, questions), so an event exists exactly
  when the change committed; best-effort otherwise (sign-ins, sign-outs, session
  revocation, password changes). Failed sign-ins for an existing account are written in a
  background task so the response time does not reveal which emails have accounts.
  Repetitive events are bounded: a file "opened" at most once per 30 minutes, a
  rate-limit hit once per 10 minutes, and as-you-type searches refine one entry (a newer
  query that extends or shortens the previous one within two minutes updates it).
- **Privacy**: events are listed only to their owner, and only with the browser session
  (`SessionUser`): API tokens and MCP clients cannot read the timeline, the security log
  or the session list. Search history (with query text) is on by default and can be
  turned off in Settings (`users.record_search_history`); then searches are not recorded
  at all. Users can clear their history (not the security log). Account deletion removes
  everything, so it is audited in the server log only; events without an account (unknown
  email, per-IP rate limits) also go to the server log (`target: "audit"`) only.
- **Retention**: a daily `prune_activity` job deletes events older than
  `AKASHA_ACTIVITY_RETENTION_DAYS` (default 365, 0 keeps them), in batches.
- **Sessions** gain the sign-in address (`sessions.ip`); `GET /me/sessions` lists them
  with the current one marked, `DELETE /me/sessions/{id}` revokes one, and
  `POST /me/sessions/revoke-others` the rest.
- **Open tracking**: `files.last_opened_at` / `open_count`, set by `POST /files/{id}/open`
  (the UI calls it when a file page opens). The `files` `updated_at` trigger skips updates
  that change `last_opened_at`, so opening is not an edit. `sort=opened` lists opened files,
  most recent first.

## Consequences
- The client address is the TCP peer: behind a reverse proxy every event shows the
  proxy's address until the planned `trust_proxy` option exists.
- Timeline volume is bounded per action, but a heavy API user still writes one row per
  upload/rename; retention keeps the table finite.
- Adding an event kind means a variant in `ActivityKind` (and a label in the UI); unknown
  kinds stored by a newer version are skipped by older ones when listing.
