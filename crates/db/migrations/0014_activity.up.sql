-- Step 6b: the activity timeline and security audit log, one table.
--
-- Each row is something the owner did (or that happened to their account). It is
-- only ever shown to its owner. `category = 'security'` rows are the audit log
-- (sign-ins with IP and user agent, password changes, tokens, sessions, rate
-- limits). Rows older than `AKASHA_ACTIVITY_RETENTION_DAYS` are pruned daily.
-- Events without an account (a sign-in attempt for an unknown email, a per-IP
-- rate limit) go to the server log only.
CREATE TABLE activity_events (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id        uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind            text NOT NULL CHECK (kind ~ '^[a-z_]+\.[a-z_]+$'),
    category        text NOT NULL
                    CHECK (category IN ('files', 'search', 'chat', 'collections', 'security')),
    -- What it was about, as it was named then (file name, query, collection, token).
    subject         text CHECK (char_length(subject) <= 500),
    file_id         uuid,
    collection_id   uuid,
    conversation_id uuid REFERENCES conversations(id) ON DELETE SET NULL,
    details         jsonb NOT NULL DEFAULT '{}' CHECK (jsonb_typeof(details) = 'object'),
    -- `session` (the browser) or `token` (an API token or MCP client).
    via             text CHECK (via IN ('session', 'token')),
    ip              text CHECK (char_length(ip) <= 64),
    user_agent      text CHECK (char_length(user_agent) <= 256),
    -- clock_timestamp(): several events written in one transaction keep their order.
    created_at      timestamptz NOT NULL DEFAULT clock_timestamp(),
    -- Links point at the owner's own rows only; they go NULL when those are deleted
    -- (the event stays, named by `subject`).
    FOREIGN KEY (file_id, owner_id) REFERENCES files (id, owner_id)
        ON DELETE SET NULL (file_id),
    FOREIGN KEY (collection_id, owner_id) REFERENCES collections (id, owner_id)
        ON DELETE SET NULL (collection_id)
);

CREATE INDEX activity_owner_idx ON activity_events (owner_id, created_at DESC, id DESC);
CREATE INDEX activity_owner_category_idx
    ON activity_events (owner_id, category, created_at DESC, id DESC);
CREATE INDEX activity_created_idx ON activity_events (created_at);
-- ON DELETE SET NULL needs these to stay cheap.
CREATE INDEX activity_file_idx ON activity_events (file_id) WHERE file_id IS NOT NULL;
CREATE INDEX activity_collection_idx ON activity_events (collection_id)
    WHERE collection_id IS NOT NULL;
CREATE INDEX activity_conversation_idx ON activity_events (conversation_id)
    WHERE conversation_id IS NOT NULL;

-- Privacy: searches are recorded (with the query) unless the user turns it off.
ALTER TABLE users ADD COLUMN record_search_history boolean NOT NULL DEFAULT true;

-- "Signed in from ..." for the session list.
ALTER TABLE sessions ADD COLUMN ip text CHECK (char_length(ip) <= 64);
