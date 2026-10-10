-- Transcripts of audio and video (step 6, ADR 0017).

-- [{ "start_ms", "end_ms", "char_start", "char_end" }]: one entry per transcript
-- line, so the text can be shown with timestamps. Empty for everything else.
ALTER TABLE file_extractions
    ADD COLUMN segments jsonb NOT NULL DEFAULT '[]',
    -- Length of the transcribed audio; NULL for non-media files.
    ADD COLUMN duration_ms integer CHECK (duration_ms >= 0);

-- When a transcript chunk's speech starts and ends (like `page` for PDFs):
-- search results and citations can say "at 12:34". NULL for other formats.
ALTER TABLE file_chunks
    ADD COLUMN start_ms integer CHECK (start_ms >= 0),
    ADD COLUMN end_ms integer,
    ADD CONSTRAINT file_chunks_time_span CHECK (
        (start_ms IS NULL AND end_ms IS NULL) OR end_ms >= start_ms
    );

-- How far a long-running job is (0–1), reported by the handler; NULL if unknown.
ALTER TABLE jobs
    ADD COLUMN progress real CHECK (progress >= 0 AND progress <= 1);
