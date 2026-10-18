---
tags: [release, process]
---

## Setup

Unleash v5.9, self-hosted in our cluster, Postgres-backed. About 140 flags as of March 2025, of which maybe 40 are actually alive. The Rust SDK polls every 15 s and keeps everything in memory, so a flag check costs nanoseconds.

## Naming and ownership

`<team>_<thing>_v<n>`, e.g. `payout_retry_v2`, `kyc_selfie_v1`. Every flag gets an owner and a `remove_by` date in its description. Anything 90 days past `remove_by` lands in a Monday report that nobody except me reads.

## Rollout

Gradual by `merchant_id` hash: 1% → 10% → 50% → 100%, at least 3 days in total. Kill switches (`ks_*`) are the exception: permanent ops toggles, never removed. `ks_sepa_instant` turns off SEPA Instant when Nordlicht Bank is having a day, which in 2024 was 6 times.

## Mistakes we made

- `ledger_dual_write_v1` sat at 100% for 7 months with both code paths still in prod. 600 lines of dead branches, removed in PAY-3341.
- A flag evaluated inside a hot loop, 11k checks/s. The SDK was fine, our metrics exporter wasn't: CPU +18%.
- Never put a flag check inside a DB migration. I have seen it once and that was enough.

## Related

The rollout plan in [[Project Atlas kickoff]] is built on these. [[Retry backoff]] v2 shipped behind one. Testing both sides of a flag: [[Pruebas de integración]].
