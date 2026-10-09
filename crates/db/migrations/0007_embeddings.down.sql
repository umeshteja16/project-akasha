DROP INDEX IF EXISTS file_chunks_embedding_idx;
ALTER TABLE file_chunks DROP COLUMN IF EXISTS embedding;
DROP TABLE IF EXISTS embedding_model;
