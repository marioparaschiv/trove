---
tags: [oncall, incident]
date: 2025-04-10
---

## Rotation

Six people in the platform rotation as of April 2025: Leni, Aylin, Mateusz, Tariq, Dmitri, me. Primary and secondary, one week each, handover Mondays at 10:00. That's on call roughly every six weeks as primary. On-call allowance is EUR 350 per primary week, EUR 150 secondary, plus time off in lieu for any page between 22:00 and 07:00.

## Before your shift

- PagerDuty app on the phone, test page sent to yourself on Monday morning. Do it. Dmitri's phone was on Do Not Disturb for a whole week in 2024.
- VPN works from your laptop and you know where the laptop charger is.
- Read the last week's handover note and open incidents.
- Check you still have cloud console access for volume resizes (see [[Incident postmortem 2025-03]] for why this line exists).

## Severity and response times

- SEV-1: money is wrong or payouts fully stopped. Acknowledge in 5 min, all hands, Leni informs the BaFin contact if over 4 h.
- SEV-2: payouts delayed, partner API degraded. Acknowledge in 15 min.
- SEV-3: something is broken but customers can't tell. Next working day.

## The first 15 minutes

1. Acknowledge the page. Nothing else first.
2. Open an incident channel `#inc-YYMMDD-short-name`, post what you know in one line.
3. Check the status board in Grafana, "Platform overview", before anything else.
4. Decide: can I handle this alone? If unsure, page secondary. Nobody has ever been told off for paging too early.
5. Write down times as you go. Future you writing the postmortem will thank you.

## Runbook index

- RB-01 Postgres primary down / failover
- RB-02 PgBouncer saturated
- RB-03 Payout backlog growing
- RB-04 Pod restart loops ([[Kubernetes liveness probes]])
- RB-05 Disk full on DB volumes
- RB-06 Certificate expiring, including the Nordlicht mTLS cert
- RB-07 fx breaker open ([[Circuit breaker]])
- RB-08 Terraform lock stuck. Do not force-unlock without asking in #infra.
- RB-09 Brightwell PSP outage, how to inform merchants

## Handover

Short note in #oncall on Monday: pages count, anything still open, anything weird you saw but didn't fix. Template pinned in the channel. If the week had more than 5 pages, we discuss it in the Wednesday ops sync.

## Looking after yourself

Being woken at 04:00 twice in a row wrecks the rest of the week, I've learned this the boring way. Things that help me: no coffee after 14:00 on on-call weeks ([[Caffeine cutoff]]), phone on the nightstand at full volume but everything else muted, and going to bed at a stupidly reasonable hour ([[Sleep]]). If you got paged at night, come in late. Nobody is checking, and if they are, send them to Leni.
