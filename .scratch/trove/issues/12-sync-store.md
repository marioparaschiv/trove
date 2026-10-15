Status: ready-for-agent

## Parent

`.scratch/trove/spec.md`

## What to build

The sync store for vectors (spec "Sync store": store folder, vector files, deterministic and atomic writes, reading, cleanup). Decisions files come in 16. The local index becomes rebuildable from the vault plus the store.

## Acceptance criteria

- [ ] Indexing the same vault twice gives byte-identical vector files
- [ ] Deleting the local index and rebuilding makes zero model calls (visible in `/status`)
- [ ] A second data folder on the same vault (a second "device") imports the first's vectors without embedding
- [ ] A file with another `vector_version` is ignored and replaced on the next embed
- [ ] Syncthing and Dropbox style conflict copies in the store folder are removed
- [ ] The store folder name is configurable and never indexed

## Blocked by

- 08-change-detection
