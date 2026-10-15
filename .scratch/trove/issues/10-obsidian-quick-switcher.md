Status: done

## Parent

`.scratch/trove/spec.md`

## What to build

The Obsidian plugin's first feature (spec "Obsidian plugin", semantic quick switcher) on the generated TS client: a suggestion modal styled like `Ctrl+O`, filename matches at once, Trove results merged in when ready, prewarm on open, Enter opens the note at the matched heading. The plugin starts the service through the client. The `trove` binary path is a plugin setting for now (download comes in 18). Obsidian classes and CSS variables only.

## Acceptance criteria

- [ ] With the service stopped, opening the modal starts it and results arrive
- [ ] Filename matches show before Trove results
- [ ] Choosing a result opens the note at its heading
- [ ] No custom colours or fonts; the modal matches the built-in switcher under the default theme and one community theme
- [x] The plugin builds and lints clean

## Blocked by

- 06-tracer-meaning-search

## Comments

Layout: `clients/obsidian` (`@trove/obsidian`), a Bun workspace package beside `ts-client`. `src/main.ts` is the `Plugin` and only registers features (one `addCommand`/`addSettingTab` line each); each feature lives in its own file (`switcher.ts`). Files with no runtime `obsidian` import (`settings.ts`, `suggestions.ts`) hold the pure logic and are tested with `node --test` (the `obsidian` package has no runtime code, so anything importing it at runtime can't run under Node).
- HTTP: the renderer's `fetch` is blocked by CORS (the app origin calling `127.0.0.1` with an `Authorization` header needs a preflight the service doesn't answer), so `src/obsidian-fetch.ts` adapts Obsidian's `requestUrl` to `fetch`. Pass `connect({ binary, fetch: obsidianFetch })`; `ConnectOptions.fetch` was added to `@trove/client` for this (defaults to the global `fetch`, tested). Raycast doesn't need it.
- Settings: `data.json` is checked by `parseSettings` (unknown fields or wrong types throw, shown as a Notice and failing the plugin load). One field today, `binaryPath`; 18 replaces it with the downloaded binary. Add fields to `Settings`/`DEFAULT_SETTINGS`/`parseSettings` together.
- Vault folder: `FileSystemAdapter.getBasePath()`. Trove results are kept only when `source` is that folder (`isSameFolder`: resolved paths, case-insensitive on Windows). Opening the switcher also checks `GET /sources` and shows a Notice when the vault isn't registered; registering and indexing are manual until 18 (`trove sources add <vault>`, `trove index`).
- Switcher (`open-switcher` command, "Trove: Open quick switcher"): constructing it connects (starting `trove serve` when needed), fires `POST /prewarm` and checks sources. Filename matches use `prepareFuzzySearch` over Markdown paths without `.md` (recent files on an empty query); `POST /search` runs 150 ms after the last keystroke and its results are appended below the filename matches, minus notes already listed. Enter opens `path#heading#subheading` with `openLinkText` (Mod+Enter in a new tab). The first failure shows one Notice and the switcher keeps working on filenames. DOM uses `mod-complex`, `suggestion-content`, `suggestion-title`, `suggestion-note`; no `styles.css`. 07's "why" can go into an aux element (`suggestion-aux`) in `renderSuggestion`.
- Build: `bun run build` writes `clients/obsidian/main.js` (gitignored; CJS with `module.exports.default` the plugin class, `obsidian` and `electron` external, `@trove/client` bundled). `bun run --filter '*'` runs ts-client's `generate` before obsidian's typecheck (dependency order).

Verified here: `bun run typecheck|lint|test|build` in `clients/` (from a state with no generated `schema.ts`); a throwaway script ran `connect` + `obsidianFetch` (with `requestUrl` stubbed over Node's `fetch`) against a real `trove serve` on a copy of the fixture vault: service started, prewarm, sources, search returned `Engineering/Retry backoff.md` at `Retry backoff > Exponential delays`, a 400 surfaced its `ErrorBody`. The bundle loads under Node with a stub `obsidian` and exports the plugin class.

Not verified (no Obsidian on the build server). Install for manual testing: copy `clients/obsidian/main.js` and `manifest.json` into `<test vault>/.obsidian/plugins/trove/`, enable the plugin, set the binary path, run `trove sources add <test vault>` and `trove index`. Then check:
- With the service stopped, opening the switcher starts `trove serve` and Trove results arrive (`requestUrl` really reaches the service).
- Filename matches show at once; Trove results appear below them.
- Enter opens the note scrolled to the matched heading; Mod+Enter opens it in a new tab.
- The modal looks like `Ctrl+O` under the default theme and one community theme, light and dark.
- An unregistered vault or an empty binary path shows one Notice and filename search still works.
