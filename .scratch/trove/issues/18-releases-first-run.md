Status: ready-for-agent

## Parent

`.scratch/trove/spec.md`

## What to build

Releases and first run (spec "Build and release", and spec "Obsidian plugin" first run). Tagged builds publish per-OS `trove` binaries with SHA-256 hashes. The plugin downloads the pinned binary for the OS, checks it, runs `setup` and shows progress from its JSON events. Record what macOS notarization and Windows SmartScreen need for downloaded binaries in the spec's Further Notes.

## Acceptance criteria

- [ ] A tag produces release assets for Windows, macOS arm64 and Linux with hash files
- [ ] A fresh plugin install with no binary downloads, verifies and starts `trove`
- [ ] A binary with a wrong hash is rejected with a visible error
- [ ] Setup progress appears as an Obsidian notice

## Blocked by

- 05-ci
- 10-obsidian-quick-switcher
