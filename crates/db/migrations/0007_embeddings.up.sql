-- Chunk embeddings for semantic search (ADR 0009).
--
-- The column has a fixed dimension: 384, the default model's
-- (multilingual-e5-small). `embedding_model` records which model produced the
-- vectors; the app refuses to start with a different configured model, and
-- `akasha reembed` switches models (it re-types this column and rebuilds the
-- index, the one schema change made outside migrations). Later migrations must
-- not assume the dimension is still 384.

CREATE TABLE embedding_model (
    -- Single row.
    id         boolean PRIMARY KEY DEFAULT true CHECK (id),
    -- Catalog name (`AKASHA_EMBED_MODEL`), e.g. `multilingual-e5-small`.
    name       text NOT NULL,
    dim        integer NOT NULL CHECK (dim > 0),
    updated_at timestamptz NOT NULL DEFAULT now()
);

-- NULL until the embed_file job has run for the chunk.
ALTER TABLE file_chunks ADD COLUMN embedding vector(384);

-- Cosine distance (`<=>`); pgvector 0.6+ (see local-postgres.sh) supports HNSW.
CREATE INDEX file_chunks_embedding_idx ON file_chunks
    USING hnsw (embedding vector_cosine_ops);
