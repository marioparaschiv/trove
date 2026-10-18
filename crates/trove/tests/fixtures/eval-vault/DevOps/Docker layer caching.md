---
tags: [docker, ci]
---

## Before

ledger-svc image build: 14 minutes on the GitLab runner. Every commit recompiled all 412 crates in the dependency tree, because the Dockerfile did `COPY . .` and then `cargo build --release`. Any change to any file invalidated the layer.

## cargo-chef

Three stages: `planner` runs `cargo chef prepare` to produce `recipe.json` (only dependency info), `cook` runs `cargo chef cook --release --recipe-path recipe.json`, then the real build copies the source. The cook layer only changes when `Cargo.lock` changes, which is maybe twice a week.

## BuildKit cache mounts and registry cache

```dockerfile
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --bin ledger-svc
```

Runners are ephemeral, so local cache alone was useless. Added `--cache-to type=registry,ref=registry.kontor.internal/cache/ledger-svc,mode=max` and the matching `--cache-from`. `mode=max` caches intermediate stages too, which is where the cook layer lives.

## Numbers

- code-only change: 14 min → 3 min 40 s
- dependency bump: 9 min
- final image on `gcr.io/distroless/cc-debian12`: 1.2 GB → 38 MB

## Gotchas

- `.dockerignore` was missing `target/` for months. Build context was 6 GB once and the upload alone took 2 minutes.
- Order matters: anything that changes often goes as late as possible in the Dockerfile.
- The registry cache image grows. Cleanup policy keeps 10 tags.

Related: [[CI pipeline flakiness]], [[Kubernetes liveness probes]].
