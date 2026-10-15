Status: ready-for-agent

## Parent

`.scratch/trove/spec.md`

## What to build

Keep the index current (spec "Indexing": change detection and chunk text hashes). A file watcher with debounce inside `trove serve`, a scan on start by mtime then hash, re-embedding only chunks whose text hash changed, vector reuse on rename, removal on delete, and `index --full` / `--reembed`. Progress goes out on the events stream.

## Acceptance criteria

- [ ] An edited fixture note is searchable after the debounce (injected), observed through the events stream
- [ ] A file changed while the service was stopped is picked up on start
- [ ] Editing one section re-embeds exactly one chunk (embed count visible in `/status`)
- [ ] Renaming a note re-embeds nothing
- [ ] `--reembed` re-embeds everything; `--full` re-reads everything but embeds only changes

## Blocked by

- 06-tracer-meaning-search
