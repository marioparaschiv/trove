---
tags: [resilience, payments]
date: 2024-11-04
---

## Why this note exists

PAY-3702. payout-gateway retried every failed call to Brightwell PSP immediately, three times, from 14 pods at once. When Brightwell had a 40 second blip on 2024-10-29 we sent them roughly 9,000 requests in under a minute and they rate limited our merchant account for an hour. Fun Tuesday. Leni asked me to "fix retries properly", so this is me fixing retries properly.

## The formula

Exponential backoff with full jitter, the version from the AWS architecture blog:

```rust
let exp = base_ms.saturating_mul(1u64 << attempt.min(16));
let capped = exp.min(cap_ms);
let delay = rand::thread_rng().gen_range(0..=capped);
```

Base 200 ms, factor 2, cap 8,000 ms, max 5 attempts. Worst case total wait is around 15 s, which still fits inside the 30 s HTTP timeout on the merchant-facing side.

### Jitter

Without jitter all 14 pods retry in lockstep and you get a thundering herd at 200, 400, 800 ms. I tried "equal jitter" first (half fixed, half random). Full jitter spread things out better in the k6 run: p99 queue depth on Brightwell's side dropped from 310 to 45. Their number, from Jana on their support Slack, so take it as gospel or don't.

## What gets retried

- 502, 503, 504: yes
- 429: yes, but `Retry-After` wins over our own delay when present
- connection reset / timeout before any response: yes, but only if the call is idempotent
- 400, 401, 403, 422: never. Retrying a validation error five times gets you five validation errors.
- 500: once. Brightwell returns 500 for "insufficient balance on float account", which is about as transient as my landlord.

## Idempotency keys or don't bother

Every payout POST carries `Idempotency-Key: po_<ulid>`. Generated once when the payout row is created, stored in `payouts.idempotency_key`, reused on every attempt. Before this, a timeout followed by a retry created a duplicate payout twice in September (EUR 1,240 and EUR 86.50, both clawed back by hand by Aylin). Retries without idempotency keys are just a creative way to send money twice.

## Retry budget

Per-process budget: retries may be at most 10% of total calls over a rolling 60 s window. If Brightwell is fully down, the budget runs dry and we fail fast instead of multiplying their load by 5. This is the bit that overlaps with the [[Circuit breaker]], and the two get confused in every design review.

## Config in prod

```toml
[payout_gateway.retry]
base_ms = 200
factor = 2
cap_ms = 8000
max_attempts = 5
jitter = "full"
budget_ratio = 0.10
budget_window_s = 60
```

Deployed 2024-11-12 behind the `payout_retry_v2` flag (see [[Feature flags]]). Fully rolled out 2024-11-19.

## Things I got wrong

1. First version counted `attempt` from 1, so the first delay was 400 ms, not 200 ms. Off by one, as is tradition.
2. Retried inside the DB transaction. Held a row lock on `payouts` and a pooled connection for up to 15 s. Mateusz spotted it as pool waits, see [[Connection pooling]]. Retries now happen strictly outside the transaction.
3. Logged every attempt at WARN. 40k log lines during the next Brightwell wobble. Attempts are DEBUG now, only the final failure is WARN.

## Related

[[Rate limiting token bucket]] is the other side of the same coin. The maths is weirdly close to [[Spaced repetition intervals]], which I only noticed because my sister won't stop talking about her flashcard streak.
