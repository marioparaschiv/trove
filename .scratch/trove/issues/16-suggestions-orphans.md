Status: done

## Parent

`.scratch/trove/spec.md`

## What to build

Link suggestions and orphans (spec "Graph") with the synced decisions files (spec "Sync store", decisions): `GET /suggestions`, accept or dismiss, `GET /orphans`, and an Obsidian command that walks suggestions one at a time (accept inserts the link, dismiss records it).

## Acceptance criteria

- [x] Two similar, unlinked fixture notes are suggested; linked pairs never are
- [x] A dismissed suggestion never returns, including after rebuilding the local index
- [x] Two data folders' decision files merge, latest timestamp winning
- [x] Orphans lists only notes with no links in or out
- [x] Accepting in Obsidian inserts a wikilink and the pair stops being suggested

## Blocked by

- 09-graph-rerank
- 10-obsidian-quick-switcher
- 12-sync-store

## Comments

Core: `crates/trove-core/src/suggestions.rs` (`suggestions`, `decide`, `orphans`), handlers in `http/suggestions.rs`, decisions files in `sync_store/decisions.rs`. Tests: `crates/trove/tests/suggestions.rs` over HTTP on a copy of `tests/fixtures/related-vault` (unchanged; Espresso links to Lisbon, Pour over is close to Espresso with no link).

API.
- `GET /suggestions?source=<folder>[&limit=N][&exclude_folder=F]` returns `{suggestions: [{note, target, similarity}]}` (`note`/`target` are `{source, path, title}`, `CategoryNote`; `note` is the one whose path sorts first; accepting means linking `note` to `target`), most similar first, default limit 10. A pair is two notes of that source with two sections at least `SUGGESTION_FLOOR` (0.78) similar, read from the stored `semantic` edges (each section's 10 nearest), with no `link`/`property_link` either way that resolves (`VaultGraph.links`, new: resolved note-to-note links), and with no decision. Measured: Espresso/Pour over 0.81; on the categories fixture same-topic pairs 0.77 to 0.85, unrelated sections up to 0.73 (09). The eval on a real vault sets the floor.
- `POST /suggestions/decisions {source, note, target, state}` (204; `state` is `accepted` or `dismissed`, and either hides the pair for good; 404 unknown source or a note not indexed, 400 the same note twice or a bad body). Which one won is not visible over HTTP, so the latest-wins test reads the merged state with `SyncStore::suggestion_states` after two data folders decide through the API.
- `GET /orphans?source=<folder>[&exclude_folder=F]` returns `{notes: [CategoryNote]}` by path: notes with no resolved link or property link in or out (tags and semantic edges don't count).
- `exclude_folder` (see 15) leaves the plugin's category notes out; without it every categorised note links to its category note and would stop being an orphan.

Decisions files (spec "Sync store"): `<source>/<store_folder>/decisions/<device id>.jsonl`, one JSON object per line: `{"at": RFC 3339 with nanoseconds, "type": "suggestion", "notes": [a, b] (paths, sorted), "state"}`. The device id is `meta.device_id` of the local index, so a rebuilt index starts a new file (harmless: all files are merged). A write rewrites the device's file whole through a temporary file and rename (`sync_store::write_atomically`, now shared with vector files), under the index lock so two decisions can't race. Reading merges every `*.jsonl` in the folder; per pair the latest `at` wins, ties broken by device id then line. A malformed line is an error naming the file and line (500), not skipped. 17: add a variant to `sync_store::Decision` (tagged by `type`) and a reader beside `suggestion_states`. Gotcha: `SyncStore::remove_conflict_copies` deletes conflict copies anywhere in the store, decisions included; one file per device means sync tools shouldn't make them.

Plugin: command "Review link suggestions" (`src/link-suggestions.ts`), a `Modal` that loads up to 50 suggestions (`exclude_folder` = the category folder) and shows one at a time with Link / Dismiss / Skip. Link appends `[[<fileToLinktext>]]` as its own paragraph at the end of `note` with `vault.process` (`appendLink` in `src/links.ts`, tested) and records `accepted`; Dismiss records `dismissed`. No orphans UI (the ticket asks only for the endpoint).

Verified: `cargo build`, `cargo clippy --all-targets` (no warnings), `cargo test`; `bun run typecheck|lint|test|build` in `clients/`; a throwaway script made the plugin's three calls through `@trove/client` against a real `trove serve`. Not verified (no Obsidian here). Manual checks: running "Review link suggestions" on a vault shows Espresso → Pour over style pairs; Link adds the wikilink at the end of the note and, after Trove re-indexes, the pair is not suggested again; Dismiss hides it for good, also on a second device syncing the vault (`.trove/decisions/` must sync).
