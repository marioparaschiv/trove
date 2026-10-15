Status: ready-for-agent

## Parent

`.scratch/trove/spec.md`

## What to build

Link suggestions and orphans (spec "Graph") with the synced decisions files (spec "Sync store", decisions): `GET /suggestions`, accept or dismiss, `GET /orphans`, and an Obsidian command that walks suggestions one at a time (accept inserts the link, dismiss records it).

## Acceptance criteria

- [ ] Two similar, unlinked fixture notes are suggested; linked pairs never are
- [ ] A dismissed suggestion never returns, including after rebuilding the local index
- [ ] Two data folders' decision files merge, latest timestamp winning
- [ ] Orphans lists only notes with no links in or out
- [ ] Accepting in Obsidian inserts a wikilink and the pair stops being suggested

## Blocked by

- 09-graph-rerank
- 10-obsidian-quick-switcher
- 12-sync-store
