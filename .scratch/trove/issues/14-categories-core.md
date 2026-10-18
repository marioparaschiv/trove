Status: done

## Parent

`.scratch/trove/spec.md`

## What to build

Categories in the core (spec "Graph" for Leiden and centrality, spec "Categories" for naming): hierarchical communities over the combined graph, a start-here note per category, c-TF-IDF names, `GET /categories` and `trove categories`. Use the Leiden finding recorded in 09's comments.

## Acceptance criteria

- [x] A fixture vault with two clearly separate topics puts each topic's notes in different top-level categories
- [x] A larger topic splits into subcategories
- [x] Each category has a start-here note from its members
- [x] Names contain terms distinctive to the category's notes
- [x] The same input gives the same hierarchy (seeded)

## Blocked by

- 09-graph-rerank

## Comments

Layout. `crates/trove-core/src/categories.rs` (`categories::categories(&Mutex<Index>)`) and `categories/names.rs` (c-TF-IDF); `http/categories.rs` (`GET /categories`); `crates/trove/src/commands/categories.rs` (`trove categories [--json]`, through the service when one runs, else the index directly). `Graph::edges(node)` was added to read a node's edges. Tests: `crates/trove/tests/categories.rs` on its own fixture, `crates/trove/tests/fixtures/categories-vault/` (the shared vault is unchanged), indexed as a copy through the new `common::copy_of(vault)` (`vault_copy()` is now `copy_of(FIXTURE_VAULT)`), since indexing writes the store folder into the vault.

Leiden. graspologic's `network_partitions` 0.3.0 as a git dependency at rev `773181488b4e5b81925e302b972b294fd51f61a4`, plus `rand` 0.10 (`default-features = false`). Seeded with `Xoshiro256PlusPlus::seed_from_u64(14)`, a named algorithm that gives the same numbers on every platform (`StdRng` is not `Clone`, which the library needs, and `SmallRng` differs between platforms). Modularity, with the library's defaults for resolution (1.0), randomness and iterations. A community of at least `SPLIT_SIZE` notes (10, GraphRAG's default) is split again, one level deeper.

Graph. Categories group notes only. The combined graph (09's `VaultGraph`: semantic, link, property link, tag, co-citation and coupling edges) is projected onto notes: an edge between two notes' own nodes (the note or its sections) adds its weight, and a tag on k notes joins each two of them with 1/(k-1) of its weight. Notes with no edge to another note (no resolved link, no shared tag and no semantic neighbour above the 0.75 floor) are in no category. Everything is computed per request from SQLite, like the graph in search; the index is locked only while reading. The `communities` table stays unused.

API. `GET /categories` returns `{categories: [{id, parent, name, start_here, notes}]}`, flat, each parent before its subcategories, `parent` null at the top level. `notes` are the notes in that category and in none of its subcategories, sorted by source then path; a fully split parent has none, so a note's `category` (15) is the deepest category that lists it. A note is `{source, path, title}`. `start_here` is the member (subcategories included) with the highest PageRank over the edges among the members (uniform restart, 09's `RESTART` and `ITERATIONS`), lowest file id on a tie. `id` is Leiden's cluster id: the same index gives the same ids, but they change when the vault changes, so 17 needs its own identity for pins.

Names. c-TF-IDF against sibling categories (top-level ones against each other, subcategories against the other subcategories of their parent): a term's frequency in the category's notes (subcategories included) times ln(1 + A / its count across the siblings), A being the average term count per sibling. Terms are lowercased words of 3+ letters or digits from note titles and chunk texts, not all digits, minus the ISO English stop words (`stop-words` crate). The name is the top 3 joined with `, ` (e.g. `wind, boat, mainsail`), ties alphabetical.

Fixture finding. Leiden's top level is already the modularity optimum, so a topic stays one top-level community (and is then split) only when it is small next to the rest of the graph (modularity's resolution limit). In the fixture, the kitchen topic (12 single-section bread and cheese notes plus two hub notes, `Kitchen projects` and `Fermentation`) is one top-level category with two subcategories; when its notes had three sections each, bread and cheese came out as separate top-level categories. On a real vault, with many topics, large topics behave like the kitchen one. The sailing topic (12 notes of three sections) splits at the top level into two categories (sail handling, navigation). Subcategories follow the graph, not folders: one bread note sits with the cheese notes because of its links.

Verified after merging trove-v1 (with 12): `cargo build`, `cargo clippy --all-targets` (no warnings) and `cargo test` pass; `cargo test -p trove --test categories` passed 4 runs in a row before the merge. `bun run typecheck`, `lint`, `test` and `build` in `clients/` pass with the new OpenAPI spec.
