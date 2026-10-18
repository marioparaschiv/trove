---
tags: [api, resilience]
---

## Why we have it

Public merchant API, and partners integrate with scripts that loop on errors. Haferkiste GmbH sent 2,300 req/s on 2024-06-18 because their cron job was a `while true` with no sleep. PAY-3120. Before that we had nothing, which in hindsight is impressive.

## Bucket parameters

Per API key: capacity 100 tokens, refill 20 tokens/s. Payout creation has its own bucket: capacity 10, refill 1/s. Token bucket over fixed window because fixed windows let through double bursts at the boundary (200 requests in 1 s across the edge of two windows, seen in the k6 run).

## Implementation

Redis, one hash per key and route, `rl:{api_key}:{route}`, fields `tokens` and `ts`. A Lua script so read-modify-write is atomic:

```lua
local cap, rate, now = tonumber(ARGV[1]), tonumber(ARGV[2]), tonumber(ARGV[3])
local tokens = tonumber(redis.call('HGET', KEYS[1], 'tokens') or cap)
local ts = tonumber(redis.call('HGET', KEYS[1], 'ts') or now)
tokens = math.min(cap, tokens + (now - ts) * rate)
if tokens < 1 then return 0 end
redis.call('HSET', KEYS[1], 'tokens', tokens - 1, 'ts', now)
redis.call('EXPIRE', KEYS[1], 120)
return 1
```

This lives on `rl-redis`, separate from the cache instance, with `noeviction` ([[Redis eviction policies]] explains the hard way we learned that).

## Response headers

`RateLimit-Limit`, `RateLimit-Remaining`, `RateLimit-Reset` (IETF draft names) plus `Retry-After` on 429. Partners that respect `Retry-After`: about 30%, per our logs in January 2025. The rest are why [[Retry backoff]] exists on our own side too.

## Gotchas

- Use Redis `TIME`, not the pod clock. Pods drifted 1.4 s once and buckets refilled in strange ways.
- Internal batch jobs bypass via `X-Internal-Caller`, accepted only from the cluster network.
- If Redis is down we fail open. Debated with Leni for 40 minutes. Failing closed would let a cache box take the whole API down.
- Limits per API version are the same for now, see [[API versioning]].
