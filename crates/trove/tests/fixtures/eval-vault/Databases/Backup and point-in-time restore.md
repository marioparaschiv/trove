---
tags: [postgres, backups]
date: 2025-06-14
---

## What we run

pgBackRest 2.52 on the ledger cluster. Full backup Sunday 01:00, differential every other night at 01:00, WAL archived continuously to the bucket `kp-pg-archive-fra` in Frankfurt, with a copy replicated to a second region. Retention: 4 full backups, WAL for 35 days.

## Targets

- RPO: 1 minute (`archive_timeout = 60`)
- RTO: 2 hours for the ledger database, 1.8 TB on disk

Nobody had checked the RTO since 2023, which is how the drill below happened.

## Restore drill BKP-0912

2025-06-12, Katrin Vogt and me, fresh VM, restore to 2025-06-11 14:37:00 UTC.

- restore of the base backup: 1 h 22 min
- WAL replay: 38 min
- total: 2 h 04 min, missing the RTO by 4 minutes

The bottleneck was `process-max=4`. Raised it to 12, and the follow-up drill on 2025-06-26 took 1 h 31 min.

## The PITR command

```bash
pgbackrest --stanza=ledger --type=time \
  "--target=2025-06-11 14:37:00+00" \
  --target-action=promote restore
```

Always write the target with an explicit `+00`. See below for why.

## Verifying a restore

1. Row counts on `ledger_entries`, `payouts`, `settlements` against the numbers we noted at the target time.
2. Sum of all ledger entries must be exactly 0. Double-entry bookkeeping is the best integrity check there is.
3. Highest `ledger_entries.id` must match the last id before 14:37:00 in the audit log.
4. App smoke test against the restored DB with the read-only role.

## What went wrong in the drill

- Timezone: first attempt used `2025-06-11 14:37:00` without offset, server was in Europe/Berlin, restored to two hours earlier than intended. Nobody noticed for 25 minutes.
- The access key for the archive bucket on the drill VM had expired in April. Nobody would have noticed until a real disaster.
- The runbook still referenced pgBackRest 2.38 flags.

## Monitoring the backups themselves

- `pgbackrest info --output=json` scraped every 15 minutes by a small exporter. Alert if the newest full backup is older than 8 days or the newest differential older than 26 hours.
- Alert if `pg_stat_archiver.failed_count` increases. One failed WAL push is noise, five in a row means the bucket credentials are broken again.
- Monthly: Katrin restores the latest backup into staging automatically and runs the ledger sum check. Results go to #db-ops. If that job ever goes quiet, someone has to ask why.

## Encryption

Repository encrypted with `aes-256-cbc`. Passphrase in Vault at `secret/pg/backrest`, plus a sealed printout in the office safe on the 4th floor. Leni has the combination, so does Katrin.

## Related

Server-level backups, configs and Vault snapshots: [[النسخ الاحتياطي للخوادم]]. Small stuff on SQLite: [[SQLite WAL mode]]. The disk-full incident that made us look at all this: [[Incident postmortem 2025-03]].
