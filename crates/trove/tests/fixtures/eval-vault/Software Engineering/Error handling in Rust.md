---
tags: [rust]
---

## Rule of thumb

`thiserror` in library and domain crates, `anyhow` in binaries and at the edges. Same as everyone else, I know.

## Domain errors

```rust
#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error("account {0} not found")]
    AccountNotFound(AccountId),
    #[error("insufficient funds: balance {balance}, requested {requested}")]
    InsufficientFunds { balance: i64, requested: i64 },
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}
```

`#[from] sqlx::Error` is convenient and slightly evil: `?` quietly turns a unique violation into a generic `Db` error and the meaning is gone. For the `payouts_idempotency_key_key` constraint we map explicitly to `LedgerError::DuplicatePayout`.

## Mapping to HTTP

One `impl IntoResponse for ApiError` in axum, one match arm per variant. 404 for `AccountNotFound`, 409 for `DuplicatePayout`, 422 for `InsufficientFunds`, everything else 500 with an error id `err_<ulid>` that is logged and returned, so support can grep for it. Never return the `Display` of an sqlx error to callers: we leaked a table name once (PAY-2980, embarrassing, not dangerous).

## Context and backtraces

`anyhow::Context` everywhere in binaries: `.with_context(|| format!("fetching fx rate {pair}"))`. Backtraces on in staging only (`RUST_BACKTRACE=1`). Off in prod because Dmitri measured them at about 3% extra on the error path.

## Panics

`panic = "abort"` in the release profile, so a panic is a pod restart and [[Kubernetes liveness probes]] don't even have to notice. Clippy `unwrap_used = "deny"` outside tests since 2025-02-03. Reviewers check the rest, see [[Code review checklist]].
