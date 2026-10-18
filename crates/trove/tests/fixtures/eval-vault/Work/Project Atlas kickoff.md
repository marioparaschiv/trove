---
tags: [work, atlas]
date: 2025-02-03
epic: ATL-1
---
Kickoff for Atlas, the dock slot scheduling module. Monday 3 Feb 2025, 10:00 to 12:30, big room on the 4th floor. Epic ATL-1 in Linear.

## Why Atlas

Warehouses using our planner still book dock doors by phone and Excel. Trucks show up, wait, drivers time out on their hours, carriers send angry emails. Our old dock planner just shows the mess, it doesn't fix it. Atlas lets carriers book a slot for a door, warehouses set capacity per door per hour, and the gate sees who's due.

Henk's one-liner for the board: "make the dock boring".

## Goals and metrics

- Average truck waiting time at the gate: from 47 min today (measured at the pilot site, Nov 2024) to 20 min
- No-shows: from 18% to under 10%
- Phone calls to the gate office: halve them (pilot counts ~230 a day)
- 3 paying sites by end of Q4 2025, see [[OKR draft 2025]]

## Pilot customer

Lagerhaus Spree GmbH, site in Berlin-Marzahn. 22 dock doors, 2 shifts, around 160 trucks a day. Contact: Ute Krämer, Lagerleiterin. She's sceptical but fair, which is the best kind of pilot customer. Works council (Betriebsrat) has to agree to anything that changes shift planning, that's the risk nobody on our side understood. Jens owns that relationship, see [[Projektstatus Berlin]].

## Team

- Me: PM, owner
- Tomás: APM, carrier side
- Priya: design
- Jens Albrecht: eng lead, Berlin, 4 engineers
- Sanne Visser: 2 engineers from Amsterdam for the carrier API
- Budget approved: €410k for 2025 (people + infra)

## Timeline

| Milestone | Date |
|-----------|------|
| Discovery done, 14 warehouse interviews | 28 Feb |
| Slot booking MVP in staging | 11 Apr |
| Carrier API v1 | 16 May |
| Pilot go-live Lagerhaus Spree | 30 Jun |
| Review + second site decision | 15 Jul |

Jens said 30 June is "possible but not likely". Writing that down so I remember he said it.

## Risks

1. Betriebsrat approval takes longer than expected
2. Carriers don't adopt the booking page (they have 40 other portals)
3. Door capacity data at Spree is wrong or missing
4. Our own carrier API rate limits, the old service tops out at 60 req/min
5. Scope creep from Henk (he already asked about yard management)

## Decisions made

- Ship behind flags per site, see [[Feature flags]]
- Slots are 30 min, fixed. No variable slots in v1.
- Carriers book through a web page first, API second
- We do not build yard management. Wrote it in capitals in the doc.

## Open questions

- Do we charge per door or per site? Finance wants per door.
- Who handles a truck that arrives 40 min late, the system or the gate?
- What language for the gate UI? German only for pilot, Ute says yes.

## Next steps

- [ ] Priya + Tomás: interview plan, 14 warehouses, by 7 Feb
- [ ] Jens: door data export from Spree WMS
- [ ] Me: kickoff summary to Henk and Ute, today
- [ ] Sanne: carrier API capacity estimate

How it actually went: [[Project Atlas retro]].
