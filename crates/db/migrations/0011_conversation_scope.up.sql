-- Step 5d: a conversation remembers which files it answers from. Empty: all of
-- the owner's files. Ids are checked against the owner's files when written;
-- a file deleted later simply stops matching (search filters by owner).
ALTER TABLE conversations
    ADD COLUMN file_ids uuid[] NOT NULL DEFAULT '{}'
        CHECK (cardinality(file_ids) <= 100);
