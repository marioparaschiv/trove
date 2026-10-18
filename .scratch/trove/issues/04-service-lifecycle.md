Status: done

## Parent

`.scratch/trove/spec.md`

## What to build

`trove serve` and the client side of spec "Service and API", with no search yet: localhost on a free port, runtime file, bearer token, `Host` check, idle exit, `GET /status`, a server-sent events stream, and OpenAPI generated from the Rust types. A TypeScript client package generated from that OpenAPI, which reads the runtime file, starts `trove serve` if it's missing or stale, and waits for it. CLI commands use a running service when one exists.

## Acceptance criteria

- [x] The runtime file is written on start and removed on clean exit
- [x] Requests without the token get 401; a foreign `Host` header is refused
- [x] The service exits after the idle timeout (injected) with no requests
- [x] The OpenAPI spec is generated in the build and the TS types regenerate from it with no hand edits
- [x] TS client test against a real `trove` binary: starts it when absent, reuses it when present, reads `/status`

## Blocked by

- 01-core-contracts

## Comments

Layout. Server code is in `crates/trove-core/src/http/`; the CLI side is `crates/trove/src/service.rs` and `commands/{serve,status,openapi}.rs`; the TypeScript client is `clients/ts-client` (package `@trove/client`).
- Adding an endpoint: write the handler in a new file under `http/` with `#[utoipa::path(...)]` (list `Rejected` in its `responses` so 401/403 appear in the spec), derive `ToSchema` on its types, and add one `.routes(routes!(file::handler))` line to `router()` in `http/mod.rs`. Handlers take `State<Arc<AppState>>`; add fields to `AppState` for what they need. Errors return `ApiError` (`ApiError::internal(&err)` is a 500 carrying the cause chain; the body is `ErrorBody`). Blocking work (SQLite, embedding) goes in `spawn_blocking`; the index is `AppState.index: Arc<Mutex<Index>>`.
- Guard (`http/guard.rs`) runs before every route: `Host` must be `127.0.0.1:<port>` or `localhost:<port>` (else 403), then `Authorization: Bearer <token>` (else 401). Only requests that pass count as activity.
- Idle exit: `Activity::begin()` returns a guard; the service exits once no guard has been alive for `service_idle_timeout`. Each request holds one. 08's watcher queue and indexing jobs should hold one too ("nothing queued"). An open `/events` stream does not keep the service alive.
- Events: `GET /events` is SSE; each event's `data` is the JSON of `http::Event` (tagged by `type`). Send with `state.events.send(...)` (broadcast; a subscriber that lags has its stream closed and should reconnect). Shutdown sends `{"type":"stopping"}` as the last event. Add variants for indexing and setup progress.
- Runtime file: `<data>/runtime.json` = `{port, token, pid, version}` (`RuntimeFile`, also in the OpenAPI components), written atomically and owner-only, removed on clean exit. `trove serve` holds an OS lock on `<data>/serve.lock` for its life, so one service runs per data folder; a second `serve` prints "already running" and exits 0. A runtime file left by a crash is harmless.
- `trove serve` prints `listening on http://127.0.0.1:<port>` on stdout once the runtime file is written; Rust tests wait on that line.
- CLI over the service: `Service::connect(&ctx.data_dir)?` gives `Some(service)` when the runtime file exists and the service answers (a refused connection means a crash leftover, so `None`), then `service.get::<T>(path)`. On `None`, open the index directly. Add `post` beside `get` when a command needs it. `trove status` is the example.
- OpenAPI: `trove openapi` (hidden) prints the spec built from the router. In `clients/ts-client`, `bun run generate` writes `openapi.json` and `src/schema.ts` (both gitignored, so never hand-edited); `test`, `typecheck` and `build` generate first. Deviation: the spec's CLI list has no `openapi`; it is hidden and only feeds generation.
- TS client: `connect({ binary, dataDir? })` returns `{ client, runtime }`; `client` is an `openapi-fetch` client with the token set. It reads the runtime file; if missing or its pid is dead, it spawns `trove serve` detached (stderr to `<data>/serve.log`, `TROVE_DATA_DIR` passed through) and waits for the runtime file with `fs.watch`, failing fast with the log if `serve` exits non-zero. `defaultDataDir()` mirrors the Rust `directories` paths. Node runs the `.ts` sources directly (type stripping); bundlers import `src/index.ts`. Tests need cargo and use `target/debug/trove`. No SSE client yet; add one when a client consumes events. `typescript` is pinned to 5.x because `openapi-typescript` 7 needs it.
- `GET /status` returns `{version, pid, index: {files, chunks}}`; 06 adds model state, 08/12 the embed counts.
- Clients layout: `clients/` is the Bun workspace root (`workspaces: ["*"]`) with the one `bun.lock`, shared devDeps (`typescript` 5.x, `@types/node` 24, `@biomejs/biome`) and one `biome.json` (formatter: spaces, width 100; recommended lint rules; generated files excluded). Packages: `ts-client` now; `obsidian` and `raycast` go beside it and depend on `"@trove/client": "workspace:*"`. Every package has `typecheck`, `lint` (`biome check .`), `test` and `build` scripts; run `bun run <script>` in `clients/` to run one across all workspaces (`bun run --filter '*'`). `bun install` runs in `clients/`. For `@trove/client`, `build` is just the typecheck: consumers import `src/index.ts` as source, so there is no emit. Its tests run under `node --test` (Node is the runtime in Obsidian and Raycast), started by `bun run test`.
