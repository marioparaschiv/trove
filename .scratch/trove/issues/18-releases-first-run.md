Status: done

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

## Comments

Release (`.github/workflows/release.yml`, on tags `v*`; never run yet). Job `build`, one leg per platform (`windows-2025`, `macos-15`, `ubuntu-24.04`): fails unless the tag is `v<workspace version>`, then `cargo build --release --locked --package trove` and `scripts/package-release.sh <platform> <version> target/release dist`, uploaded as an artifact. Job `publish` (needs all three; `contents: write`): fails unless `clients/obsidian/manifest.json` has the same version, bundles the plugin with the pin (below), then `gh release create v<version> --generate-notes` with the archives, their `.sha256` files, `main.js` and `manifest.json`. No caches: a release builds llama.cpp cold (about 10 min on Linux, per 05). Bump `version` in the root `Cargo.toml` and in `manifest.json` together before tagging.

Assets: `trove-<version>-<platform>.tar.gz` (`linux-x86_64`, `macos-arm64`) or `.zip` (`windows-x86_64`), files at the archive root, plus `<archive>.sha256` in `sha256sum` format. Linux ships `trove`, `libllama.so*`, `libggml*.so*` (every `libggml-cpu-*` variant; the `.so`, `.so.0` and versioned names are copies, as build.rs leaves them). macOS ships `trove` alone. Windows ships `trove.exe`, `llama.dll`, `ggml*.dll` and the VC++ runtime and OpenMP DLLs (`msvcp140`, `vcruntime140*`, `vcomp140`, ...) found with `vswhere` in the Visual Studio redistributables. The script checks every required file is there and fails otherwise.

Plugin first run (`clients/obsidian`).
- Pin: `scripts/release-pin.sh <version> <owner/repo> <dist>` prints `{version, repository, assets: {<platform>: {file, sha256}}}` from the `.sha256` files; the release job passes it as `TROVE_RELEASE_PIN` to `bun run bundle` (`bun build --env 'TROVE_RELEASE_*'` inlines it). So the plugin from release `vX` pins `vX`'s archives and hashes, with no hashes in git. A local build has no pin. `src/release.ts` parses and checks it (`parseReleasePin`).
- `src/first-run.ts` `prepareTrove`, started on layout ready, gives the binary promise the switcher and related notes await: the `binaryPath` setting when set, else `<data folder>/bin/<version>/trove[.exe]`, downloaded with `requestUrl` from `https://github.com/<repository>/releases/download/v<version>/<file>` when missing, under a "downloading" notice. `installBinary` checks the SHA-256 first (a mismatch throws "`<file>` has SHA-256 X, expected Y; it was not installed", shown as a persistent Notice), unpacks with `tar` into a temporary folder beside it (Windows uses `System32\tar.exe`, which also reads zip), checks the binary is there, then renames the folder into place, so a version folder that exists is always good. The data folder keeps the binary per device and out of the synced vault.
- Then `trove setup --json` (`src/setup.ts`, `runSetup`), each event shown in one Notice (`describeSetupEvent`); a rerun with the model present prints only `done`, which shows nothing. Then, if the vault isn't a source, `POST /sources` and `POST /index` under an "indexing this vault" notice that ends with the note count.
- Deviation: 10 said 18 replaces `binaryPath`; it stays as an override (empty means the release), since development builds have no pin and need it.
- Core: `model_setup::Event` and `Source` derive `ToSchema` and are OpenAPI components `SetupEvent` and `SetupSource`, so the plugin's type comes from the generated schema; `parseSetupEvent` still checks each line.
- Scripts: obsidian `build` is `tsc && bun run bundle`; `bundle` alone needs no generated schema (type imports are dropped), which the release job relies on. `test` runs `cargo build` first, like ts-client.

Spec: Further Notes gained what macOS Gatekeeper/notarization and Windows SmartScreen need for downloaded binaries.

Verified here (Linux): `cargo build --release` then `scripts/package-release.sh linux-x86_64 0.1.0 target/release <dist>`; the archive unpacked into an empty folder runs with a bare environment (`env -i`, no `LD_LIBRARY_PATH`; `RUNPATH $ORIGIN` on `trove` and `libllama.so.0`), indexes the fixture vault (35 chunks embedded, so the CPU backends load) and searches. `scripts/release-pin.sh` on that output. Both workflows pass `bunx @action-validator/cli`. The plugin tests package `target/debug` with the same script, install it through `installBinary` (download replaced by reading the archive), and start it with `connect` (`/status` answers with the release version); a wrong hash is rejected and leaves nothing behind; `runSetup` runs the real `trove setup --json` against a local stand-in for Hugging Face serving a small fixture model and reports `file`, ..., `verifying`, `done`, then only `done`; a bad `HF_ENDPOINT` rejects with setup's error. The bundle with a pin inlines it; it loads under Node with a stub `obsidian` and exports the plugin class.

Not verified:
- A real tag run: the macOS and Windows legs (`7z`, `vswhere -find` globs, `cygpath`, CRLF: `scripts/.gitattributes` forces LF), `upload-artifact`/`download-artifact` and `gh release create`. Test with a throwaway tag on a fork.
- In Obsidian (manual): a fresh install with an empty `binaryPath` downloads, verifies and starts trove on Windows, macOS and Linux, then shows setup progress and indexing notices; `requestUrl` follows GitHub's redirect to its CDN for a ~20 MB archive; a tampered `.sha256` pin (edit `main.js`) gives the persistent "not installed" Notice; the downloaded `trove` runs on macOS without a Gatekeeper prompt and on Windows without SmartScreen.
