---
tags: [resilience, payments]
date: 2025-01-20
---

## Context

fx-quotes calls the Kursfeld API for live EUR/GBP/PLN rates. When Kursfeld degrades, response time goes from 80 ms to 12 s, and every checkout request waits on it. Retries (see [[Retry backoff]]) make it worse: a slow dependency doesn't fail, it just eats your worker threads one by one. Ticket PAY-3918.

## The three states

- Closed: calls go through, failures and slow calls are counted.
- Open: calls fail immediately with `FxError::Unavailable`, no network at all.
- Half-open: after the cooldown, a few trial calls go through. If they succeed we close, otherwise straight back to open.

## Numbers we picked

| setting | value |
|---|---|
| sliding window | 20 calls, count-based |
| failure rate threshold | 50% |
| slow call threshold | 2,000 ms |
| slow call rate threshold | 60% |
| wait in open state | 30 s |
| permitted calls in half-open | 5 |
| minimum calls before evaluating | 10 |

Tariq argued for a 60 s time-based window. We went count-based because fx-quotes traffic drops to about 3 calls a minute at night, and a time window would trip on 2 failures out of 3. He still brings it up.

## The Kursfeld outage, 2025-01-14

Timeline, CET:

- 09:02 Kursfeld p95 latency 9.8 s
- 09:04 checkout error rate 23%, all Tokio workers parked waiting on fx calls
- 09:11 I paged Tariq, we flipped `FX_STATIC_FALLBACK=1` by hand
- 09:40 Kursfeld recovered on its own

Lost roughly 1,700 checkouts. The breaker shipped 2025-01-27, two weeks later, which is fast for us.

## Fallbacks

When open: serve the last known rate from Redis if it's younger than 15 minutes, with a 0.4% spread markup to cover drift. Older than 15 minutes means we reject cross-currency checkout; EUR-only still works. Compliance (Sabine Krüger) signed off on the 0.4% in writing, COMP-212. The cached rates live in `cache-redis`, see [[Redis eviction policies]].

## Breaker vs retry, since everyone asks

Retry: one call, a few attempts, assumes the failure is transient. Breaker: looks across all calls, assumes the dependency is sick and protects us from it. They stack. Retries live inside the breaker, so a retry that hits an open breaker fails immediately instead of sleeping for 8 s. I had the order the other way round in the first PR, and Tariq caught it in review.

## Metrics and alerts

`fx_breaker_state` gauge (0 closed, 1 open, 2 half-open) and `fx_breaker_transitions_total`. Alert if open for more than 5 minutes, which links to runbook RB-07 in the [[On-call handbook]]. In the first month it opened 9 times, 7 of them between 02:00 and 03:00 when Kursfeld does maintenance without telling anyone.

## Open questions

- 5 or 3 trial calls in half-open? Kursfeld allows 50 rps, so 5 is harmless.
- Per-currency-pair breakers? The PLN feed fails separately from GBP sometimes. Parked until it hurts.
