Status: ready-for-agent

## Parent

`.scratch/trove/spec.md`

## What to build

`trove serve` and the client side of spec "Service and API", with no search yet: localhost on a free port, runtime file, bearer token, `Host` check, idle exit, `GET /status`, a server-sent events stream, and OpenAPI generated from the Rust types. A TypeScript client package generated from that OpenAPI, which reads the runtime file, starts `trove serve` if it's missing or stale, and waits for it. CLI commands use a running service when one exists.

## Acceptance criteria

- [ ] The runtime file is written on start and removed on clean exit
- [ ] Requests without the token get 401; a foreign `Host` header is refused
- [ ] The service exits after the idle timeout (injected) with no requests
- [ ] The OpenAPI spec is generated in the build and the TS types regenerate from it with no hand edits
- [ ] TS client test against a real `trove` binary: starts it when absent, reuses it when present, reads `/status`

## Blocked by

- 01-core-contracts
