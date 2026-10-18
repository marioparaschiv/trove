---
tags: [postgres, databases]
date: 2024-12-02
---

## What selectivity means here

The fraction of rows a predicate keeps. Small fraction means selective means an index is worth it. The planner gets this from `pg_stats`: `n_distinct`, `most_common_vals`, `most_common_freqs`, histogram bounds. If those are wrong, every estimate built on them is wrong too.

## The status column trap

`payouts.status` distribution on 2024-11-28: settled 97.2%, pending 1.9%, failed 0.8%, held 0.1%. Someone had added `idx_payouts_status`, a plain btree, 1.1 GB. For `settled` the planner never used it (seq scan is correct when you want 97% of the table), and the pending sweeper didn't benefit much either. Replaced it with a partial index:

```sql
CREATE INDEX CONCURRENTLY idx_payouts_pending
  ON payouts (created_at)
  WHERE status = 'pending';
```

18 MB instead of 1.1 GB. The pending sweeper went from 2.3 s to 4 ms. Dropping the big index also took HOT updates on `payouts` from 12% to 64%, which [[Postgres vacuum]] was very grateful for.

## Composite column order

`(merchant_id, created_at)`, not the other way round. 41k distinct merchants, queries are always "one merchant, a date range". Equality column first, range column second. Dmitri had it reversed on `refunds` and the index was used for nothing but taking up 700 MB.

## Skewed data and the statistics target

The top 5 merchants hold 38% of all payouts. With the default `default_statistics_target = 100`, merchant number 4 didn't make the MCV list and got the average estimate, off by a factor of 300. Fix:

```sql
ALTER TABLE payouts ALTER COLUMN merchant_id SET STATISTICS 1000;
ANALYZE payouts;
```

## Related

How I actually read the resulting plans: [[Query plan reading]]. Normalisation notes from the same month: [[データベース正規化]].
