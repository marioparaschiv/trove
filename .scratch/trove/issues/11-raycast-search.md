Status: done

## Parent

`.scratch/trove/spec.md`

## What to build

The Raycast extension (spec "Raycast extension"): one "Search Vault" command on the generated TS client, opening results with `obsidian://open` links.

## Acceptance criteria

- [ ] Typing a query lists notes with their heading
- [ ] Enter opens the note in Obsidian
- [ ] The service is started if it isn't running
- [x] The extension builds and lints clean

## Blocked by

- 06-tracer-meaning-search

## Comments

Layout: `clients/raycast` (workspace package `trove`; the name doubles as the Raycast extension name). `package.json` is the Raycast manifest: one `view` command `search-vault`, one required extension preference `troveBinary` (type `file`), `platforms: ["macOS", "Windows"]` (Raycast runs on nothing else). `src/search-vault.tsx` is the command, `src/obsidian-url.ts` the only pure logic, tested in `src/obsidian-url.test.ts` (`node --test`).
- Flow: the command calls `connect({ binary: troveBinary })` from `@trove/client` on open, so the service starts as soon as the command shows (no prewarm; the first search loads the model). Then each query (throttled, aborting the previous request) is a `POST /search {query}`; a non-200 throws its `ErrorBody.error`, and `usePromise` shows it as a failure toast ("Couldn't start Trove" / "Search failed"). Each result is a `List.Item`: title = note title, subtitle = heading path joined with ` > `, accessory = path; Enter runs `Open in Obsidian`.
- `obsidian://open?vault=<basename(source)>&file=<path without .md>#<last heading>`, encoded with `encodeURIComponent` (`URLSearchParams` writes spaces as `+`, which Obsidian keeps). Assumption: the vault is named after the registered source folder, i.e. a source is a whole vault (spec: one vault, one source). Registering a subfolder of a vault as a source would give a wrong vault name; that would need the vault name in the API.
- Scripts: `build` is `ray build --environment dist --output dist --non-interactive`; it bundles with esbuild (`@trove/client`, `openapi-fetch` and `@raycast/utils` inlined; `@raycast/api` and `react` provided by Raycast), writes the gitignored `raycast-env.d.ts` (the `Preferences` types) and runs `tsc`, so `typecheck` is `bun run build`. `ray build` needs `./node_modules/.bin/tsc` inside the extension folder, hence `typescript` (same range as the root) and `@types/react` (pinned to the version `@raycast/api` uses) as the package's devDependencies. It relies on `bun run --filter '*'` running `@trove/client` first (it does, by dependency order), which generates `src/schema.ts`. `lint` is Biome like every package; `ray lint` also passes the manifest and icon checks and skips its ESLint/Prettier checks since neither is installed.
- Load it in Raycast: build `trove` (`cargo build`), then in `clients/raycast` run `bunx ray develop` (or `bun run build` and import the folder in Raycast's "Import Extension"), and set "Trove Binary" to the `trove` executable when Raycast asks. Register a vault first with `trove sources add <vault>` and `trove index`.

Not verified (no Raycast on the build server; the first three criteria need these manual checks on macOS or Windows):
- Typing a query lists notes with title and heading path.
- Enter opens the note in Obsidian at the heading (confirm `file=<path>#<heading>` lands on the heading).
- With no `trove serve` running, opening the command starts it (`runtime.json` appears in the data folder).
- A wrong "Trove Binary" or a failed search shows a failure toast.
