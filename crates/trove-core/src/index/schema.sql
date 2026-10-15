CREATE TABLE meta (
    key TEXT PRIMARY KEY,
    value ANY NOT NULL
) STRICT;

CREATE TABLE sources (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE
) STRICT;

-- path is relative to the source, '/'-separated. mtime is nanoseconds since the Unix epoch.
-- hash is the SHA-256 of the file's bytes. properties is the frontmatter as a JSON object.
CREATE TABLE files (
    id INTEGER PRIMARY KEY,
    source_id INTEGER NOT NULL REFERENCES sources (id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    mtime INTEGER NOT NULL,
    hash BLOB NOT NULL,
    title TEXT NOT NULL,
    properties TEXT NOT NULL,
    UNIQUE (source_id, path)
) STRICT;

-- heading_path joins the headings with '\n', which a heading can never contain.
-- span_start and span_end are byte offsets of the chunk text in the file.
-- token_count is the length of the chunk's document prompt (title and text) in model tokens.
-- text_hash is the SHA-256 of the chunk text and keys its vector.
CREATE TABLE chunks (
    id INTEGER PRIMARY KEY,
    file_id INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL,
    heading_path TEXT NOT NULL,
    span_start INTEGER NOT NULL,
    span_end INTEGER NOT NULL,
    token_count INTEGER,
    text_hash BLOB NOT NULL,
    UNIQUE (file_id, ordinal)
) STRICT;

CREATE INDEX chunks_text_hash ON chunks (text_hash);

-- vector is little-endian fp16.
CREATE TABLE vectors (
    text_hash BLOB PRIMARY KEY,
    vector BLOB NOT NULL
) STRICT;

-- rowid is the chunk id.
CREATE VIRTUAL TABLE chunks_fts USING fts5 (
    title,
    heading_path,
    text,
    tokenize = 'porter unicode61'
);

CREATE TRIGGER chunks_fts_delete AFTER DELETE ON chunks BEGIN
    DELETE FROM chunks_fts WHERE rowid = old.id;
END;

-- src and dst are graph node keys (notes, sections, tags, categories).
CREATE TABLE edges (
    src TEXT NOT NULL,
    dst TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (
        kind IN ('link', 'property_link', 'tag', 'semantic', 'co_citation', 'coupling')
    ),
    weight REAL NOT NULL,
    PRIMARY KEY (src, dst, kind)
) STRICT;

CREATE TABLE communities (
    level INTEGER NOT NULL,
    id INTEGER NOT NULL,
    parent INTEGER,
    member INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
    centrality REAL NOT NULL,
    PRIMARY KEY (level, id, member)
) STRICT;

CREATE TABLE suggestions (
    note_a INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
    note_b INTEGER NOT NULL REFERENCES files (id) ON DELETE CASCADE,
    state TEXT NOT NULL CHECK (state IN ('pending', 'accepted', 'dismissed')),
    PRIMARY KEY (note_a, note_b),
    CHECK (note_a < note_b)
) STRICT;
