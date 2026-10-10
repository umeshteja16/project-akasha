-- Step 6: watched folders ("sources"). A source is a directory on the server, under one
-- of the admin's AKASHA_WATCH_ROOTS, whose files are imported for its owner and kept in
-- sync by the `scan_source` job.
--
-- Ownership is enforced by the schema like collections: `source_files` carries the
-- owner and references both its source and its file by `(id, owner_id)`.

CREATE TABLE sources (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id        uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind            text NOT NULL DEFAULT 'folder' CHECK (kind IN ('folder')),
    name            text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    -- Canonical absolute path (symlinks resolved) when the source was added.
    path            text NOT NULL CHECK (char_length(path) BETWEEN 1 AND 4096),
    -- Relative-path globs; no includes = every supported file.
    include_globs   text[] NOT NULL DEFAULT '{}',
    exclude_globs   text[] NOT NULL DEFAULT '{}',
    -- What happens to an imported file when it disappears from the folder:
    -- `delete` it from Akasha too, or `keep` it (detached from the source).
    on_delete       text NOT NULL DEFAULT 'delete' CHECK (on_delete IN ('delete', 'keep')),
    -- Markdown front-matter `tags:` become the file's tags (Obsidian).
    import_tags     boolean NOT NULL DEFAULT true,
    -- false = paused: no scans, no watching.
    enabled         boolean NOT NULL DEFAULT true,
    status          text NOT NULL DEFAULT 'pending'
                    CHECK (status IN ('pending', 'scanning', 'ok', 'error')),
    last_error      text,
    -- Lease of the running scan (a crashed scan is taken over after a while).
    scan_started_at timestamptz,
    last_scan_at    timestamptz,
    -- Counts from the last finished scan (files, imported, updated, removed, skipped).
    last_scan       jsonb NOT NULL DEFAULT '{}',
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    UNIQUE (id, owner_id),
    UNIQUE (owner_id, path)
);

CREATE TRIGGER sources_updated_at BEFORE UPDATE ON sources
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- One row per file seen in a source. `file_id` NULL with `skip_reason` NULL: the owner
-- deleted the imported file in Akasha; it stays out until it changes on disk.
CREATE TABLE source_files (
    source_id    uuid NOT NULL,
    owner_id     uuid NOT NULL,
    -- Path relative to the source, `/`-separated.
    rel_path     text NOT NULL CHECK (char_length(rel_path) BETWEEN 1 AND 4096),
    size_bytes   bigint NOT NULL CHECK (size_bytes >= 0),
    mtime        timestamptz NOT NULL,
    content_hash text CHECK (content_hash ~ '^[0-9a-f]{64}$'),
    file_id      uuid,
    -- This source added the file (so removing the source may delete it). False when
    -- the owner already had the same bytes.
    created_file boolean NOT NULL DEFAULT false,
    -- Why the file was not imported (too large, unsupported content).
    skip_reason  text,
    synced_at    timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (source_id, rel_path),
    FOREIGN KEY (source_id, owner_id) REFERENCES sources (id, owner_id) ON DELETE CASCADE,
    FOREIGN KEY (file_id, owner_id) REFERENCES files (id, owner_id) ON DELETE SET NULL (file_id)
);

CREATE INDEX source_files_file_idx ON source_files (file_id) WHERE file_id IS NOT NULL;
