#tls #devops

- **2024-03-10** `api.kontorpay.de` cert expired at 00:00 UTC on a Sunday. cert-manager had been failing to renew since 2024-02-09 because the ClusterIssuer pointed at Let's Encrypt staging after somebody's test (it was Mateusz, he bought the team lunch). 3 h 12 min of partner API failures. INC-2403-02.
- **2024-03-12** Prometheus alert: `certmanager_certificate_expiration_timestamp_seconds - time() < 14 * 24 * 3600`, warning. Below 3 days pages on-call.
- **2024-03-12** Also alerting on `certmanager_certificate_ready_status{condition="False"}` for more than 1 h. That's what would have caught it a month earlier.
- **2024-06-04** Switched from HTTP-01 to DNS-01 via the INWX webhook, so internal hostnames get certs too. API token in Vault, `secret/dns/inwx`.
- **2025-01-20** Wildcard `*.internal.kontorpay.de` renewed on its own for the third time. Nobody noticed. That's the goal.
- **2025-07-01** Nordlicht Bank mTLS client cert: issued by their CA, manual process, valid 1 year, expires 2026-07-03. Calendar reminder for 2026-06-01, and an entry in the [[On-call handbook]] because it will absolutely be someone else's problem by then.
- **2025-07-01** Their onboarding fee for the cert is EUR 389 a year, expensed under the vendor category per [[Expense policy]].
- **2025-09-15** Checked: all ingress certs now ECDSA P-256, renewal 30 days before expiry, 47 certificates managed in total.
- Lesson from INC-2403-02, also written up in [[Incident postmortem 2025-03]] as an example of "alert on the process, not the symptom".
