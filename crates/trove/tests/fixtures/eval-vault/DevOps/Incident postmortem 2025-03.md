---
tags: [incident, postmortem]
date: 2025-03-21
---

## Summary

INC-2503-17. On 2025-03-17 the ledger Postgres primary ran out of disk on its WAL volume, crashed, and SEPA payouts were delayed by 2 h 47 min. 8,412 payouts worth EUR 3.9M went out late, none were lost or duplicated. Severity SEV-2. I was incident commander, Aylin was ops lead, Leni did comms.

## Timeline (CET)

- 03:41 `pg_wal` volume at 85%. Warning alert fires, goes to Slack only. Nobody awake reads Slack.
- 05:58 Volume at 100%. Postgres PANIC: could not write to file in `pg_wal`. Primary down.
- 06:00 Pager goes off. Me, on call, in pyjamas.
- 06:09 Found `pg_wal` at 400 GB. Normal is 6 GB.
- 06:24 Aylin joins. `pg_replication_slots` shows `slot_analytics_old`, inactive, `restart_lsn` from 2025-02-26.
- 06:31 Leni posts the status page update.
- 06:52 Volume expanded from 400 GB to 600 GB via the cloud console, Postgres starts, crash recovery.
- 07:18 Dropped `slot_analytics_old`. WAL starts recycling.
- 08:05 Payout worker re-enabled, backlog drains at roughly 70 payouts/s.
- 08:47 Backlog empty, incident resolved.

## Root cause

The old analytics replica was decommissioned on 2025-02-26 (ticket OPS-1301), but its physical replication slot on the primary was never dropped. An inactive slot keeps every WAL segment from its `restart_lsn` onwards, forever. 19 days of WAL at our write rate filled the volume.

## Contributing factors

- No alert on inactive replication slots or on slot lag.
- The 85% disk alert was warning-level, routed to Slack, not the pager.
- `max_slot_wal_keep_size` was unset (default -1, unlimited).
- The decommission checklist for replicas didn't mention slots at all.

## What went well

- No data loss: crash recovery was clean, the ledger sum check came out at exactly 0.
- Idempotency keys meant the payout worker could resume without any duplicate risk.
- Status page updated within 31 minutes of the page.

## What went badly

- 2 h 17 min between the first warning and anyone looking at it.
- I spent 10 minutes reading the wrong dashboard (replica disk, not primary).
- Nobody had the cloud console permission to resize volumes except Aylin. Bus factor of one at 6 in the morning.

## Action items

| ID | Action | Owner | Status |
|---|---|---|---|
| OPS-1342 | `max_slot_wal_keep_size = 50GB` | Aylin | done 2025-03-19 |
| OPS-1343 | Alert on inactive slots older than 1 h, pages | me | done 2025-03-20 |
| OPS-1344 | Disk > 85% on DB volumes pages instead of Slack | Mateusz | done 2025-03-24 |
| OPS-1345 | Add "drop replication slot" to replica decommission checklist | me | done |
| OPS-1346 | Volume resize permission for all on-call engineers | Leni | done 2025-04-02 |

## Lessons

Alert on the cause, not just the symptom. The cert expiry in [[TLS certificate renewal]] taught us the same thing a year ago and we evidently didn't generalise it. The alerting rework that came out of this is in [[निगरानी अलर्ट]]. Backup side of things: [[Backup and point-in-time restore]]. We ran the retro in the format from [[Project Atlas retro]], which worked better than our usual free-for-all.
