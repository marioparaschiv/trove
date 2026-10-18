-- Keyword search over each chunk. rowid is the chunk id. title, heading_path and text are normalised for matching
-- (keyword_text.rs); original is the text as written, when normalising changed it.
-- The tokenizer is DigUp's: marks count as letters, so a vowel sign or a virama stays inside its word, and diacritics
-- are removed.
CREATE VIRTUAL TABLE chunks_fts USING fts5 (
    title,
    heading_path,
    text,
    original UNINDEXED,
    tokenize = "porter unicode61 remove_diacritics 2 categories 'L* N* Co M*'"
);

CREATE TRIGGER chunks_fts_delete AFTER DELETE ON chunks BEGIN
    DELETE FROM chunks_fts WHERE rowid = old.id;
END;
