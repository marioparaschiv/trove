Status: done

## Parent

`.scratch/trove/spec.md`

## What to build

Related notes end to end: `GET /related?path=` using personalised PageRank seeded by the note (spec "Graph"), and the Related section at the bottom of each note in Obsidian, styled like backlinks (spec "Obsidian plugin").

## Acceptance criteria

- [x] For a fixture note, linked and semantically close notes rank above unrelated ones
- [x] A note never appears in its own list
- [ ] The section updates when switching notes and uses only Obsidian styles

## Blocked by

- 09-graph-rerank
- 10-obsidian-quick-switcher

## Comments

API: `GET /related?source=<folder>&path=<note>[&limit=N]` returns a `SearchResponse` (`results: [{source, path, title, heading_path, score, why}]`), best first, at most `limit` (default 10). `score` is the note's rank over the best related note's (the first is 1.0); `why` is always `["graph"]`. 404 when the note isn't indexed, 400 for a missing parameter or `limit=0`. Deviation: the ticket says `?path=`; `source` is required too, since paths are per source and Trove can hold several. Clients pass the registered source string (find it with `GET /sources` and `isSameFolder`), as they get it in search results.

Core: `crates/trove-core/src/related.rs` (`related::related(&Mutex<Index>, &RelatedRequest)`), handler `http/related.rs`. Personalised PageRank (09's `Graph`) seeded by the note's note node alone, weight 1; a note's rank is its note node plus its sections (`VaultGraph::note_ranks`, now shared with `GraphRerank`), and each result points at its best-ranked section (the linked heading or block, or the semantically closest section). The note itself and notes the walk never reaches (rank 0) are left out; there is no `MIN_RANK`-style cut beyond `limit`. Nothing is cached; the graph is loaded per request like search.

Measured. On `tests/fixtures/related-vault` (new, 6 one-topic notes: `Coffee/Espresso.md` links to `Travel/Lisbon.md`, `Coffee/Pour over.md` is close in meaning with no link, three unrelated notes), Espresso's list is Lisbon 1.0, Pour over 0.57, and nothing else; unrelated notes have no path to it and are absent. On the main fixture, the hub's list is its six targets (linked ones 0.94 to 1.0, the shared tag 0.27). For `Engineering/Retry backoff.md`, the notes co-cited with it by the hub (Kyoto, Index funds, ...) rank above the hub that links to it, since each co-citation edge weighs 1.0; the eval on the real vault should tune co-citation against direct links.

Tests: `crates/trove/tests/related_notes.rs` (linked and close notes rank above unrelated ones; no note lists itself, for every note; 404 and 400s), over HTTP against an indexed copy of the vault. `common::copy_of(vault)` (copies any fixture vault; `vault_copy()` uses it) and `common::percent_encode` (moved from `meaning_search.rs`).

Plugin: `clients/obsidian/src/related.ts` (`RelatedNotes`, one `register()` line in `main.ts`). Each Markdown view gets one `div.embedded-backlinks > div.backlink-pane` with a `tree-item-self` header ("Related notes" and a `tree-item-flair` count) and a `search-result-container` of `tree-item search-result` items (`search-result-file-title` with the note title, `search-result-file-matches > search-result-file-match` with the heading path); "No related notes." in `search-empty-state`. It is appended where Obsidian puts backlinks: `.cm-sizer` in editing views, `previewMode.containerEl .mod-footer` in reading view. `file-open` and `layout-change` re-run it over every Markdown leaf: the element moves to the current mode's container, it fetches only when the view's note changed, and a response for a note no longer shown is dropped. A 404 (note not indexed yet) shows the empty state; other failures show `Trove: <message>` in the empty state, not a Notice. Clicking opens `path#heading` (Mod-click in a new tab). The list does not refresh when the note is re-indexed, only when the view's note changes. Shared helpers moved to `src/connection.ts` (`connectTo`, `vaultSource`, `noteLink`, `failure`, `message`); the switcher uses them too.

Not verified (no Obsidian here). Manual checks:
- The section appears under each note in Live Preview, Source mode and Reading view, below the core backlinks section when that is on, and survives switching modes.
- It updates when switching notes in a tab and in split panes; clicking a result opens the note at the heading.
- It looks like the backlinks section under the default theme and one community theme, light and dark, with no styles of its own; the core backlinks section still works with a second `embedded-backlinks` element beside it.
