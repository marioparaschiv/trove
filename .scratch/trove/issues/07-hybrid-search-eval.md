Status: done

## Parent

`.scratch/trove/spec.md`

## What to build

The words stage and fusion (spec "Search pipeline" steps 2 and 3) as new pipeline stages, a per-result "why" (which stages matched), CLI switches to turn stages off, and `trove eval <queries.json>` reporting recall@5, recall@10 and MRR. Ship an eval file for the fixture vault; the real query set from my vault gets added later by me.

## Acceptance criteria

- [x] An exact identifier in one note is found by a words query even when meaning ranks it low
- [x] `--no-words` and `--no-meaning` each change results as expected on the fixture vault
- [x] Each result lists the stages that matched
- [x] `trove eval` prints the three metrics and exits non-zero on a malformed queries file

## Blocked by

- 06-tracer-meaning-search

## Comments

Why. `trove_core::search::Signal` (`meaning`, `words`; lowercase in JSON, `Display` gives the same word) is both the per-result reason and the stage switch. `Candidate.why: BTreeSet<Signal>`: each finding stage puts its own signal on what it adds; `Fusion` unions them per chunk and `CollapseToNotes` per note, so `SearchResult.why` lists every stage that found any of the note's chunks. 09 adds `Signal::Graph` (plus its `Display` arm) and inserts it in its stage.

Pipeline (`search/request.rs`): `Meaning` unless skipped, `Words` unless skipped, `Fusion`, then `CollapseToNotes`; 09's graph stage goes between `Fusion` and `CollapseToNotes`. Finding stages append their matches best first; `Fusion` ranks a chunk per signal by its place among the candidates carrying that signal.
- Words (`search/words.rs`): FTS5 over `chunks_fts`, `bm25(chunks_fts, 3.0, 2.0, 1.0)` (title, heading path, text), each whitespace word quoted as a phrase and ANDed (so `7Q4K-29XP` stays one phrase and punctuation is never FTS5 syntax); words with no letter or digit are dropped.
- Fusion (`search/fusion.rs`): reciprocal rank fusion, K = 60. Scores are small (one signal at rank 1 is 0.0164), so the plain CLI prints 4 decimals.
- Meaning: `MIN_Z` lowered from 2.5 to 2.35. Measured on the fixture vault (34 chunks): best z of wanted notes 2.47 to 4.65 (lowest: "float test for starter" 2.47, "what to do when I can't fall asleep at night" 2.49, "how to keep my bread culture alive" 2.73); queries on topics the vault lacks top out at 2.27 (honeybees 2.25, quantum computing 2.27, weather 2.25, car tyre 2.15, orbital mechanics 1.68). Loosely associated topics go higher (castle architecture to the chess note 3.14, pizza in Naples to "Italian Game" 2.80, tax return to index funds 2.50), so no gate separates everything on a vault this small. Re-measure on the real query set.

API: `SearchRequest.skip: Signal[]` (optional, default none, unknown names are a 400), `SearchResult.why: Signal[]`. The TS schema picks both up on `bun run generate`.

CLI: `crates/trove/src/searcher.rs` holds `StageSwitches` (`--no-meaning`, `--no-words`, flattened into both `search` and `eval`; 09 adds `--no-graph` there and maps it in `skip()`) and `Searcher` (the running service, else one `Mutex<Index>` and one `Embedder`, so `eval` loads the model once). Plain `trove search` lines are `score  path > headings  (meaning, words)`.

Eval: `trove eval <queries.json> [--no-meaning] [--no-words]`. The file is a JSON array of `{"query", "note"}`, `note` being the path inside its source, `/`-separated; unknown fields, an empty list or an empty `query`/`note` are rejected and any bad file exits non-zero with "failed to read queries from <path>". Each query is searched with limit 10 and its rank printed (`-` when not in the top 10), then `recall@5`, `recall@10` and `MRR` (top 10, a miss counts 0). Fixture set: `crates/trove/tests/fixtures/queries.json`, 20 queries.

Fixture vault grew three notes: `Home/Boiler service.md` (holds the identifier `7Q4K-29XP` inside a long service log: meaning ranks the note second at z 1.88, below the gate, and words find it), `Games/Chess openings.md`, `Health/Burns first aid.md`. 34 chunks now; `meaning_search` still passes.

Measured (`trove eval crates/trove/tests/fixtures/queries.json`, fixture vault):
- all stages: recall@5 1.000, recall@10 1.000, MRR 1.000
- `--no-words`: 0.950 for all three (misses the identifier)
- `--no-meaning`: 0.250 for all three (finds only the 5 word queries)
- with the old `MIN_Z = 2.5`: 0.900 for all three (misses the starter float test and the sleep query)
The fixture set saturates; the real query set from the vault is what should steer fusion and graph weights.

Tests: `crates/trove/tests/hybrid_search.rs` (identifier found by words and gone with `--no-words`; paraphrase found by meaning and gone with `--no-meaning`; `why` in JSON and plain output; `skip` and `why` over HTTP and the CLI switches through the service; eval metrics on the fixture set; eval failures on malformed files).
