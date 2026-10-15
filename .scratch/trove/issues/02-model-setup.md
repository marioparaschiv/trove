Status: ready-for-agent

## Parent

`.scratch/trove/spec.md`

## What to build

`trove setup [--json]` puts the pinned model file (spec "Embedding runtime") into the data folder, following spec "Model download": Hugging Face cache first, resumable `.partial` download with `Range` kept across redirects, size and SHA-256 check before rename, backoff and retry limits, waiting when offline, JSON-lines progress events.

## Acceptance criteria

- [ ] Downloads from a local test HTTP server serving a small fixture file with its size and hash (the only replaced external)
- [ ] An interrupted download resumes from the partial size (the server sees a `Range` request)
- [ ] A matching file in a fake HF cache folder is copied with no network request
- [ ] A corrupted download is deleted and retried, giving up after 3 bad checks with an `error` event
- [ ] `--json` emits the spec's event kinds in order
- [ ] Running `setup` again with a good file present does nothing

## Blocked by

- 01-core-contracts
