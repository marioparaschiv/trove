Status: done

## Parent

`.scratch/trove/spec.md`. Read it once; every issue references its sections instead of repeating them.

## What to build

Prefactor that lets later slices run in parallel without colliding. Split the workspace into a core library crate and the `trove` binary crate. Put in place the contracts every later slice builds on:
- The local index schema, version 1, with every table from spec "Local index", applied on open.
- Per-OS data folder and a validated config (timeouts and store folder name, defaults from spec "Further Notes"), failing loudly on bad values.
- The search pipeline as an ordered list of independent stages behind one interface (spec "Search pipeline"), so meaning, words and graph stages land in separate modules.
- A CLI skeleton (subcommands from spec "CLI") that later slices fill in.

No llama.cpp, no HTTP, no search logic.

## Acceptance criteria

- [x] `cargo build`, `cargo clippy` and `cargo test` pass on the workspace
- [x] Opening an empty data folder creates the schema; reopening is a no-op; schema version is readable
- [x] Invalid config values fail with a message naming the field
- [x] A pipeline built from two test-only stages runs them in order

## Blocked by

None - can start immediately

## Comments

Layout. Add new work as new files; the shared files below only need a one-line registration.
- `crates/trove-core` (lib `trove_core`): `data_dir.rs`, `config.rs`, `index.rs` + `index/schema.sql`, `indexer.rs` + `indexer/chunker.rs`, `search.rs`. Later modules go beside them: `embedder.rs` (or a separate crate for the llama.cpp bindings, per spec), `model_setup.rs`, `graph.rs`, `categories.rs`, `sync_store.rs`, `http/`, and search stages as `search/meaning.rs`, `search/words.rs`, `search/fusion.rs`, `search/graph.rs`. Register each in `lib.rs` or with a `mod` line in its parent.
- `crates/trove` (bin): `main.rs` resolves the data folder, loads config, dispatches. One file per command in `commands/` with its own `Args` (empty for now) and a `run(ctx, args)` stub that bails "not implemented yet". Add flags in that file only. `commands::Context` holds `data_dir` and `config`; it carries `#[expect(dead_code)]` that the first command reading it must delete.

Contracts:
- Data folder: `DataDir::resolve()` uses `TROVE_DATA_DIR` (must be absolute) or the OS local data folder (`directories` `ProjectDirs` "Trove"). Tests point `TROVE_DATA_DIR` at a temp folder. `DataDir::at(path)` for library tests. Each module names its own file in it (`INDEX_FILE`, `CONFIG_FILE`).
- Config: `<data>/config.toml`, all fields optional, unknown fields rejected. `model_idle_timeout`, `service_idle_timeout`, `index_debounce` are humantime strings (`"180s"`, `"500ms"`; zero rejected) so tests can inject short timeouts. `store_folder` must be a single folder name valid on every OS. Per-vault store folder overrides belong on `sources` when 12 needs them.
- Index: `Index::open(&DataDir)` applies schema v1 (or checks it) in an immediate transaction; other versions fail with `UnsupportedSchema`. `meta` holds `schema_version` (integer) and `device_id` (UUID v4); `vector_version` is not written yet (03/06). Bump `SCHEMA_VERSION` and add a migration for any schema change. Notes on columns are in `schema.sql`: `heading_path` is headings joined by `\n`; spans are byte offsets; hashes are SHA-256 blobs; `chunks_fts` rowid is the chunk id and a trigger removes FTS rows when chunks go (including cascades from `files`). `edges.src`/`dst` are text node keys; 09 defines the key format. `token_count` is nullable.
- Indexer: `FileProcessor::process(&Transaction, &NoteFile)` replaces the rows it owns for one note; `Indexer` runs its processors in order in one transaction. `Indexer::default()` is `[Chunker]`; 09 appends the link parser there. The caller (06 scan, 08 change detection) writes the `files` row first and passes its id. `Chunker` splits on ATX headings (ignoring fenced code and frontmatter), skips empty sections, and writes `chunks` + `chunks_fts`. Left for 06: splitting long sections at paragraphs to fit the context and filling `token_count` (both need the embedder's tokenizer), parsing frontmatter into `files.properties`. Open question for 06/08: `text_hash` covers the chunk text only, but the document prompt also includes the title, so a renamed note's preamble chunk would reuse a vector embedded with the old title.
- Search: `Stage::run(&Query, Vec<Candidate>) -> Result<Vec<Candidate>, SearchError>`; `Pipeline::new(stages).run(&query)` folds them in order from an empty list. CLI switches build the pipeline without the disabled stages. `Query` and `Candidate` hold only `text` and `chunk_id`/`score`; 07 adds the per-stage "why", collapse-to-notes and limit.
