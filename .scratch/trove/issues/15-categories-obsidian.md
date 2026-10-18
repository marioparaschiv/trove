Status: done

## Parent

`.scratch/trove/spec.md`

## What to build

Categories in Obsidian (spec "Categories", plugin side): "Preview categories" and "Apply categories" commands; category notes in a configurable folder with `parent`; `category` on member notes, written through Obsidian's frontmatter API; an embedded Bases view per category note (subcategories and notes); and the breadcrumb above the title. Verify the Bases `this` filter syntax against the installed Obsidian and record it in the spec.

## Acceptance criteria

- [x] Preview shows the changes and writes nothing
- [x] Apply creates category notes and properties and moves no files
- [x] Each category note's Bases view lists its subcategories and notes
- [x] The breadcrumb shows the category path and links each level
- [x] Applying twice changes nothing the second time

## Blocked by

- 14-categories-core
- 10-obsidian-quick-switcher

## Comments

Plugin (`clients/obsidian`).
- `src/category-notes.ts` is the pure logic, tested in `category-notes.test.ts`: `planCategories(categories, {folder, inVault, state})` gives the changes (`create`, `set-parent`, `set-category`); `CATEGORY_NOTE_BODY`; `linkTo(path)` (`[[Folder/Name]]`, the full path without `.md`); `linkTarget(value)` (link path of a property value, first of a list); `categoryChain`; `describeChange` (preview lines). A category note is `<folder>/<name>.md` named after the category (characters Obsidian refuses become spaces; a taken name gets ` 2`, compared case-insensitively). Only categories with a note from this vault (themselves or below) are kept. A change is planned only where the vault differs, comparing resolved paths (`state` resolves each property link through `metadataCache.getFirstLinkpathDest`), so applying twice changes nothing the second time. Top-level category notes are never given or stripped of a `parent`; stale category notes and `category` values pointing at categories no longer returned are left alone (17's pins decide what manual edits mean).
- `src/categories.ts`: commands "Preview categories" (a `Modal` listing the changes, writes nothing) and "Apply categories" (creates the folder and notes with `vault.create`, then sets `parent` and `category` through `fileManager.processFrontMatter`; moves no file). Both fetch `GET /categories?exclude_folder=<category folder>` and keep notes whose `source` is the vault. `noteState(app, path)` is shared with the breadcrumb.
- `src/breadcrumb.ts`: in each Markdown view, a `div.view-header-title-parent` of `span.view-header-breadcrumb` (separated by `span.view-header-breadcrumb-separator` "/") placed before `.inline-title` (`.cm-sizer` in editing views, `.mod-header` in reading view; first child there when the inline title is off). It follows the note's `category` (or `parent`, for a category note) and each category note's `parent` to the top; each level opens its category note (Mod-click: new tab). Refreshes on `file-open`, `layout-change` and `metadataCache` `changed`; hidden when there is no category.
- Setting `categoryFolder` (default `Categories`, validated by `parseFolder`/`parseSettings`, editable in the settings tab).

Core. `GET /categories` takes an optional `exclude_folder` (`graph::ExcludedFolder`, a `/`-separated folder inside each source; `..`, `.` and empty are a 400). `VaultGraph::load_without(conn, excluded)` builds the graph as if those notes didn't exist, so links to them are unresolved and add no link, co-citation or coupling edges. Without it the plugin's own output fed back: on the categories fixture, writing the category notes and `category` properties then re-indexing changed the hierarchy; with it the response is identical (`category_notes_in_the_excluded_folder_leave_the_categories_unchanged` in `crates/trove/tests/categories.rs`, which writes exactly what the plugin writes). The CLI `trove categories` has no flag for it. Search and related notes still see category notes.

Bases (spec "Categories" records it). Checked against Obsidian's help ("Bases syntax" > "Access properties with `this`", "Create a base" > "Embed a base as a code block", from the obsidian-help repo): in a base embedded in a note `this` is that note, and a link compares equal to the file it resolves to. Each category note has `## Subcategories` with a `base` block filtered by `list(parent).contains(this)` and `## Notes` with `list(category).contains(this)`, one table view each.

Verified here: `bun run typecheck|lint|test|build` in `clients/`; the plan logic and idempotence under `node --test`; a throwaway script ran the plugin's `GET /categories?exclude_folder` and `planCategories` through `@trove/client` against a real `trove serve`. Not verified (no Obsidian on the build server). Manual checks:
- The Bases `this` syntax in the installed Obsidian: each category note's two tables list its subcategories and its notes.
- Preview lists the changes and writes nothing; Apply creates `Categories/*.md` with `parent`, sets `category` on members, moves no file; running Apply again says "categories are up to date" (also after Trove re-indexes the changed notes).
- The breadcrumb shows above the title in Live Preview, Source mode and Reading view, with and without the inline title, links each level, updates after editing `category`, and looks like the header breadcrumbs under the default theme and one community theme.
