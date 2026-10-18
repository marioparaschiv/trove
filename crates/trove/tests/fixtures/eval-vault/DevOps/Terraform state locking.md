---
tags: [terraform, devops]
---

Terraform 1.10.3, S3 backend on the bucket `kp-tfstate` with `use_lockfile = true` (native S3 lock files, no DynamoDB needed since 1.10). Before that, GitLab-managed state with its HTTP backend locking. 2025-02-27: Mateusz and I both ran `terraform apply` on `prod-network` within 20 seconds of each other. The lock did its job, my run died with "Error acquiring the state lock", lock ID `6f1c2a9e`. Good. Two weeks later a CI job crashed mid-apply and left the lock behind for 3 hours, which blocked everyone. `terraform force-unlock 6f1c2a9e-...` only after asking in #infra that nobody was applying, and checking the job was really dead. Rule since then: only CI applies to prod (job `tf-apply-prod`, manual gate, one at a time via `resource_group`), humans only run `plan`. Breaking that rule is on the list in the [[On-call handbook]]. Related: [[CI pipeline flakiness]].
