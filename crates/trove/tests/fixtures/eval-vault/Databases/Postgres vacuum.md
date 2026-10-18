---
tags: [postgres, databases]
date: 2024-07-22
---

## The problem

`ledger_entries`: 410 million rows, around 2.5k inserts/s at peak. `payouts` gets updated 4 to 6 times per row over its lifecycle (created, reserved, sent, confirmed, sometimes failed and retried). Default autovacuum settings: `autovacuum_vacuum_scale_factor = 0.2`, which on a 40M-row `payouts` table means vacuum waits for 8M dead tuples before it even starts. pgstattuple on 2024-07-15: table 31 GB, of which 11 GB bloat. Lovely.

## How MVCC makes garbage

An UPDATE writes a new tuple version, the old one stays until vacuum removes it. Long transactions hold back the xmin horizon, so vacuum can't remove anything newer than the oldest open snapshot. Our nightly reporting job on the replica, with `hot_standby_feedback = on`, held xmin back for 3 hours every night. That alone explained half the bloat.

## Tuning per table

```sql
ALTER TABLE payouts SET (
  autovacuum_vacuum_scale_factor = 0.01,
  autovacuum_vacuum_threshold = 5000,
  autovacuum_vacuum_cost_limit = 2000
);
```

Globally: `autovacuum_max_workers = 6` (was 3), `autovacuum_naptime = 15s`, `maintenance_work_mem = 2GB`. The cost limit was the big one; the default throttle made vacuum on `payouts` take 5 hours and it never caught up.

## The wraparound scare

2024-09-03, alert: `age(datfrozenxid)` at 1.4 billion on the `ledger` database. The anti-wraparound vacuum on `ledger_entries` took 9 h 40 min, I watched most of it from my sofa with a Club-Mate. Since then we warn at 1.0 billion and page at 1.5 billion.

## Monitoring

- `pg_stat_user_tables`: `n_dead_tup`, `last_autovacuum`, `autovacuum_count`
- `pg_stat_progress_vacuum` while it runs, to see which phase it's stuck in
- Grafana dashboard uid `pgvac-01`, panel "dead tuples / live tuples" per table, alert above 0.15

## VACUUM FULL: no

Takes an ACCESS EXCLUSIVE lock on the table for the whole run. For the 11 GB we used pg_repack 1.5.0 instead: Sunday 2024-07-28 at 02:00, 47 minutes, exclusive lock only for a couple of seconds during the swap. Aylin sat on the call with me, mostly to make sure I didn't type `VACUUM FULL` out of spite.

## Things I'd tell past me

- `idle in transaction` sessions are the real enemy. `idle_in_transaction_session_timeout = 60s` everywhere now.
- Check replica feedback before blaming autovacuum.
- Bloat estimation queries lie. pgstattuple is slow but honest.
- Dropping useless indexes helps vacuum too: fewer indexes to clean, more HOT updates.

## Related

The idle-in-transaction mess came from a PgBouncer misconfig, see [[Connection pooling]]. The index cleanup is in [[Index selectivity]]. Big table rewrites are in [[Migrations without downtime]].
