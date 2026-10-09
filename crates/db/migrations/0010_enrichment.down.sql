ALTER TABLE conversations DROP COLUMN IF EXISTS title_source;

DROP INDEX IF EXISTS files_auto_tags_idx;
ALTER TABLE files
    DROP COLUMN IF EXISTS enriched_at,
    DROP COLUMN IF EXISTS enriched_from,
    DROP COLUMN IF EXISTS enrichment_model,
    DROP COLUMN IF EXISTS enrichment_status,
    DROP COLUMN IF EXISTS auto_tags,
    DROP COLUMN IF EXISTS summary;
