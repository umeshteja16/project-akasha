-- Uploaded files. The bytes live in content-addressed storage under `content_hash`;
-- blobs are shared across users and deleted only when no row references them.

-- NULL = unlimited. Counted as the sum of `files.size_bytes` the user owns.
ALTER TABLE users ADD COLUMN storage_quota_bytes bigint
    CHECK (storage_quota_bytes IS NULL OR storage_quota_bytes >= 0);

CREATE TABLE files (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id      uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    original_name text NOT NULL CHECK (length(original_name) BETWEEN 1 AND 255),
    content_hash  text NOT NULL CHECK (content_hash ~ '^[0-9a-f]{64}$'),
    mime_type     text NOT NULL,
    size_bytes    bigint NOT NULL CHECK (size_bytes >= 0),
    status        text NOT NULL DEFAULT 'pending'
                  CHECK (status IN ('pending', 'processing', 'ready', 'failed')),
    error         text,
    is_pinned     boolean NOT NULL DEFAULT false,
    tags          text[] NOT NULL DEFAULT '{}',
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    -- Re-uploading the same bytes returns the existing file.
    UNIQUE (owner_id, content_hash)
);

CREATE TRIGGER files_updated_at BEFORE UPDATE ON files
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE INDEX files_owner_created_idx ON files (owner_id, created_at DESC, id DESC);
-- Blob reference checks on delete.
CREATE INDEX files_content_hash_idx ON files (content_hash);
CREATE INDEX files_tags_idx ON files USING gin (tags);
