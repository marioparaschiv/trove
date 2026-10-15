Status: ready-for-agent

## Parent

`.scratch/trove/spec.md`. Read it once; every issue references its sections instead of repeating them.

## What to build

Prefactor that lets later slices run in parallel without colliding. Split the workspace into a core library crate and the `trove` binary crate. Put in place the contracts every later slice builds on:
- The local index schema, version 1, with every table from spec "Local index", applied on open.
- Per-OS data folder and a validated config (timeouts and store folder name, defaults from spec "Further Notes"), failing loudly on bad values.
- The search pipeline as an ordered list of independent stages behind one interface (spec "Search pipeline"), so meaning, words and graph stages land in separate modules.
- A CLI skeleton (subcommands from spec "CLI") that later slices fill in.

No llama.cpp, no HTTP, no search logic.

## Acceptance criteria

- [ ] `cargo build`, `cargo clippy` and `cargo test` pass on the workspace
- [ ] Opening an empty data folder creates the schema; reopening is a no-op; schema version is readable
- [ ] Invalid config values fail with a message naming the field
- [ ] A pipeline built from two test-only stages runs them in order

## Blocked by

None - can start immediately
