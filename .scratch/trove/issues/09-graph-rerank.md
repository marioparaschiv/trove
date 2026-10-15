Status: done

## Parent

`.scratch/trove/spec.md`

## What to build

The graph (spec "Graph": nodes, every edge kind and its starting weight, in-memory build from the edges table) and the graph stage of search (spec "Search pipeline" step 4): the link parser per spec "Indexing", nearest-neighbour semantic edges, personalised PageRank seeded by the hits, `--no-graph`, and "graph" in each result's why. Also check whether a solid Leiden crate exists and record the finding under a `## Comments` heading here for 14.

## Acceptance criteria

- [x] Wikilinks (with heading and block targets), embeds, markdown links, tags and property links all become edges on the fixture vault
- [x] A note one link away from the direct match appears in results and disappears with `--no-graph`
- [x] Editing one note updates only its own edges
- [x] PageRank on a small hand-built graph matches known values

## Blocked by

- 06-tracer-meaning-search

## Comments

Layout. `crates/trove-core/src/graph.rs` is the in-memory graph (`Graph`: undirected and weighted, with `add_node`, `add_edge` and `personalized_pagerank(&[(node, weight)])`; `RESTART` 0.15, `ITERATIONS` 20; a node with no edges sends its walk back to the seeds). `graph/vault.rs` (`VaultGraph::load(&Connection)`) builds it from the index on each call. `graph/node_key.rs` holds the `edges` key format, and `graph/resolve.rs` resolves link targets. The indexer side is `indexer/links.rs` (`LinkParser`, appended to `Indexer::standard`) and `indexer/neighbours.rs` (`update_graph_edges`, one line in `index_sources` after `embed_missing`). The search stage is `search/graph.rs` (`GraphRerank`, after `Fusion` and before `CollapseToNotes`). No schema change: the `edges` CHECK already allowed every kind.

Edges table contract (for 13, 14, 16):
- Node keys: `note:<file id>`; `section:<file id>:<text hash hex>` (by text, not chunk id, since chunk ids change on every re-chunk and an unchanged section keeps its key); `tag:<lowercased>`; `link:<target as written>`, e.g. `link:Kyoto#Arashiyama`, `link:Retry backoff#^spread` or `link:../Home/Bicycle chain.md` (percent-decoded). `NodeKey` implements `ToSql`/`FromSql`.
- Stored kinds: `link` (wikilinks, embeds and markdown links without a URL scheme; note to `link:`, 1.0), `property_link` (wikilinks and markdown links in any property string except `tags`; 1.0), `tag` (`#tag` in the body and each string in the `tags` property; note to `tag:`, 0.5), and `semantic` (section to section, each section's 10 nearest sections in other notes, weighted by cosine). Code fences and inline code are skipped. `co_citation` and `coupling` are never stored. They are computed at build from the resolved note-to-note links, as shared count over the geometric mean of the two notes' counts, so editing one note never rewrites another note's rows.
- Ownership: `LinkParser` replaces the note's own `link`/`property_link`/`tag` rows whenever the note is processed. With 08's skip, unchanged notes keep their rows. `update_graph_edges` runs after every index run. It first deletes edges whose `note:`/`section:` end no longer exists, then adds semantic rows for sections that have a vector but no semantic rows yet. The similarity floor is applied at build (`SEMANTIC_FLOOR` 0.75), so the stored rows keep every section's nearest whatever the similarity and the floor can change without re-indexing. On the fixture, unrelated sections reach 0.73, and a circuit-breaker note scored 0.80 against retry backoff.
- Resolution at build (Obsidian's rules, within the linking note's source): case-insensitive, `.md` optional. Order: relative to the linking note (`./`, `../`, or a bare relative path); `/` from the source root; exact path; then by name suffix, same folder first, then the shortest path. `#Heading` (or `#Outer#Inner`, last part counts) lands on the first chunk whose last heading matches, else the first chunk under it, else the note. `#^block` lands on the chunk with a line ending in `^block`. An unresolved target is ignored until its note exists. Links to the linking note itself are dropped.
- The graph has one node per note, per distinct section and per tag, plus section-to-note membership edges (1.0). Parallel edges of the same kind are counted once at their larger weight, and different kinds add up.

Search (`skip: ["graph"]` over HTTP, `--no-graph` on `search` and `eval` through 07's `StageSwitches`):
- Seeds are the fused candidates, weighted by score. A note's rank is its note node plus its sections. Each candidate becomes `0.7 * score / best_score + 0.3 * note_rank / best_note_rank` (`GRAPH_SHARE` 0.3). Blending by note rank reorders notes without moving the best heading inside a note.
- A note with no candidate is added with `why: ["graph"]` at its best-ranked section (the linked heading or block) when its rank is at least `MIN_RANK` 0.025 of the best note's. Its score is `0.3 * relative rank`, so it ranks below every direct match. On the fixture, with one match: linked or co-cited notes get about 0.3, a shared tag 0.04 to 0.08, unconnected notes 0. The eval on `queries.json` is unchanged with and without the graph (recall@5, recall@10 and MRR are all 1.000).

Fixture: one new hub, `Hobbies/Home espresso.md` (a single section), linking to unrelated notes with one kind each: `[[Kyoto#Arashiyama]]`, `![[Index funds]]`, `[chain care](../Home/Bicycle%20chain.md)`, `[[Retry backoff#^spread]]` (block id added to the Jitter paragraph of `Engineering/Retry backoff.md`), `related: "[[Tomato blight]]"`, and `#baking`, shared with Sourdough's `tags` property. Tests are in `crates/trove/tests/graph_search.rs` and `crates/trove-core/tests/pagerank.rs` (exact ranks of a 4-node weighted graph, within 1e-4 after 20 iterations). New notes move z-scores. The honeybee query's best non-match sits at z 2.28 against `MIN_Z` 2.35 (2.24 without the hub), so keep new fixture notes few and of a single section, and rerun `meaning_search`.

Deviations:
- The spec has the graph "updated incrementally" in memory. Instead it is rebuilt from SQLite on each search (no cache until measured, like vectors). The incremental part is in the rows: a changed note rewrites only its own link rows and adds semantic rows only for its new sections.
- petgraph is not used. The adjacency list plus a hand-written weighted, personalised PageRank is all that's needed.

Leiden finding (for 14). No crates.io crate does hierarchical Leiden properly. The best option is graspologic's Rust core `network_partitions` 0.3.0 (MIT, maintained, release 2026-06-14; graspologic and GraphRAG use it). It is not on crates.io, so it would be a git dependency pinned to a commit. It takes weighted edges and does true hierarchical Leiden: it re-runs on clusters over `max_cluster_size` and returns `node, cluster, level, parent_cluster, is_final_cluster`. It is deterministic with a seeded `rand` 0.10 RNG, single-threaded, and its only required dependency is `rand` (petgraph optional). API: build a `CompactNetwork` with `LabeledNetworkBuilder`, then call `hierarchical_leiden(..)`. The fallback is `leiden-rs` 0.8.1 (MIT/Apache, crates.io, weighted, seeded; use `default-features = false` to avoid clap and rayon). Its levels are only the aggregation levels of one run, not recursive. The others are unsuitable: `single-clustering` is flat and needs rayon; `fa-leiden-cd` uses rayon, has no seed and is unmaintained; `rune-leiden` is flat and toy-sized; `xgraph` is flat. Writing our own would take about 3 to 5 days for seeded flat Leiden plus 1 for the hierarchy.
