-- Step 6b: collections (named groups of files, many-to-many) and open tracking.
--
-- Ownership is enforced by the schema: `collection_files` carries the owner and
-- references both sides by `(id, owner_id)`, so a collection can only ever hold
-- its owner's files, whatever the application code does.

ALTER TABLE files ADD CONSTRAINT files_id_owner_key UNIQUE (id, owner_id);

CREATE TABLE collections (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name        text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    description text NOT NULL DEFAULT '' CHECK (char_length(description) <= 2000),
    -- Names from the UI's palette and icon set (never raw colours).
    color       text NOT NULL DEFAULT 'sage'
                CHECK (color IN ('sage', 'sky', 'ochre', 'clay', 'plum', 'slate')),
    icon        text NOT NULL DEFAULT 'folder'
                CHECK (icon IN ('folder', 'book', 'briefcase', 'flask', 'heart', 'star',
                                'archive', 'receipt', 'plane', 'home', 'graduation', 'code')),
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (id, owner_id)
);

-- One name per owner, case-insensitively.
CREATE UNIQUE INDEX collections_owner_name_key ON collections (owner_id, lower(name));

CREATE TRIGGER collections_updated_at BEFORE UPDATE ON collections
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TABLE collection_files (
    collection_id uuid NOT NULL,
    file_id       uuid NOT NULL,
    owner_id      uuid NOT NULL,
    added_at      timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (collection_id, file_id),
    FOREIGN KEY (collection_id, owner_id) REFERENCES collections (id, owner_id) ON DELETE CASCADE,
    FOREIGN KEY (file_id, owner_id) REFERENCES files (id, owner_id) ON DELETE CASCADE
);

CREATE INDEX collection_files_file_idx ON collection_files (file_id);

-- A conversation may answer from one collection (checked against the owner on write).
ALTER TABLE conversations
    ADD COLUMN collection_id uuid REFERENCES collections(id) ON DELETE SET NULL;

-- Open tracking (legacy `PATCH /files/:id/open`). Opening a file is not an edit, so
-- it must not bump `updated_at`: the trigger now skips updates that change these.
ALTER TABLE files
    ADD COLUMN last_opened_at timestamptz,
    ADD COLUMN open_count integer NOT NULL DEFAULT 0 CHECK (open_count >= 0);

DROP TRIGGER files_updated_at ON files;
CREATE TRIGGER files_updated_at BEFORE UPDATE ON files
FOR EACH ROW WHEN (OLD.last_opened_at IS NOT DISTINCT FROM NEW.last_opened_at)
EXECUTE FUNCTION set_updated_at();

CREATE INDEX files_owner_opened_idx ON files (owner_id, last_opened_at DESC, id DESC)
    WHERE last_opened_at IS NOT NULL;
