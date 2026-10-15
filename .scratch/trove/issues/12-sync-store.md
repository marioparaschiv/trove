Status: done

## Parent

`.scratch/trove/spec.md`

## What to build

The sync store for vectors (spec "Sync store": store folder, vector files, deterministic and atomic writes, reading, cleanup). Decisions files come in 16. The local index becomes rebuildable from the vault plus the store.

## Acceptance criteria

- [x] Indexing the same vault twice gives byte-identical vector files
- [x] Deleting the local index and rebuilding makes zero model calls (visible in `/status`)
- [x] A second data folder on the same vault (a second "device") imports the first's vectors without embedding
- [x] A file with another `vector_version` is ignored and replaced on the next embed
- [x] Syncthing and Dropbox style conflict copies in the store folder are removed
- [x] The store folder name is configurable and never indexed

## Blocked by

- 08-change-detection

## Comments

Code: `crates/trove-core/src/sync_store.rs` (file format, paths, atomic write, conflict cleanup), `indexer/store.rs` (glue between the scan and the store), small hooks in `indexer/scan.rs` and `indexer/chunker.rs`. Tests: `crates/trove/tests/sync_store.rs`.

Store layout and format.
- `<source>/<store_folder>/vectors/<first two hex>/<SHA-256 of the note's source-relative, '/'-separated path>.vec`. 16's decisions files go beside `vectors/` in the store folder (for example `decisions/<device id>.jsonl`).
- Little-endian: magic `TROVEVEC`, format version u32 (1), `vector_version` as 64 ASCII hex bytes (offset 12..76), note content hash (32 bytes, SHA-256 of the file bytes, same as `files.hash`), dims u32, chunk count u32; per chunk in ordinal order: text hash (32), heading path (u32 length + UTF-8, `\n`-joined like the index), span start and end (u64 byte offsets), vector (`dims` fp16, the `vectors.vector` blob as is). No `token_count`: chunks loaded from the store have `token_count` NULL.
- `SyncStore::write` compares with the existing bytes and skips identical writes (sync tools would see a change otherwise), else writes a `tempfile` in the shard folder and renames it.

Index flow (`index_sources`).
- Per note that gets chunked (not `reembed`), the note's vector file is read. If its `vector_version` matches, its vectors are imported (`INSERT OR IGNORE`, so a changed note reuses the vectors of unchanged texts). If its content hash also matches and every span's text hashes to its record, `NoteFile.stored_chunks` carries the records and the `Chunker` takes them instead of splitting, so the model is never loaded. Other processors (09's link parser) run as before and can ignore `stored_chunks`.
- A file with another format or `vector_version` is ignored; a malformed or unreadable one is an `IndexReport` problem and treated as missing.
- After the unused-vector cleanup, the vector files of notes chunked in this run plus notes with a chunk about to be embedded are written once `embed_missing` is done (so a `vector_version` change rewrites every file on the next run). Write failures are problems, not errors: the local index stays right.
- Notes dropped from `files` (deleted, or the old path of a rename) have their file removed. Each source's store folder is walked for Syncthing (`.sync-conflict-`) and Dropbox (`conflicted copy`) copies, which are deleted.

Zero model calls on rebuild: `GET /status` `indexing.embedded` stays 0 and `model.loaded` stays false after deleting `index.sqlite` and letting the startup run rebuild it (asserted in `a_rebuilt_index_makes_no_model_calls`). No new counter was needed: token counting is skipped too, so the model is never loaded.

Determinism. The file bytes are a pure function of the note, its chunks and their vectors. llama.cpp vectors depend slightly on batch composition (measured: the same note embedded alone and among all chunks differs in the low fp16 bits) and on the CPU variant or backend. So the same machine re-embedding the same vault gives identical files (tested with `--reembed`), but two devices that each embed a note independently can write different bytes. A device that finds a matching file imports its vectors and rewrites nothing, so only truly simultaneous embeds diverge, and their conflict copies are deleted.

Tests that index the fixture vault now use `vault_copy()` (`hybrid_search.rs`, `meaning_search.rs`): indexing writes the store into the vault, and a shared store would let tests import each other's vectors.

Not done, deviations.
- The store folder is not watched: a vector file arriving after its note was already embedded locally is not imported (the local vectors are valid; the note's next change rewrites the file). The watcher still ignores the store folder.
- The store folder name is the global `store_folder` config (default `.trove`), not a per-source setting; v1 has one vault as one source.
