-- Step 6: personal API tokens for machine clients (MCP, scripts), as ADR 0004
-- promised. Like sessions, only the SHA-256 of the token is stored. A token is
-- usable while it is neither revoked nor expired; revoked rows are kept so the
-- list can show them until the user deletes their account.
CREATE TABLE api_tokens (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id     uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name         text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    token_hash   bytea NOT NULL UNIQUE,
    -- The first characters of the token (e.g. `akasha_pat_AbCd`), to recognise it.
    prefix       text NOT NULL,
    scopes       text[] NOT NULL CHECK (
                     cardinality(scopes) >= 1 AND scopes <@ ARRAY['read', 'write']::text[]
                 ),
    created_at   timestamptz NOT NULL DEFAULT now(),
    last_used_at timestamptz,
    expires_at   timestamptz,
    revoked_at   timestamptz
);

CREATE INDEX api_tokens_owner_idx ON api_tokens (owner_id, created_at DESC);
