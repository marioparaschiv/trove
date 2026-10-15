Status: ready-for-agent

## Parent

`.scratch/trove/spec.md`

## What to build

Related notes end to end: `GET /related?path=` using personalised PageRank seeded by the note (spec "Graph"), and the Related section at the bottom of each note in Obsidian, styled like backlinks (spec "Obsidian plugin").

## Acceptance criteria

- [ ] For a fixture note, linked and semantically close notes rank above unrelated ones
- [ ] A note never appears in its own list
- [ ] The section updates when switching notes and uses only Obsidian styles

## Blocked by

- 09-graph-rerank
- 10-obsidian-quick-switcher
