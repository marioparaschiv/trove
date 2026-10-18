---
tags: [postgres, databases]
---

## Setup

PgBouncer 1.22 in front of the Postgres 16 primary. `pool_mode = transaction`, `default_pool_size = 40`, `max_client_conn = 2000`, `max_db_connections = 120`. Each app pod also has an sqlx pool with `max_connections = 10`. 36 pods × 10 = 360 app connections into the bouncer, at most 120 server connections out of it.

## PAY-4471: prepared statements in transaction mode

sqlx uses named prepared statements. In transaction mode the next transaction can land on a different server connection, which has never heard of that statement: `prepared statement "sqlx_s_3" does not exist`. Started 2025-05-06 after a bouncer restart reshuffled things, 0.6% of ledger requests failing. Fix: PgBouncer 1.22 with `max_prepared_statements = 200`, which tracks them at protocol level (supported since 1.21). Took me a full day to find, ten minutes to fix.

## Pool exhaustion

Retries inside a transaction held connections for up to 15 s each (the story is in [[Retry backoff]]). Now sqlx has `acquire_timeout = 3s` and we export `db_pool_wait_seconds` as a histogram. If p99 goes above 500 ms, something is holding connections it shouldn't, usually an `idle in transaction` session, see [[Postgres vacuum]].

## Sizing

Rule of thumb I use: server connections around 2 to 4 × cores. The primary has 32 vCPU, so 120 is in range. More connections is not more throughput: pgbench on the staging primary gave 14.2k TPS at 120 connections and 11.8k at 300.

## Related

Liveness probes must never check the DB, or a failover becomes a reconnect storm: [[Kubernetes liveness probes]]. I submitted the PAY-4471 story as a lightning talk, see [[Conference talk proposal]].
