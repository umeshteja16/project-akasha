DROP INDEX IF EXISTS files_owner_opened_idx;
DROP TRIGGER IF EXISTS files_updated_at ON files;
CREATE TRIGGER files_updated_at BEFORE UPDATE ON files
FOR EACH ROW EXECUTE FUNCTION set_updated_at();
ALTER TABLE files DROP COLUMN IF EXISTS open_count, DROP COLUMN IF EXISTS last_opened_at;
ALTER TABLE conversations DROP COLUMN IF EXISTS collection_id;
DROP TABLE IF EXISTS collection_files;
DROP TABLE IF EXISTS collections;
ALTER TABLE files DROP CONSTRAINT IF EXISTS files_id_owner_key;
