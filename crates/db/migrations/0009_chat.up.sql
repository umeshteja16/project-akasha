-- Grounded chat (step 4): conversations and their messages. Everything is owned
-- by one user and disappears with the account.

CREATE TABLE conversations (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id   uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- Empty until the first question names it (or the user does).
    title      text NOT NULL DEFAULT '' CHECK (char_length(title) <= 200),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TRIGGER conversations_updated_at BEFORE UPDATE ON conversations
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Listing: most recently active first, keyset-paged.
CREATE INDEX conversations_owner_updated_idx ON conversations (owner_id, updated_at DESC, id DESC);

CREATE TABLE messages (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    conversation_id uuid NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    -- Denormalised so every query can filter by owner directly.
    owner_id        uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role            text NOT NULL CHECK (role IN ('user', 'assistant')),
    content         text NOT NULL,
    -- How an assistant turn ended: answered, refused (weak evidence), no_llm
    -- (no model configured), cancelled (client went away) or error.
    status          text NOT NULL DEFAULT 'answered'
                    CHECK (status IN ('answered', 'refused', 'no_llm', 'cancelled', 'error')),
    -- [{n, chunk_id, file_id, file_name, page, char_start, char_end, quote}]
    citations       jsonb NOT NULL DEFAULT '[]' CHECK (jsonb_typeof(citations) = 'array'),
    model           text,
    input_tokens    integer,
    output_tokens   integer,
    latency_ms      integer,
    -- clock_timestamp: two messages written in one transaction still order.
    created_at      timestamptz NOT NULL DEFAULT clock_timestamp()
);

CREATE INDEX messages_conversation_created_idx ON messages (conversation_id, created_at, id);
CREATE INDEX messages_owner_idx ON messages (owner_id);
