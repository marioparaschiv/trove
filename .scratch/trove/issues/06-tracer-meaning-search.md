Status: done

## Parent

`.scratch/trove/spec.md`

## What to build

Tracer bullet through every layer. `trove sources add <folder>` registers a vault; `trove index` scans `.md` files (skipping folders per spec "Indexing"), chunks by heading, embeds, and stores chunks and vectors. `trove search` and `POST /search` return notes by meaning as the first pipeline stage (spec "Search pipeline" step 1: z-scored, collapsed to notes with the best heading). Also `POST /prewarm`, `POST /unload`, the sources endpoints, and model state in `/status`.

Creates the seam 1 fixture vault (a handful of notes on clearly separate topics) that later slices extend.

## Acceptance criteria

- [x] A paraphrased query sharing no words with a fixture note returns it first, through CLI and HTTP
- [x] Results carry the best chunk's heading path
- [x] A query matching nothing returns an empty list
- [x] `.obsidian` and trash folders are never indexed
- [x] Prewarm loads the model and unload frees it, both visible in `/status`

## Blocked by

- 03-embedder
- 04-service-lifecycle

## Comments

Fixture vault (seam 1): `crates/trove/tests/fixtures/vault/`, checked out with LF everywhere (`.gitattributes` beside it). Nine notes on separate topics (sourdough, retry backoff, Kyoto, tomato blight, index funds, guitar, sleep, bicycle chain, a birding journal), most with an H1 and two H2 sections, 26 chunks. `Nature/Birding journal.md` is one section of about 2,430 tokens whose last paragraphs (an air mattress repair) are past the 2048-token context, so a query about them only finds the note because the section is split. `.trash/Beekeeping.md` and `.obsidian/Beekeeping copy.md` must never be indexed. To extend it, add notes on topics distinct from these; z-scores are relative to the whole vault, so a new note can move scores of existing queries: rerun `cargo test -p trove --test meaning_search`. Helpers in `crates/trove/tests/common/mod.rs` (`mod common;` in a test file): `FIXTURE_VAULT`, `data_dir_with_model()` (temp data folder with the pinned model hard-linked from the HF cache), `vault_copy()` (a temp copy a test may edit), `indexed(vault)`, `run(data_dir, args)`, `search(data_dir, query)` (`trove search --json`), `Serve::start_in(data_dir)` and `serve.call(method, path, body)`. `serve.rs` uses the same `Serve`. Tests in `meaning_search.rs` check that each paraphrase shares no word with its note.

Indexing flow (`indexer::index_sources(&Mutex<Index>, &Arc<Embedder>, &Config)`, used by `trove index` and `POST /index`):
- `ensure_vector_version` writes `meta.vector_version` and deletes every vector if it changed.
- Per source, `indexer/scan.rs` walks the folder (skips every entry whose name starts with `.`, as Obsidian does, which covers `.obsidian`, `.trash` and the default `.trove`; also the configured store folder and symbolic links), reads every `.md` file, upserts its `files` row (mtime, SHA-256, title = file stem, properties) and runs `Indexer::standard(embedder)` in the same transaction, one note per lock of the index. Then it deletes `files` rows of notes no longer found. A source folder that can't be listed is reported and keeps its rows. 08 adds the mtime/hash skip on top of this loop; today every note is re-chunked on every run, but nothing unchanged is re-embedded.
- `indexer/embed.rs` then embeds chunk texts with no vector, 16 per call, holding the index only to read and write (never while the model runs).
- Problems (non-UTF-8 note, unreadable folder, frontmatter that isn't a YAML mapping) go into `IndexReport.problems`; a note with bad frontmatter is still indexed, with no properties. The CLI prints them as warnings.
- `Indexer::process` now takes the caller's `&Transaction` and processors return `IndexerError` (index or embedder). 09 appends the link parser to `Indexer::standard`.
- Chunker: splits sections whose document prompt is over `CONTEXT_TOKENS` at blank lines, packing paragraphs greedily; a single paragraph over the context stays whole and is cut by the embedder. `token_count` is the prompt's length. Counting tokens loads the model, so re-chunking needs the model; 12's rebuild-without-model-calls should take chunk spans from the store instead of re-chunking (or count "model calls" as embeds). The document prompt title is `document_title(note title, heading path)`: the heading path joined with ` > `, or the note title for text before the first heading. `Chunker::VERSION` stays 1: nothing was embedded under the old chunker.
- Frontmatter: `indexer/frontmatter.rs` (`serde-saphyr`, pure Rust and panic-free) parses it into `files.properties` as a JSON object; the chunker skips it through the same `Frontmatter::find`.
- Vectors: `index/vectors.rs` stores fp16 little-endian (`half`), and `chunk_scores` streams them from SQLite per query. Deviation: the spec holds vectors in memory while the model is loaded; reading them per query was not measured as too slow, so there is no cache yet (worth measuring on a 20k-chunk vault).

Search (`search::search(&Mutex<Index>, &Embedder, &SearchRequest)`):
- `Pipeline<'a>` now takes stages that borrow (`Box<dyn Stage + 'a>`), built per request. Stages hold `&Mutex<Index>` (and the embedder), locking briefly. Order today: `Meaning` (search/meaning.rs), then `CollapseToNotes` (search/collapse.rs), then the limit and the lookup of path and headings (search/request.rs). 07 inserts `Words` and fusion between them and 09 the graph stage before the collapse; `Meaning` appends its matches to the incoming candidates rather than replacing them.
- Meaning: cosine to every chunk vector, z-scored over all of them (population deviation); a chunk matches at `MIN_Z = 2.5`. Deviation from DigUp's 2.0: on this fixture unrelated queries reach z 2.2 to 2.4 and paraphrases score 3.3 to 3.5, so 2.0 returned results for "honeybees in winter". 07's eval should set the final value. Candidates carry the z-score as `score`.
- Request: `{query, limit?}` (default 10, at least 1, unknown fields rejected). Response: `{results: [{source, path, title, heading_path: [..], score}]}`, best first; `source` is the registered folder and `path` is `/`-separated inside it, so the Obsidian plugin can open `path#last heading`. 07 adds stage switches and the "why".

HTTP (one file each under `http/`): `POST /search`, `POST /prewarm` and `POST /unload` (both return `{loaded}` once done), `GET /sources`, `POST /sources {path}` (201; 400 relative or not a folder, 409 duplicate), `DELETE /sources?path=` (404 unknown), `POST /index` (returns the `IndexReport` when finished, no progress events yet; 08 adds a body for `--full`/`--reembed` and SSE progress). `/status` gained `model: {loaded}`. Malformed JSON or query strings are a 400 with `ErrorBody`. `AppState` now has `config` and `embedder: Arc<Embedder>` (`Embedder::from_config`). Lock order is index then model; nothing takes the index while holding the model.

CLI: `sources add|remove <folder>` (made absolute on the client), `sources list`, `index`, `search <query> [--limit N] [--json]`, `unload`, and `status` prints a `model:` line. Each goes through the running service when there is one (`Service::post`, `post_empty`, `delete` beside `get`), otherwise opens the index directly.

Measured (Ryzen 7 5800X, debug build with release llama.cpp): `trove index` on the fixture vault 4.7 s from cold (model load plus 26 chunks); a CLI search including model load about 1 s; `cargo test -p trove --test meaning_search` 17 s.
