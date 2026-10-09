-- pgvector: embedding columns and HNSW indexes for semantic search (step 2+).
-- Docker/CI use the pgvector/pgvector image; `scripts/local-postgres.sh` installs it
-- for a system Postgres.
CREATE EXTENSION IF NOT EXISTS vector;
