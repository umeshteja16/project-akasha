-- Baseline: extensions and helpers every later migration may rely on.
-- pgvector (`vector`) is added by the ingestion migration in step 2.

CREATE EXTENSION IF NOT EXISTS citext;

-- Keeps `updated_at` columns honest. Attach with:
--   CREATE TRIGGER t_updated_at BEFORE UPDATE ON <table>
--   FOR EACH ROW EXECUTE FUNCTION set_updated_at();
CREATE OR REPLACE FUNCTION set_updated_at() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$;
