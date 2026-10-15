Status: ready-for-agent

## Parent

`.scratch/trove/spec.md`

## What to build

The Raycast extension (spec "Raycast extension"): one "Search Vault" command on the generated TS client, opening results with `obsidian://open` links.

## Acceptance criteria

- [ ] Typing a query lists notes with their heading
- [ ] Enter opens the note in Obsidian
- [ ] The service is started if it isn't running
- [ ] The extension builds and lints clean

## Blocked by

- 06-tracer-meaning-search
