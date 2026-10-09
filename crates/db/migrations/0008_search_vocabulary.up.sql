-- Per-owner vocabulary for spelling suggestions ("did you mean", step 3).
--
-- `user_terms` holds every word (3-32 letters, lowercased, unstemmed) that occurs
-- in an owner's chunks, with the number of chunks containing it. Statement-level
-- triggers on `file_chunks` keep it in step with every insert and delete
-- (extraction, reindex, file and account deletion), so no code path can forget.
-- Suggestions query it with pg_trgm similarity, always filtered by owner: a user
-- is never offered a word from someone else's documents.
--
-- Concurrency: each trigger takes a transaction-level advisory lock per owner
-- (two-int key space, so it never collides with the per-hash bigint locks) before
-- touching that owner's rows, which rules out deadlocks between concurrent
-- extractions and deletions of the same owner.
CREATE EXTENSION IF NOT EXISTS pg_trgm;
-- Lets one GIN index serve `owner_id = $1 AND term % $2`.
CREATE EXTENSION IF NOT EXISTS btree_gin;

CREATE TABLE user_terms (
    owner_id    uuid    NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    term        text    NOT NULL,
    chunk_count integer NOT NULL,
    PRIMARY KEY (owner_id, term)
);

CREATE INDEX user_terms_trgm_idx ON user_terms USING gin (owner_id, term gin_trgm_ops);

-- The words of a text as the vocabulary counts them.
CREATE FUNCTION akasha_terms(body text) RETURNS SETOF text
LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
    SELECT DISTINCT u.lexeme
    FROM unnest(to_tsvector('simple'::regconfig, body)) AS u(lexeme)
    WHERE char_length(u.lexeme) BETWEEN 3 AND 32 AND u.lexeme ~ '^[[:alpha:]]+$'
$$;

CREATE FUNCTION user_terms_lock(owners uuid[]) RETURNS void
LANGUAGE plpgsql AS $$
DECLARE
    o uuid;
BEGIN
    FOR o IN SELECT DISTINCT x FROM unnest(owners) AS x ORDER BY x LOOP
        PERFORM pg_advisory_xact_lock(1433, hashtext(o::text));
    END LOOP;
END
$$;

CREATE FUNCTION user_terms_add() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    PERFORM user_terms_lock(ARRAY(SELECT DISTINCT owner_id FROM new_chunks));
    INSERT INTO user_terms (owner_id, term, chunk_count)
    SELECT n.owner_id, t.term, count(*)::int
    FROM new_chunks n CROSS JOIN LATERAL akasha_terms(n.text) AS t(term)
    GROUP BY 1, 2
    ORDER BY 1, 2
    ON CONFLICT (owner_id, term)
        DO UPDATE SET chunk_count = user_terms.chunk_count + EXCLUDED.chunk_count;
    RETURN NULL;
END
$$;

CREATE FUNCTION user_terms_remove() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    gone_owners uuid[];
    gone_terms  text[];
BEGIN
    PERFORM user_terms_lock(ARRAY(SELECT DISTINCT owner_id FROM old_chunks));
    WITH removed AS (
        SELECT o.owner_id, t.term, count(*)::int AS n
        FROM old_chunks o CROSS JOIN LATERAL akasha_terms(o.text) AS t(term)
        GROUP BY 1, 2
    ), updated AS (
        UPDATE user_terms u SET chunk_count = u.chunk_count - r.n
        FROM removed r
        WHERE u.owner_id = r.owner_id AND u.term = r.term
        RETURNING u.owner_id, u.term, u.chunk_count
    )
    SELECT array_agg(owner_id), array_agg(term) INTO gone_owners, gone_terms
    FROM updated WHERE chunk_count <= 0;
    IF gone_owners IS NOT NULL THEN
        DELETE FROM user_terms u
        USING unnest(gone_owners, gone_terms) AS g(owner_id, term)
        WHERE u.owner_id = g.owner_id AND u.term = g.term;
    END IF;
    RETURN NULL;
END
$$;

CREATE TRIGGER file_chunks_terms_insert
    AFTER INSERT ON file_chunks REFERENCING NEW TABLE AS new_chunks
    FOR EACH STATEMENT EXECUTE FUNCTION user_terms_add();

CREATE TRIGGER file_chunks_terms_delete
    AFTER DELETE ON file_chunks REFERENCING OLD TABLE AS old_chunks
    FOR EACH STATEMENT EXECUTE FUNCTION user_terms_remove();

-- Chunks are never updated in place (re-extraction deletes and re-inserts), so
-- there is no UPDATE trigger.

INSERT INTO user_terms (owner_id, term, chunk_count)
SELECT c.owner_id, t.term, count(*)::int
FROM file_chunks c CROSS JOIN LATERAL akasha_terms(c.text) AS t(term)
GROUP BY 1, 2;
