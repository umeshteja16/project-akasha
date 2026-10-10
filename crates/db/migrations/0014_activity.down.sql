ALTER TABLE sessions DROP COLUMN IF EXISTS ip;
ALTER TABLE users DROP COLUMN IF EXISTS record_search_history;
DROP TABLE IF EXISTS activity_events;
