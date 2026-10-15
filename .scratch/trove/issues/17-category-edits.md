Status: done

## Parent

`.scratch/trove/spec.md`

## What to build

Manual category edits win (spec "Categories", last point). Renames, merges, moves, and manual `category` or `parent` edits become pinned constraints, stored in the decisions files, sent through `PUT /categories/pins`, and respected by later runs. Renamed or merged categories keep their identity.

## Acceptance criteria

- [x] A renamed category keeps its name after the next run
- [x] A note moved to another category by hand stays there after the next run
- [x] Two merged categories stay merged
- [x] Pins made in one data folder hold in a second one on the same vault

## Blocked by

- 15-categories-obsidian
- 16-suggestions-orphans

## Comments

Core: `sync_store::CategoryPin` and readers in `sync_store/decisions.rs`; `categories::pin` and `CategoryPins` in `categories.rs`; pin resolution in `categories/pins.rs`; applying pins to the Leiden hierarchy in `categories/tree.rs`; handler in `http/categories.rs`. Tests: `crates/trove/tests/category_pins.rs` over HTTP and the CLI on a copy of `tests/fixtures/categories-vault` (unchanged).

Identity. Leiden ids change with the vault, so a pinned category is identified by its name (the plugin's category note file name) and found in each run by an anchor: the notes it held when pinned, subcategories included. A pinned category claims every community most of whose notes are in its anchor and most of each of whose subcategories' notes too (so a pin on a subcategory doesn't claim its parent), best Jaccard first, ties by name; a pin that claims a community can't claim its descendants. Several communities claimed by one pin are one category (a merge). Unclaimed communities keep their c-TF-IDF name, computed against their final siblings.

API. `PUT /categories/pins {source, pins: [CategoryPin]}` (204; 404 unknown source; 400 an empty name or a bad body). Pins, tagged by `type`:
- `{type: "category", name, notes, replaces?}`: pins a category under `name` with `notes` (source paths) as its anchor. `replaces` lists names this one replaces (a rename lists the old name, a merge the merged one); pins naming a replaced name then mean this one.
- `{type: "parent", category, parent}`: moves a pinned category under another pinned one, or to the top level for `null`. Without it, a pinned category sits where its shallowest claimed community does.
- `{type: "note", path, category}`: the note is in that pinned category, whatever Leiden says.
Each pin is stored as a decision `{"at", "type": "category_pin", "pin": {...}}` (one PUT is one rewrite of the device's file, all with one `at`, later lines winning). `SyncStore::decisions()` now reads and orders every decision once; `suggestion_states` and the new `category_pins` (oldest first) read from it, and `record_decisions` writes several. Resolution folds pins oldest first: per name the latest `category` pin or replacement wins, per category the latest `parent` pin, per note the latest `note` pin; a pin naming something unknown or unresolvable drops the earlier pin on that key. Notes not indexed (or in `exclude_folder`) are ignored, so a renamed note loses its `note` pin. A parent loop sends the category it closes on to the top level. A community left with no notes of its own and one subcategory is replaced by it. `GET /categories` and `trove categories` read every source's pins; `id` is now the position in the response (still deterministic, still not stable across vault changes).

Plugin (`clients/obsidian`): `src/category-pins.ts` is the pure logic (`notesIn`, `editPins`, `renamePin`, `mergePin`, `renamedStates`; tested in `category-pins.test.ts`), `src/category-edits.ts` the wiring. It keeps every note's resolved `category`/`parent` (built on layout ready, updated on metadata `changed`, `rename`, `delete`):
- Renaming a category note inside the category folder sends a `category` pin with the new name, the notes that were under the old one, and `replaces: [old name]`.
- Setting `category` of the active note to a category note sends a `category` pin for it (notes in it now) and a `note` pin; changing `parent` of the active category note sends `category` pins for it and its new parent plus a `parent` pin (`null` when removed). Only the active note counts, since other notes change when sync brings another device's edits; removing `category` pins nothing.
- Command "Merge category into..." (active file a category note): pins the union under the target's name replacing the source's, then repoints `category`/`parent` links from the source to the target and trashes the source note.
- Apply categories writes through `CategoryEdits.setLink`, which records the value first, so its own writes are never taken for manual edits.

Verified: `cargo build`, `cargo clippy --all-targets` (no warnings), `cargo test`; `bun run typecheck|lint|test|build` in `clients/`. Not verified (no Obsidian here). Manual checks:
- Apply categories, rename a category note in the file explorer, let Trove re-index (or edit a note), then Preview categories: no change, the renamed note kept, no note with the old name created.
- In a note's properties set `category` to another category note, re-index, Preview: no change for that note. Same for a category note's `parent`, and for removing `parent` (category moves to the top level).
- On a category note run "Merge category into...", pick another: members and subcategories now link the target, the source note is in the trash, and Preview after a re-index shows no change.
- With a second device syncing the vault (`.trove/decisions/` synced), Preview there shows the same names and placements.
