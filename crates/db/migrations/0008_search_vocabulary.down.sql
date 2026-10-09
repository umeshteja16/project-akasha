DROP TRIGGER IF EXISTS file_chunks_terms_delete ON file_chunks;
DROP TRIGGER IF EXISTS file_chunks_terms_insert ON file_chunks;
DROP FUNCTION IF EXISTS user_terms_remove();
DROP FUNCTION IF EXISTS user_terms_add();
DROP FUNCTION IF EXISTS user_terms_lock(uuid[]);
DROP FUNCTION IF EXISTS akasha_terms(text);
DROP TABLE IF EXISTS user_terms;
DROP EXTENSION IF EXISTS btree_gin;
DROP EXTENSION IF EXISTS pg_trgm;
