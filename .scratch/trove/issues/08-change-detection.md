Status: done

## Parent

`.scratch/trove/spec.md`

## What to build

Keep the index current (spec "Indexing": change detection and chunk text hashes). A file watcher with debounce inside `trove serve`, a scan on start by mtime then hash, re-embedding only chunks whose text hash changed, vector reuse on rename, removal on delete, and `index --full` / `--reembed`. Progress goes out on the events stream.

## Acceptance criteria

- [x] An edited fixture note is searchable after the debounce (injected), observed through the events stream
- [x] A file changed while the service was stopped is picked up on start
- [x] Editing one section re-embeds exactly one chunk (embed count visible in `/status`)
- [x] Renaming a note re-embeds nothing
- [x] `--reembed` re-embeds everything; `--full` re-reads everything but embeds only changes

## Blocked by

- 06-tracer-meaning-search

## Comments

Scan (`indexer::index_sources(index, embedder, config, mode, progress)`, `indexer/scan.rs`). `FileProcessor`/`Indexer` are unchanged.
- `IndexMode` (serde snake_case): `changed` (default) skips a note whose `files.mtime` matches, reads the rest, and for a matching content hash only updates `mtime`; otherwise it upserts the row and runs the indexer. `full` reads and chunks every note. `reembed` runs `DELETE FROM vectors` first, then does what `full` does. Every mode embeds only chunk texts without a vector.
- `IndexReport.files` counts the notes chunked again (0 for an unchanged `changed` run, every note for `full`).
- After every source is scanned, vectors no chunk uses are deleted (`DELETE FROM vectors WHERE text_hash NOT IN (SELECT text_hash FROM chunks)`). One scan sees the whole folder, so a rename (new path chunked, old row dropped) keeps its vectors and embeds 0. 12: the sync store must keep vectors the same way, and a note deleted here is the cue to delete its store file.
- `progress` gets `EmbedProgress { embedded, total }` after each batch of 16; `total` is the distinct unembedded texts counted when embedding starts. The CLI passes a no-op when no service runs.

Service (`trove serve`).
- `http/indexing.rs`: `Job::queue(&state)` takes an `Activity` guard and marks the service busy until the run is over; `job.run(mode)` waits for its turn (a tokio mutex, so runs never overlap), runs on `spawn_blocking`, and publishes events. Failures go to the events stream and stderr. `POST /index` runs its job on its own task, so a client hanging up doesn't cut it short.
- Startup: `Service::start` watches every source, then queues a `changed` run before `listening` is printed (`/status` shows it busy from the first request); `run` spawns it.
- `http/watcher.rs` (`notify` 8.2, the newest stable): watches each source recursively; `POST /sources` and `DELETE /sources` add and remove watches. A folder that can't be watched is a warning on stderr. Ignored: `Access` events (indexing reads notes, which would loop) and paths with a hidden component or the store folder under a source (matched against the registered and the canonical path, since macOS reports `/private/var/...`). Unmatched paths, rescan flags and watcher errors count as changes. After `index_debounce` with no relevant event, one `changed` run over all sources.
- Events (`GET /events`, `type`-tagged): `index_started {mode}`, `index_progress {embedded, total}`, `index_finished {report}`, `index_failed {message}`, then `stopping` as before. `index_finished` is the last event of a run and is sent before the run stops counting as busy.
- `GET /status` gained `indexing: {busy, embedded}`; `embedded` is chunk texts embedded since the service started (12: "zero model calls" can assert it stays 0). The CLI `status` output is unchanged.
- `POST /index` takes `{mode?}` (unknown fields rejected, 400 on bad JSON); the CLI is `trove index [--full] [--reembed]` (`--reembed` wins over `--full`).

Tests: `crates/trove/tests/change_detection.rs`, real model, `index_debounce = "200ms"`. To wait on the service without races, subscribe with `serve.events()` (returns once the response headers arrive, so the subscription exists) before acting, then `events.until("index_finished")`. `serve_indexed` waits out the startup run (subscribe, then `/status` busy means wait for `index_finished`), and the test then opens a fresh subscription, so no startup events leak into it. `common::Serve` now kills its process on drop, so a failing test doesn't leave a service running. `serve.rs`'s events test asserts `stopping` is the last event, since the startup run may come first. Watcher tests passed 6 runs in a row here.

Not done: an edit within the filesystem's mtime granularity right after a scan (coarse-time filesystems like FAT) is skipped by `changed` until the file changes again; `--full` catches it.
