---
tags: [ci, testing]
---

Flake hunting log, ledger-svc main branch.

- **2025-01-08** Flake rate on main: 7.4% over the last 500 pipelines. Target below 1% by end of Q1, it's KR2 in [[OKR draft 2025]].
- **2025-01-09** Worst offender: `payout_webhook_retries_after_timeout`, fails about 1 in 30. Real 500 ms sleeps racing each other. Replaced with `tokio::time::pause()` and `advance()`. Gone.
- **2025-01-15** Second offender: tests sharing the `public` schema and depending on run order. Moved to per-test schemas, see [[Pruebas de integración]].
- **2025-01-22** `gl-runner-07` produces random linker crashes. SMART says the SSD has 3% life left. Drained it, Mateusz ordered a replacement (EUR 149).
- **2025-02-03** Added a `flaky-tracker` job: parses the JUnit XML from nextest, writes pass/fail per test into a table, Grafana board "Flaky top 20".
- **2025-02-19** Pulls of `postgres:16.4-alpine` timing out because of Docker Hub limits. Set up a pull-through mirror at `mirror.kontor.internal`. Image caching notes in [[Docker layer caching]].
- **2025-03-05** 1.8%.
- **2025-03-27** One test only fails between 23:00 and 01:00 UTC: date boundary in a "settled today" assertion. Fixed clock.
- **2025-04-14** 0.9%. Celebrated with Club-Mate and a slightly smug Slack post.
- Policy: nextest `retries = 0` on main. Automatic retries hide real races, and in a ledger a real race is money.
- Still open: `fx_cache_refresh` flakes about once a month. Can't reproduce locally. Suspect the runner clock.
