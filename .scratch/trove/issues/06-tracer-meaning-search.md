Status: ready-for-agent

## Parent

`.scratch/trove/spec.md`

## What to build

Tracer bullet through every layer. `trove sources add <folder>` registers a vault; `trove index` scans `.md` files (skipping folders per spec "Indexing"), chunks by heading, embeds, and stores chunks and vectors. `trove search` and `POST /search` return notes by meaning as the first pipeline stage (spec "Search pipeline" step 1: z-scored, collapsed to notes with the best heading). Also `POST /prewarm`, `POST /unload`, the sources endpoints, and model state in `/status`.

Creates the seam 1 fixture vault (a handful of notes on clearly separate topics) that later slices extend.

## Acceptance criteria

- [ ] A paraphrased query sharing no words with a fixture note returns it first, through CLI and HTTP
- [ ] Results carry the best chunk's heading path
- [ ] A query matching nothing returns an empty list
- [ ] `.obsidian` and trash folders are never indexed
- [ ] Prewarm loads the model and unload frees it, both visible in `/status`

## Blocked by

- 03-embedder
- 04-service-lifecycle
