---
tags: [kubernetes, devops]
date: 2024-09-30
---

## The incident that started this

2024-09-24, 14:12. Planned Postgres failover, expected to take 40 seconds. The liveness probe on ledger-svc ran `SELECT 1` against the database. Database unreachable for 40 s → liveness failed on all 36 pods → kubelet restarted all of them at once → 36 × 10 fresh connections stampeding the new primary through PgBouncer → 11 minutes of outage instead of 40 seconds. We turned a non-event into an incident with one line of YAML.

## Liveness, readiness, startup

- Liveness: "is this process wedged beyond repair?" Failing it means a restart. Only check things a restart would fix.
- Readiness: "should this pod get traffic right now?" Failing it removes the pod from the Service endpoints, nothing restarts.
- Startup: holds off the other two until the app has booted. Exists so you don't need a huge `initialDelaySeconds` on liveness.

A database outage is never fixed by restarting the app. So the database never goes into liveness.

## What /livez does now

A background Tokio task bumps an `AtomicU64` heartbeat every second. `/livez` returns 200 if the heartbeat is younger than 10 s, 503 otherwise. That catches a blocked runtime or a deadlock, which are the only things a restart actually fixes.

## Config

```yaml
livenessProbe:
  httpGet: { path: /livez, port: 8081 }
  periodSeconds: 10
  timeoutSeconds: 2
  failureThreshold: 6
startupProbe:
  httpGet: { path: /livez, port: 8081 }
  periodSeconds: 5
  failureThreshold: 24
readinessProbe:
  httpGet: { path: /readyz, port: 8081 }
  periodSeconds: 5
  failureThreshold: 2
```

No `initialDelaySeconds` on liveness at all, the startup probe covers boot (up to 120 s, migrations run in an init container anyway). Liveness needs 6 × 10 s = 1 minute of failures before a restart.

## Readiness can check dependencies, carefully

`/readyz` checks that the sqlx pool can hand out a connection within 500 ms. If the DB is down, all pods go unready together and the Service has no endpoints, which returns 503 at the ingress. That's honest and fine. What we don't do is check downstream HTTP services in readiness: Kursfeld being slow must not take ledger-svc out of rotation, that's the [[Circuit breaker]]'s job.

## Separate port

Probes on port 8081, app traffic on 8080. A flood of API requests can't starve the probe handler, and the probe paths are never exposed through the ingress.

## Gotchas

- `timeoutSeconds` defaults to 1. A GC-free Rust app still misses that under CPU throttling; we had restarts during load tests until we set 2 and raised the CPU limit from 500m to 1000m.
- Panics abort the process, so the pod restarts without any probe involved ([[Error handling in Rust]]).
- `kubectl describe pod` shows probe failures in Events, but they roll off after an hour. The `kube_pod_container_status_restarts_total` metric doesn't.

## Related

[[On-call handbook]] has the runbook for restart loops (RB-04). The database side of the failover story is in [[Connection pooling]]. A later, unrelated disaster: [[Incident postmortem 2025-03]].
