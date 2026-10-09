-- Step 4.4: model-written file summaries and tags, and conversation titles.

ALTER TABLE files
    -- A short description written by the language model (NULL: none yet).
    ADD COLUMN summary text CHECK (char_length(summary) <= 1000),
    -- Tags suggested by the language model. Kept apart from `tags`, which are the
    -- user's own and are never written by enrichment.
    ADD COLUMN auto_tags text[] NOT NULL DEFAULT '{}',
    -- NULL: never enriched; done; skipped (nothing to describe); failed.
    ADD COLUMN enrichment_status text
        CHECK (enrichment_status IN ('done', 'skipped', 'failed')),
    -- `provider/model` that wrote summary and auto_tags.
    ADD COLUMN enrichment_model text,
    -- `file_extractions.created_at` of the text that was described: a newer
    -- extraction makes the summary stale (and queues a new enrichment).
    ADD COLUMN enriched_from timestamptz,
    ADD COLUMN enriched_at timestamptz;

CREATE INDEX files_auto_tags_idx ON files USING gin (auto_tags);

-- Who named a conversation: the user, the first question (shortened) or the
-- model. Only a `question` title is ever replaced by a model-written one.
ALTER TABLE conversations
    ADD COLUMN title_source text NOT NULL DEFAULT 'user'
        CHECK (title_source IN ('user', 'question', 'model'));
