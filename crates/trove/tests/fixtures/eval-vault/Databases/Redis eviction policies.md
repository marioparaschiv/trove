---
tags: [redis, databases]
---

## Two instances, two policies

`cache-redis` (sessions, fx rate cache, merchant config): `maxmemory 6gb`, `maxmemory-policy allkeys-lfu`.
`rl-redis` (rate limit buckets, idempotency locks): `maxmemory 2gb`, `noeviction`. Anything that would be wrong if it silently disappeared lives here.

## The August 2024 thing

Before the split, everything was on one instance with `allkeys-lru`. During a big merchant export someone cached 1.2M report rows, memory filled up, and LRU happily evicted rate limit buckets and idempotency locks along with them. Locks gone meant duplicate webhook deliveries to 230 merchants. PAY-3290. Split the instances the following week.

## LFU tuning

`lfu-log-factor 10`, `lfu-decay-time 1` (minutes). `OBJECT FREQ <key>` shows the counter for a key, handy for checking that the fx rate keys stay hot. LFU beat LRU on hit rate for us: 94.1% vs 89.7% over the same 7 days in September 2024.

## volatile-* policies

They only evict keys that have a TTL. If nothing has a TTL they behave like `noeviction` and writes start failing with OOM errors. Mateusz calls `volatile-ttl` a trap and I agree with him, for once.

## Monitoring

- `evicted_keys` from `INFO stats` on `cache-redis`: trend only, no alert.
- `rl-redis` can't evict, so alert on `used_memory` above 80% of maxmemory instead.

## Related

[[Rate limiting token bucket]], the fallback rates in [[Circuit breaker]].
