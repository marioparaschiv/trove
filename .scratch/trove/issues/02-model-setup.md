Status: done

## Parent

`.scratch/trove/spec.md`

## What to build

`trove setup [--json]` puts the pinned model file (spec "Embedding runtime") into the data folder, following spec "Model download": Hugging Face cache first, resumable `.partial` download with `Range` kept across redirects, size and SHA-256 check before rename, backoff and retry limits, waiting when offline, JSON-lines progress events.

## Acceptance criteria

- [x] Downloads from a local test HTTP server serving a small fixture file with its size and hash (the only replaced external)
- [x] An interrupted download resumes from the partial size (the server sees a `Range` request)
- [x] A matching file in a fake HF cache folder is copied with no network request
- [x] A corrupted download is deleted and retried, giving up after 3 bad checks with an `error` event
- [x] `--json` emits the spec's event kinds in order
- [x] Running `setup` again with a good file present does nothing

## Blocked by

- 01-core-contracts

## Comments

- Code: `crates/trove-core/src/model_setup.rs`, `crates/trove/src/commands/setup.rs`, tests in `crates/trove/tests/setup.rs` (local HTTP server, fake HF cache).
- Pin: `model_setup::{MODEL_REPO, MODEL_REVISION, MODEL_FILE, MODEL_SIZE, MODEL_SHA256}` (hash as lowercase hex). The effective model is `config.model` (`ModelFile`); `config.model.path_in(&data_dir)` is `<data>/models/<file>`, the path the embedder (03) should load. A file there has always passed its size and hash check.
- Config: optional `[model]` table (`repo`, `revision`, `file`, `size`, `sha256`, all required together) and `download_retry_delay` (humantime, default `2s`, doubles up to 60 s). Both exist so tests can use a small fixture and short delays.
- Download host comes from `HF_ENDPOINT` (default `https://huggingface.co`), cache from `HF_HUB_CACHE`, `HF_HOME/hub`, `~/.cache/huggingface/hub`.
- `model_setup::setup(&DataDir, &Config, &mut dyn FnMut(Event))` is blocking and always ends with a `Done` or `Error` event; the service can forward `Event` (serde, tagged by `event`) over SSE from a blocking task. Event fields: `file {name, bytes, from: cache|download}`, `progress {name, bytes, total}`, `waiting`, `verifying {name}`, `retry {message, seconds}`, `done {path}`, `error {message}`. A rerun with the model present emits only `done`.
- "No network" means the host can't be resolved or connected to; setup then emits `waiting` once and keeps polling without counting a failure. The body has no idle timeout (ureq only has total timeouts), so a stalled transfer waits on the OS.
