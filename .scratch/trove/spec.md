Status: ready-for-agent

# Trove v1

## Problem Statement

I keep notes and resources in an Obsidian vault, but I can't find things when I need them. Obsidian's search only matches words, so a vague memory ("that thing about retry backoff I read last month") finds nothing unless I remember the exact phrasing. The notes I wrote are linked to each other, but those links don't help search, and many related notes were never linked at all. There is no structure beyond the folders I made by hand, so I can't browse by topic either.

Existing tools don't fit. Smart Connections adds its own panel that looks out of place. DigUp (an on-device semantic search app built on EmbeddingGemma 2) is a good file finder, but it runs its own search window, has no categories or graph, gives every client its own process and model copy, and its core is Swift. Nothing uses my links and the model's sense of similarity together, and nothing works from both Obsidian and a Raycast-style launcher.

I also want this to cost nothing when I'm not using it (no model sitting in RAM), to run fully on my machine, and to work on Windows, macOS and Linux. I use more than one desktop, so I don't want to embed the whole vault again on each one.

## Solution

Trove is a local service, written in Rust, that indexes my vault, embeds it with EmbeddingGemma 2 running in process through llama.cpp, and builds a knowledge graph from the links I wrote plus the similarities the model finds. Search, "related notes" and categories are all graph walks over that combined graph, so the right resource turns up even when I don't remember its words.

Obsidian and Raycast are thin clients that talk to Trove over localhost HTTP. In Obsidian, everything looks native: semantic results appear in a quick switcher that looks like `Ctrl+O`, related notes appear at the bottom of each note like backlinks, breadcrumbs show where a note sits in the category hierarchy, and categories are ordinary note properties browsed through Bases views. No new panels, no custom styling.

Trove loads the model only while it's embedding or searching and unloads it when idle. The service itself exits when nothing uses it, and clients start it again on demand. Embeddings are written as small per-note files inside the vault, so whatever syncs the vault also syncs them, and a second machine reuses them instead of embedding again.

## User Stories

### Finding things
1. As a vault owner, I want to search my notes by meaning, so that I find a note from a vague description even when it shares none of my words.
2. As a vault owner, I want exact names, IDs and phrases to still match, so that searching for a specific term never does worse than Obsidian's own search.
3. As a vault owner, I want search results to include notes closely connected to the direct matches, so that I find the resource I need even when it's one link away from what my words describe.
4. As a vault owner, I want each result to open at the matching heading, so that I land on the relevant section of a long note.
5. As a vault owner, I want filename matches to appear instantly while semantic results merge in once ready, so that search never feels slower than the built-in quick switcher.
6. As a vault owner, I want search to say honestly when nothing matches, so that I don't waste time scanning a list of weak results.
7. As a vault owner, I want to see why a result was returned (meaning, words, or graph), so that I can trust and refine my searches.
8. As a vault owner, I want to search from Raycast without opening Obsidian first, so that I can find a resource from anywhere on my machine.
9. As a vault owner, I want a Raycast result to open the note in Obsidian, so that I go straight from finding to reading.
10. As a vault owner, I want to search from the command line, so that I can script searches and check results without any UI.

### Related notes
11. As a vault owner, I want a list of related notes at the bottom of each note, so that I discover connected material while I read.
12. As a vault owner, I want related notes ranked by how close they are in the graph (my links plus meaning), so that the list reflects how my knowledge actually connects.
13. As a vault owner, I want the related list to look like Obsidian's backlinks section, so that it feels like part of Obsidian.

### Categories
14. As a vault owner, I want my notes grouped into categories automatically, so that I can browse by topic without organising by hand.
15. As a vault owner, I want categories to have subcategories, so that large topics break down into navigable pieces.
16. As a vault owner, I want categories built from my links and from meaning together, so that the grouping matches both how I connected things and what they're about.
17. As a vault owner, I want categories stored as ordinary note properties, so that they work with Obsidian's built-in features and survive without Trove.
18. As a vault owner, I want each category to be a note listing its subcategories and member notes through a Bases view, so that I can click down through the hierarchy like folders.
19. As a vault owner, I want my files never to move, so that categorising is fully reversible and my folder layout stays mine.
20. As a vault owner, I want to preview category changes before they're written, so that nothing in my vault changes without my approval.
21. As a vault owner, I want each category to have a "start here" note (its most central note), so that I know where to begin on a topic.
22. As a vault owner, I want to rename, merge or move categories, so that names and structure match how I think.
23. As a vault owner, I want my manual category edits to win over later automatic runs, so that I never fight the tool over a decision I already made.
24. As a vault owner, I want a breadcrumb above each note showing its category path, so that I always know where a note sits.
25. As a vault owner, I want category names generated from what the notes have in common, so that a new category starts with a sensible name.

### Improving the graph
26. As a vault owner, I want suggestions for links between notes that are clearly related but not linked, so that my vault's own links get better over time.
27. As a vault owner, I want to accept or dismiss each suggestion, so that nothing is linked without my approval.
28. As a vault owner, I want dismissed suggestions to stay dismissed, so that I'm never asked the same thing twice.
29. As a vault owner, I want a list of orphan notes (no links in or out), so that I can connect or clean up forgotten notes.

### Staying up to date
30. As a vault owner, I want notes re-indexed automatically shortly after I edit them, so that search always reflects my current vault.
31. As a vault owner, I want edits made outside Obsidian (other editors, sync) to be picked up, so that the index is correct no matter how a file changed.
32. As a vault owner, I want changes made while Trove wasn't running to be caught on its next start, so that the index never silently goes stale.
33. As a vault owner, I want editing one section to re-embed only that section, so that small edits cost almost nothing.
34. As a vault owner, I want to force a full re-index or a full re-embed from the CLI, so that I can recover from any doubt about the index.
35. As a vault owner, I want to register more than one folder as a source, so that Trove can index things beyond one vault later.
36. As a vault owner, I want Trove to skip `.obsidian`, trash and its own store folder, so that config and internal files never pollute results.

### Resource use
37. As a vault owner, I want the model loaded only while it's needed and unloaded after a short idle time, so that Trove uses no RAM when I'm not searching or editing.
38. As a vault owner, I want the model to start loading the moment I open the search box, so that loading overlaps my typing.
39. As a vault owner, I want the service to exit when nothing has used it for a while, so that nothing runs in the background for no reason.
40. As a vault owner, I want clients to start the service automatically when they need it, so that I never manage a background process by hand.
41. As a vault owner, I want to unload the model on demand, so that I can free memory immediately.
42. As a vault owner, I want to see whether the model is loaded and how big the index is, so that I know what Trove is doing.

### Setup
43. As a vault owner, I want the model downloaded and verified on first run, so that setup is one step.
44. As a vault owner, I want an interrupted download to resume where it stopped, so that a flaky connection doesn't mean starting over.
45. As a vault owner, I want a model already in my Hugging Face cache to be reused, so that I don't download it twice.
46. As a vault owner, I want to see download progress in Obsidian, so that I know setup is working.
47. As a vault owner, I want the Obsidian plugin to download the right Trove binary for my OS, so that I don't install anything by hand.
48. As a vault owner, I want downloads checked against pinned hashes, so that a damaged or tampered file never runs.

### Sync across devices
49. As a vault owner with several desktops, I want embeddings stored as files inside the vault, so that whatever syncs my vault also syncs them.
50. As a vault owner with several desktops, I want a second machine to reuse synced embeddings instead of computing them again, so that a new device is ready in minutes.
51. As a vault owner with several desktops, I want sync conflicts to be harmless, so that two machines indexing at once never corrupt anything.
52. As a vault owner with several desktops, I want my dismissed suggestions and category edits to sync, so that decisions I made on one machine hold on the others.
53. As a vault owner with several desktops, I want the local index to be a rebuildable cache, so that deleting it costs nothing but a quick rebuild.
54. As a vault owner, I want the store folder name to be configurable, so that sync tools that skip dot-folders (Obsidian Sync) still work.
55. As a vault owner, I want vectors made on a Mac and on a Windows PC to mix in one index, so that the backend each machine uses doesn't matter.

### Platforms and look
56. As a vault owner, I want Trove to run on Windows, macOS and Linux, so that it works on every desktop I use.
57. As a vault owner on macOS, I want the model to run on the GPU through Metal, so that indexing is fast.
58. As a vault owner, I want everything Trove adds to Obsidian to use Obsidian's own styles, so that my theme and CSS snippets apply automatically.
59. As a vault owner, I want everything to run on my machine with no network calls except downloads, so that my notes stay private.

### Quality
60. As the developer, I want a set of real queries with known answers and a score (recall@5, recall@10, MRR), so that every search or graph change is measured instead of guessed.
61. As the developer, I want each search stage (meaning, words, graph) switchable from the CLI, so that the eval can measure what each adds.
62. As the developer, I want a reference vector test, so that a llama.cpp or model change that alters vectors is caught before it ships.
63. As the developer, I want the HTTP API described by an OpenAPI spec that generates the TypeScript client, so that the core and clients can't drift apart.
64. As the developer, I want CI to build and test on real Windows, macOS and Linux machines, so that platform bugs are caught before release.

## Implementation Decisions

### Shape
- A Cargo workspace. One binary, `trove`, is both the service and the CLI. A separate crate holds the llama.cpp bindings and its build.
- Two TypeScript clients (Obsidian plugin, Raycast extension) share one generated client package.
- The core knows nothing about Obsidian or Raycast. It indexes folders and returns paths, headings, scores and categories. Obsidian-specific behaviour (`obsidian://` links, Bases views, writing properties) lives only in the plugin.
- Only the core writes the local index and the sync store. Only the Obsidian plugin writes to notes.

### Embedding runtime
- llama.cpp is built into the binary from a pinned tag and commit: `b11461`, commit `4d756bc72bf00a4aacf410ae15a2d315f3db400d` (EmbeddingGemma 2 support starts at b11452).
- DigUp's no-logits patch is applied: without it an embedding-only context reserves vocabulary-sized output per token (about 1 MiB per token with Gemma's 262k vocabulary).
- macOS builds llama.cpp statically with Metal. Windows and Linux build it as shared libraries with runtime-loaded backends and CPU variants picked at run time, so one build runs on any CPU.
- Rust bindings are generated with bindgen from `llama.h` once per tag and committed. libclang is needed only when bumping the tag.
- The model is `ggml-org/embeddinggemma-2-GGUF` at revision `bfcd298762cc34d0357ece5ebdd31791a3a374d8`, file `embeddinggemma-2-Q8_0.gguf`, 309,855,456 bytes, SHA-256 `2188ac1deca4b77dffefd603c2776a9d76d9d74ec01841392982ebb840b09135`. Never F16 (the model's activations overflow float16).
- Text only in v1. Mean pooling, L2-normalised, so dot product equals cosine.
- Prompts: queries use `task: search result | query: {text}`. Documents use `title: {heading path or note title} | text: {chunk}`, to be confirmed against the model card during the first epic.
- Context starts at 2048 tokens. Raising it means raising context, batch and micro-batch together, up to 8192.
- Embedder interface: load (lazy), embed a batch of texts as queries or documents returning one vector per text in order, unload. Load is triggered by the first call; unload happens after the model idle timeout (default 180 s) or on request.
- A `vector_version` identifies everything that shapes vectors: model file, llama.cpp tag, prompts, chunker version. It does not include the compute backend.

### Model download
Same design as DigUp's downloader:
- The Hugging Face cache (`$HF_HUB_CACHE`, `$HF_HOME/hub`, `~/.cache/huggingface/hub`) is checked first and the file copied if its hash matches.
- Otherwise download to a `.partial` file at the pinned revision, resuming with an HTTP `Range` header that is kept across the redirect to the CDN.
- Size and SHA-256 are checked before the file gets its final name, so a file with its final name is always good.
- Retries back off 2, 4, 8 … 60 seconds, at most 20 failures; a file that arrives damaged is retried at most 3 times. With no network, it waits for one.
- Progress is reported as JSON lines with events `file`, `progress` (at most 5 per second), `waiting`, `verifying`, `retry`, `done`, `error`.

### Indexing
- Sources are registered folders. Paths are per device and not synced. A vault is one source.
- v1 indexes `.md` files only. `.obsidian`, trash folders and the store folder are skipped, along with gitignore-style excludes.
- Chunks follow headings. Each chunk keeps its heading path as its title. Long sections split at paragraph boundaries to fit the context. Frontmatter is parsed into properties and not embedded.
- The link parser extracts wikilinks (with heading and block targets), embeds, markdown links to vault files, tags, and links inside any property.
- A file watcher queues changes. Embedding runs after a debounce (default 30 s) since the last change.
- On start, every source is scanned: mtime first, then content hash. Only changed files are processed.
- Each chunk is keyed by a hash of its text, so unchanged sections of an edited note reuse their vectors, and a renamed note reuses all of its vectors.

### Local index
- One SQLite database (bundled SQLite, FTS5 enabled) in the per-OS data folder. It is a cache: deleting it and rebuilding from the vault plus the sync store needs no model calls.
- Tables: sources; files (path, mtime, hash, title, properties); chunks (file, heading path, span, token count, text hash); vectors (text hash, fp16 768-dim vector); full-text index over title, heading path and text with Porter stemming; edges (from, to, kind, weight); communities (level, id, parent, member, centrality); suggestions (pair, state); meta (`vector_version`, schema version, device id).
- Vectors are held in memory only while the model is loaded and released with it (fp16 is about 30 MB per 20k chunks).

### Sync store
- Lives at `<vault>/<store folder>`, default `.trove/`, configurable per vault. Obsidian Sync users set a name without a leading dot and enable "Sync all other types"; Syncthing, git and cloud drives work with the default.
- Vectors: one file per note, named by the SHA-256 of the note's vault-relative path and sharded into subfolders by the first two hex characters. The header holds magic, format version, `vector_version`, the note's content hash and dims. Then one record per chunk: text hash, heading path, span, fp16 vector.
- Files are deterministic (same note and `vector_version` give the same bytes), so two devices writing the same note produce identical files and a conflict can't lose anything.
- Files are written atomically (temporary name in the same folder, then rename) so a sync tool never picks up half a file.
- Decisions (suggestion accept or dismiss, category renames, merges, pins) are append-only JSON lines, one file per device id, merged on read with the latest timestamp winning.
- Reading: a note whose hash and `vector_version` match its file is loaded without the model. A changed note reuses vectors for chunks whose text hash matches. The store folder is watched, and a file arriving from another device is imported if it matches the current note. A file with another `vector_version` is ignored and replaced on the next embed.
- Cleanup: a deleted note's file is deleted. Conflict copies made by sync tools inside the store folder are deleted, since their content can always be rebuilt.
- Not synced: the local index, graph, communities, model files, runtime file, sources.

### Graph
- Nodes: notes, sections (chunks), categories. Built in memory from the edges table and updated incrementally (a changed note changes only its own edges and nearest-neighbour entries).
- Edge kinds and starting weights: link 1.0; property link 1.0; shared tag 0.5 (through a tag node); semantic, each chunk to its 10 nearest chunks above a similarity floor, weighted by cosine; co-citation (linked from the same notes) and coupling (linking to the same notes), normalised counts. The eval decides the final weights.
- Personalised PageRank with restart probability 0.15 and about 20 power iterations, seeded by search hits (weighted by score) for search, or by the open note for related notes.
- Hierarchical Leiden community detection on the combined graph. Each level is a category level.
- PageRank centrality inside each community picks the "start here" note and acts as a small tie-breaker in search.
- Link suggestions: pairs with high similarity, no link in either direction, not dismissed before.
- Orphans: notes with no links in or out.

### Search pipeline
1. Meaning: cosine against every vector, put on a per-query scale (z-score); only chunks that clearly stand out count as matches, so an empty result is honest.
2. Words: FTS5 with BM25, title and heading path weighted above body text.
3. Fusion of meaning and words, with reciprocal rank fusion as the baseline.
4. Graph rerank: personalised PageRank seeded by the fused hits, blended with the fused score.
5. Chunks collapse to notes, keeping the best chunk's heading.
Each stage can be turned off from the CLI so the eval can measure it.

### Categories
- Leiden levels become categories. v1 names come from each community's most distinctive terms (c-TF-IDF). Names are editable.
- The core returns the hierarchy. The plugin writes it after a preview: one category note per category in a configurable folder with a `parent` property, a `category` property on each member note, and an embedded Bases view in each category note listing subcategories (`parent` is this note) and notes (`category` is this note).
- Manual edits become pinned constraints for later runs, and renamed or merged categories keep their identity.

### Service and API
- `trove serve` binds localhost on a free port and writes a runtime file (port, token, pid, version) to the data folder.
- Every request carries the token as a bearer header. Requests with an unexpected `Host` header are refused (protects against DNS rebinding from browsers).
- Clients read the runtime file. If it's missing or its pid is dead, they start `trove serve` and wait for the file.
- The service exits after an idle timeout (default 15 min) with no requests and nothing queued.
- The OpenAPI spec is generated from the Rust types. The TypeScript types are generated from that spec.
- Endpoints: status; search (query, limit, filters, returning notes with best heading, score and which stages matched); related for a path; categories (hierarchy, members, start-here notes); category pins; suggestions list and accept or dismiss; orphans; index now (full or re-embed); prewarm; unload; sources list, add, remove; a server-sent events stream for indexing and setup progress.

### CLI
`setup`, `serve`, `status`, `sources add|remove|list`, `index [--full] [--reembed]`, `search` (with JSON output and switches to turn off graph, words or meaning), `unload`, `eval`. Commands talk to a running service if there is one, otherwise they open the index directly.

### Obsidian plugin
- Semantic quick switcher: a suggestion modal styled like `Ctrl+O`. Filename matches appear at once; Trove results merge in when ready; Enter opens the note at the matched heading. Opening it calls prewarm.
- Related: a section at the bottom of each note, styled like backlinks.
- Breadcrumb: the category path above the note title, read from properties.
- Commands: preview categories, apply categories (writes properties through Obsidian's frontmatter API), review link suggestions one at a time.
- First run: downloads the pinned `trove` binary for the OS, checks its SHA-256, runs setup with a progress notice.
- Uses Obsidian's own CSS classes and variables only.

### Raycast extension
- One "Search Vault" command using the same search endpoint, opening results through `obsidian://open` links.

### Build and release
- GitHub Actions on native runners (Windows, macOS arm64, Linux). Release assets per OS with SHA-256 hashes. Clients download a pinned version.
- Each epic is developed in its own git worktree.

## Testing Decisions

- A good test drives Trove from the outside the way a client would and checks observable results: API responses, CLI output, files in the sync store. Tests don't reach into internal modules, tables or data structures, so the internals can change freely.
- Tests use the real model and a real index on a small fixture vault in a temporary data folder. Only true externals are replaced (for example, the download host, served locally from fixture files).
- No sleeps. File-watcher and idle-timeout behaviour is tested with timeouts injected as configuration and by waiting on events from the server-sent events stream.

### Seam 1: the `trove` binary's public interface (HTTP API, with the CLI over it)
Covers indexing, search, related, graph, categories, suggestions, orphans, sync and lifecycle. Examples:
- A fixture vault with known topics returns the expected note for a paraphrased query that shares none of its words.
- An exact identifier in a note is found by a word query.
- A note one link away from the direct matches shows up through the graph rerank and disappears when the graph stage is turned off.
- Editing a note is reflected in search after the debounce; a file changed while the service was down is picked up on start.
- Categories for a fixture vault with two clearly separate topics put each topic's notes in a different category.
- Sync: indexing the same vault twice gives byte-identical vector files; deleting the local index and rebuilding makes zero model calls (visible through status); a second data folder (a second "device") imports vectors written by the first; conflict copies are removed; two devices' decision files merge.
- Lifecycle: the model unloads after its idle timeout and prewarm loads it; the service exits after its idle timeout; the runtime file's token is required.
- The generated TypeScript client is tested against a running `trove` through the same seam.

### Seam 2: the Embedder
Only for what the API doesn't expose:
- Reference vectors: a few fixed texts must produce stored vectors within tolerance. A llama.cpp bump ships only if this passes, or `vector_version` goes up.
- Cross-backend tolerance: CPU and Metal vectors for the reference texts agree at cosine 0.999 or higher; if not, the backend goes into `vector_version`.

### Eval
- A queries file of about 20 real "I needed this once" queries with the note each should find. Reports recall@5, recall@10 and MRR. Every search or graph change must keep or raise them.

### Prior art
The repo is new and has no tests yet. DigUp's test suite (reference vector tests, recall evals) is the model for seam 2 and the eval.

## Out of Scope

- Mobile. Obsidian mobile can't run Trove; it keeps the built-in search.
- PDFs, images, audio and video. The model supports them and they come after v1.
- Creating resources (YouTube transcript summaries, ffmpeg segments).
- GPU backends on Windows and Linux (CPU first, Vulkan later).
- LLM-written category names.
- Shortest path between notes and time-based edges.
- Automated UI tests for the Obsidian plugin and Raycast extension.
- Moving files into folders.
- A sync service of Trove's own. Trove relies on whatever syncs the vault.

## Further Notes

- Milestone estimates: embedding runtime 1.5 to 2 days; index and search with sync store 2.5 to 3.5 days; graph 4 to 5 days; service and API 1 to 1.5 days; Obsidian plugin 3 to 4 days; distribution and Raycast 2 days.
- Open questions: vector dimensions (768 or truncated to 256 through Matryoshka) decided by the eval; folder for category notes; Bases filter syntax for `this` against the installed Obsidian version; default timeouts (model 180 s, service 15 min, debounce 30 s).
- Risks to check early: whether a solid Leiden implementation exists in Rust (otherwise about a day to write one); macOS notarization and Windows SmartScreen for downloaded binaries (may need an Apple Developer account); Obsidian's community plugin rules require disclosing that a plugin downloads a binary, which only matters if the plugin is published.
- Credits: DigUp (MIT) for the download design, the llama.cpp pin and no-logits patch, the reference vector test, z-scored hybrid search and the eval approach.
- The original planning note is `Projects/Vault Search Spec.md` in the vault; this spec supersedes it.
