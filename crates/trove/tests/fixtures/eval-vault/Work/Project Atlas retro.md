---
tags: [work, atlas, retro]
date: 2025-09-04
---
Atlas retro, Thursday 4 Sep 2025, 14:00 to 16:00. Facilitated by Sanne so I could actually take part. Format: numbers first, then went well / hurt / try, then actions. 9 people, Mei Lin sent written input.

## Setup

Retro covers kickoff (3 Feb) through the Berlin pilot go-live and the first two weeks live. Ground rule: talk about the system, not people. Sanne had a Miro board with 3 columns, 112 stickies by the end.

## Numbers vs the kickoff

| | Kickoff target | Actual |
|---|---|---|
| Go-live Lagerhaus Spree | 30 Jun | 18 Aug (7 weeks late) |
| Avg truck waiting time | 20 min | 26 min (from 47) |
| No-shows | under 10% | 12% |
| Gate phone calls / day | ~115 | 140 |
| Spend | €410k | €468k |

So: late, over budget, and still a big improvement for Ute's team. Waiting time down 45% is real. Ute sent flowers. Actual flowers, to the office.

## What went well

- The 14 warehouse interviews. Priya's door status icons came straight out of them.
- Flags per site: we turned self-rescheduling on for Spree on 25 Aug with no deploy.
- Jens' relationship with the Betriebsrat. Approval came 2 June after he went to two of their meetings in person.
- Carriers did adopt: 31 carriers booked at least one slot in week 1.

## What hurt

### ATL-212

The double booking bug. When a carrier rescheduled through the API and the web page at the same time, both requests got the slot. Happened 9 times on 20 and 21 August, two trucks at door 14 at once, once with a reefer that had to idle for an hour. Root cause: no unique constraint on (door, slot_start), we relied on an app-level check. Fixed 22 Aug. The flaky integration test that should have caught it had been skipped since May, see [[CI pipeline flakiness]].

### Scope creep

Henk's yard management ask came back in April as "just a map of the yard". Cost us 3 weeks. I should have said no harder.

### Carrier API retries

Two carriers hammered the API with instant retries after 429s, 1,800 req/min peaks. We had to publish proper backoff docs, see [[Retry backoff]].

### Planning

The 30 June date was never real. Jens said so at kickoff. I let it stand because Henk had told the board. My mistake.

## What we'll try next time

- Dates come from the team, then go to Henk, not the other way round
- Every skipped test gets a ticket and an owner within 48h
- DB constraints for anything that must be unique, no app-level only checks
- Design review in the definition of done (Priya has asked since January)

## Actions

- [ ] Sanne: audit skipped tests across repos, by 19 Sep
- [ ] Tomás: carrier API docs v2 with backoff examples, by 26 Sep (part of his Q3 goals, see [[Quarterly review Tomás]])
- [ ] Jens: write up the Betriebsrat process for the next German site
- [ ] Me: second-site plan for Waalhaven with dates from the team, before 1 Oct
- [ ] Me: share the retro with Mei Lin's team in Shenzhen

Original plan for comparison: [[Project Atlas kickoff]].
