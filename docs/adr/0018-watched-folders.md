# 0018: Watched folders (sources)

- Status: accepted · 2026-10-10

## Context
People keep notes and documents in folders that other tools write: an Obsidian vault, a
Documents or Downloads directory, a scanner's output folder. Uploading them by hand and
again after every edit does not work. The server (often in a container) can see such
folders when they are mounted into it, but in a multi-user install a user must never be
able to read arbitrary server paths, and a scan must never follow a symlink out of the
folder it was allowed to read.

## Decision
- **Admin allow-list**: `AKASHA_WATCH_ROOTS` (comma-separated, so paths may contain
  spaces). Empty = feature off. A root may contain `{user_id}` or `{email}` to give each
  user a subtree (`/data/users/{email}`). Roots are canonicalised; a requested folder is
  canonicalised (`..` and symlinks resolved) and must be inside a root component-wise
  (`/data/notes2` is not inside `/data/notes`). Missing paths outside the roots get the
  same 403 as existing ones (no probing). Overlapping folders of one owner are refused.
- **Sources** (`sources` table, migration 0016): owner, kind `folder`, canonical path,
  include/exclude globs, `on_delete` (`delete` | `keep`), `import_tags`, `enabled`
  (pause), status/last error/last scan counts and a scan lease. **`source_files`** maps
  each relative path to size, mtime, content hash and the imported file, with
  `created_file` (this source added the file, versus the owner already having the same
  bytes) and `skip_reason`. Like collections, it references source and file by
  `(id, owner_id)`, so a cross-owner mapping cannot exist; a file deleted in Akasha leaves
  the row with `file_id NULL`, and it stays out until it changes on disk.
- **Sync is a job** (`scan_source`, dedupe per source), queued on create/resume/glob
  change, by "Rescan now", by a periodic `scan_all_sources` (`AKASHA_WATCH_SCAN_MINUTES`,
  default 15) and by filesystem events (`notify`, debounced 3 s quiet / 30 s max, hidden
  folders ignored, in every worker process). Events are only hints; the periodic scan is
  the source of truth (inotify limits, network filesystems).
- **A scan** re-checks the source path against the owner's current roots, lists the
  folder with `walkdir` (never following symlinks; hidden entries and unsupported
  extensions skipped; at most 200k files), and imports files whose size or mtime differ
  from their row, up to 200 per run (60 s) with 4 at a time; a follow-up run continues
  (counts carried in the payload). Files modified in the last 2 s are left for the next
  scan. Unchanged files cost one `stat`.
- **Imports reuse the upload pipeline**: the same streaming receiver (size limit, sniffed
  type allow-list, UTF-8 text) into staging, then one transaction stores the file (dedupe
  by hash per owner; quota), queues extraction/thumbnail and writes the `source_files`
  row. Changed content replaces the bytes of the same file (`files::replace_content`:
  id, name, tags, pins and collections survive; old blob released by job). Files are opened
  with `O_NOFOLLOW`, and the opened inode must be the one at the canonical path inside the
  root (defeats directory symlink swaps mid-scan). Markdown front-matter `tags:` merge into
  the file's tags.
- **Deletions** only after a complete pass with no unreadable entries and nothing left
  to import: a vanished path whose file another row maps (a rename) hands the file over
  and renames it; otherwise the file is deleted or kept per `on_delete`. A folder that
  lists empty while files are known is treated as an unmounted volume: nothing is
  deleted and the source shows an error.
- **Removing a source** keeps its files by default; `delete_files=true` deletes only files
  the source created and no other source maps.

## Consequences
- The server process needs read access to the mounted folders; Akasha never writes them
  (mount read-only).
- Hard links inside a root to files outside it cannot be told apart from regular files;
  admins should not mount trees where untrusted users can create hard links to secrets.
- One activity event per scan that changed something (`source.synced`), not one per file.
- Concurrency per source is one scan (lease, 15 min takeover); a scan arriving meanwhile
  re-queues itself 15 s later.
