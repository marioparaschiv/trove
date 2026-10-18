#code-review #process

What I actually go through on a PR. Pinned in the team channel since March 2024.

- [ ] PR description says *why*, not only what. Ticket linked (PAY-xxxx).
- [ ] Money is `i64` minor units or `rust_decimal::Decimal`. Never `f64`. Found `f64` in money code 3 times in 2024. Three.
- [ ] Every new query: has someone run `EXPLAIN ANALYZE` on prod-sized data? ([[Query plan reading]])
- [ ] Migration safe while the old version is still serving traffic? ([[Migrations without downtime]])
- [ ] New outbound HTTP call has a timeout. reqwest's default is no timeout at all.
- [ ] Errors carry context, no `unwrap()` outside tests ([[Error handling in Rust]])
- [ ] No IBANs, PANs or full names in logs. grep the diff for `iban` inside `info!`/`warn!`.
- [ ] New feature flag has an owner and a `remove_by` date
- [ ] At least one unhappy-path test
- [ ] Under 400 changed lines, or a reason why not
- First review within 4 working hours. Our median in Q1 2025 was 6.2 h, so: aspirational.
- `nit:` comments never block a merge. Leni's rule since the bikeshed of February 2024 (73 comments on renaming `PayoutSvc` to `PayoutService`).
- New joiners get this list walked through together with the [[Onboarding checklist]] in week one.
