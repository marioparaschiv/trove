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

-- The same normalised columns split into every run of three characters, so that a word matches inside a longer run
-- (Chinese, Japanese and Thai don't separate words) and a misspelt word finds the words it shares runs with.
-- Contentless: the text is in chunks_fts.
CREATE VIRTUAL TABLE chunks_trigram USING fts5 (
    title,
    heading_path,
    text,
    content = '',
    contentless_delete = 1,
    tokenize = 'trigram remove_diacritics 1'
);

CREATE TRIGGER chunks_fts_delete AFTER DELETE ON chunks BEGIN
    DELETE FROM chunks_fts WHERE rowid = old.id;
    DELETE FROM chunks_trigram WHERE rowid = old.id;
END;
