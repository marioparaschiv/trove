---
tags: [postgres, databases]
date: 2025-02-18
---

## Why I keep this note

Every time I open an EXPLAIN output after a few months away I forget which way to read it. So here it is, written down once.

## The command

`EXPLAIN (ANALYZE, BUFFERS, SETTINGS)` on a replica, never on the primary for anything that writes (ANALYZE actually runs the query). explain.dalibo.com for the visual version, but never paste prod queries with merchant names in them. Sabine would end me.

## Reading order

Inside out, bottom up. Indentation means child node. `cost=startup..total` is in planner units, not milliseconds. `actual time` is per loop, so multiply by `loops`. I got that wrong for a whole afternoon once and thought a node took 2 ms when it took 2 ms × 41,000.

## Estimated vs actual rows

This is the one that matters. The settlements report: `settlements JOIN settlement_lines ... WHERE settlement_date = $1 AND currency = 'PLN'`. The planner estimated 12 rows, the actual count was 48,213. Cause: `settlement_date` and `currency` are correlated (PLN settlements only happen on Polish business days) and the planner multiplies the two selectivities as if they were independent. Fix:

```sql
CREATE STATISTICS st_settle_date_ccy (dependencies)
  ON settlement_date, currency FROM settlements;
ANALYZE settlements;
```

New estimate 45,900. The plan switched from a nested loop to a hash join and the query went from 19 s to 1.4 s. Ticket PAY-3687.

## Join types in one line each

- Nested loop: great when the outer side is small. Disaster when the estimate says 12 and reality says 48k.
- Hash join: builds a hash of the smaller side; watch for `Batches: 8` meaning it spilled, raise `work_mem` for that session.
- Merge join: both sides sorted, shows up with big range scans.

## Buffers

`shared hit` came from Postgres' cache, `read` came from the OS or disk. `read=212000` means about 1.6 GB pulled in for one query. If the second run is much faster, it's the cache, not your fix.

## Things that bite

- Generic plans: after 5 executions of a prepared statement Postgres may switch to a generic plan. The settlements query was fast in psql and slow from the app because sqlx prepares everything. `plan_cache_mode = force_custom_plan` for the `reporting_ro` role.
- JIT: compiling took 380 ms on a 20 ms OLTP query. `jit = off` on the primary.
- A big `Rows Removed by Filter` means a missing or useless index, which is a selectivity question, see [[Index selectivity]].
- Stale statistics after a bulk load: run ANALYZE before reading anything into the plan ([[Postgres vacuum]] covers autovacuum's analyze thresholds).

## Related

Every PR with a new query should come with a plan, that's on the [[Code review checklist]].
