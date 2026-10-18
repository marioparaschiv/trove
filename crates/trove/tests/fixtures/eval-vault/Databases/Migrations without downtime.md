---
tags: [postgres, release]
date: 2025-04-08
---

## Ground rules

Expand, migrate, contract. During a rolling deploy (about 6 minutes across 36 pods) the old and the new version run side by side against the same schema, so every migration has to be compatible with both. If a change can't be made compatible, it gets split into several deploys. No exceptions, not even on Friday afternoons. Especially not on Friday afternoons.

## Lock timeouts

Every migration file starts with `SET lock_timeout = '3s';`. Most ALTER TABLE forms need an ACCESS EXCLUSIVE lock, even if only for milliseconds. If a long query already holds a conflicting lock, the ALTER waits in the queue, and every normal query on that table queues up behind the ALTER. That was 2024-10-02: adding a column on `merchants` waited behind a 4-minute analytics query and we served 504s for 2 minutes. With the timeout the migration fails fast and we just retry it.

## Safe and unsafe operations (PG 16)

Safe:
- `ADD COLUMN` with a constant default (no rewrite since PG 11)
- `CREATE INDEX CONCURRENTLY`
- `ADD CONSTRAINT ... NOT VALID`, then `VALIDATE CONSTRAINT` separately

Unsafe:
- changing a column type (full rewrite)
- `CREATE INDEX` without `CONCURRENTLY`
- `SET NOT NULL` directly on a big table (full scan under exclusive lock)

## Renaming a column: the five-step dance

`payouts.iban` → `payouts.beneficiary_iban`, ticket PAY-3802:

1. 2025-03-03: add `beneficiary_iban`, nullable.
2. 2025-03-04: app writes both columns, reads old.
3. 2025-03-05 to 03-06: backfill.
4. 2025-03-10: app reads new, still writes both.
5. 2025-03-24: app stops writing old; drop `iban` two weeks later.

Three weeks for a rename. Product asked why. I sent them this list.

## Backfills

Batches of 5,000 rows by id range, 200 ms pause between batches, progress cursor stored in `backfill_progress` so it can resume after a crash. 40M rows took 11 hours. Watch replication lag while it runs; above 30 s the script backs off on its own.

## NOT NULL without pain

```sql
ALTER TABLE payouts ADD CONSTRAINT beneficiary_iban_nn
  CHECK (beneficiary_iban IS NOT NULL) NOT VALID;
ALTER TABLE payouts VALIDATE CONSTRAINT beneficiary_iban_nn;
ALTER TABLE payouts ALTER COLUMN beneficiary_iban SET NOT NULL;
ALTER TABLE payouts DROP CONSTRAINT beneficiary_iban_nn;
```

`VALIDATE` only takes SHARE UPDATE EXCLUSIVE, and from PG 12 the `SET NOT NULL` uses the validated check to skip the scan.

## Tooling

`sqlx migrate` for applying, and squawk 1.4 as a CI lint that fails the pipeline on unsafe operations. It caught 7 bad migrations in the first quarter of 2025, two of them mine.

## Related

Same thinking as [[API versioning]], just for tables. Feature work behind [[Feature flags]] usually needs one of these first. Reviewers check it via [[Code review checklist]].
