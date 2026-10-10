ALTER TABLE jobs DROP COLUMN progress;
ALTER TABLE file_chunks
    DROP CONSTRAINT file_chunks_time_span,
    DROP COLUMN end_ms,
    DROP COLUMN start_ms;
ALTER TABLE file_extractions
    DROP COLUMN duration_ms,
    DROP COLUMN segments;
