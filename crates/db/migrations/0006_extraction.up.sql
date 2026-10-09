-- Extracted text (one row per file) and its chunks (the unit of search and
-- citation). Both are replaced as a whole on every (re)extraction.

CREATE TABLE file_extractions (
    file_id           uuid PRIMARY KEY REFERENCES files(id) ON DELETE CASCADE,
    -- `text`, `markdown`, `csv`, `json`, `pdf`, `ocr` or `none` (nothing extractable).
    extractor         text NOT NULL,
    -- akasha-ingest version, to find extractions made by older code.
    extractor_version text NOT NULL,
    -- NULL for formats without pages.
    page_count        integer CHECK (page_count >= 0),
    -- Length of `text` in characters.
    char_count        integer NOT NULL CHECK (char_count >= 0),
    -- The whole normalised text; chunk and page offsets index into it.
    text              text NOT NULL,
    -- [{ "number", "char_start", "char_end", "source" }] for paged formats.
    pages             jsonb NOT NULL DEFAULT '[]',
    -- Remarks shown with the text (OCR disabled, truncated, not supported yet, ...).
    notes             text[] NOT NULL DEFAULT '{}',
    created_at        timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE file_chunks (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    file_id     uuid NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    -- Denormalised from files so searches filter by owner without a join.
    owner_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    chunk_index integer NOT NULL CHECK (chunk_index >= 0),
    -- 1-based PDF page; NULL for formats without pages.
    page        integer CHECK (page >= 1),
    -- Character offsets into file_extractions.text: [char_start, char_end).
    char_start  integer NOT NULL CHECK (char_start >= 0),
    char_end    integer NOT NULL CHECK (char_end >= char_start),
    text        text NOT NULL,
    -- 'english' like the legacy index (stemming); revisit with the search step.
    tsv         tsvector GENERATED ALWAYS AS (to_tsvector('english', text)) STORED,
    UNIQUE (file_id, chunk_index)
);

CREATE INDEX file_chunks_tsv_idx ON file_chunks USING gin (tsv);
CREATE INDEX file_chunks_owner_idx ON file_chunks (owner_id);
