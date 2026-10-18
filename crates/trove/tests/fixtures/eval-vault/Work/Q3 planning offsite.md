---
tags: [work, planning, atlas]
date: 2025-06-13
location: Strandpaviljoen Noordzeeduin, Zandvoort
---
Two days, 12 and 13 June 2025. Writing this up on the 17:46 train back from Zandvoort before I forget half of it.

## Who was there

- Henk Bosman (VP Product, ran the agenda, mostly)
- Sanne Visser (eng manager Amsterdam)
- Jens Albrecht (Berlin, came over on the night before, stayed at the Hotel Zuiderbad)
- Priya Raman (design lead)
- Tomás Ferreira (APM)
- Mei Lin dialled in from Shenzhen for the Thursday morning block only, 15:00 her time
- plus Joost, Anouk, Ilse, Ravi and me. 11 people in the room.

Venue was €2,340 for both days incl. lunch and the sad sandwiches. Booked on PO-55813, Henk approved. Coffee machine broke at 10:20 on day 1, which explains the mood of the first session.

## Agenda as planned vs as happened

Planned:
1. H1 look-back (60 min)
2. Atlas scope for Q3 (90 min)
3. Carrier API roadmap (60 min)
4. Team health + hiring (45 min)
5. Commit to Q3 goals (60 min)

What actually happened: look-back ran 95 min, carrier API got squeezed into 25 min after lunch on day 2, team health became a walk on the beach which honestly worked better than the slot we'd planned.

## Day 1: H1 look-back

- Revenue H1 €5.9M vs plan €6.2M. Henk calm about it, Finance (Marloes) less so.
- Atlas behind: Berlin go-live will not make 30 June, everybody knew, nobody had said it out loud in a room yet. Jens said it. Good.
- Customer churn 3 accounts, all small, two of them because of the old dock planner UI.
- Support tickets per 100 sites down from 41 to 33. Ravi's triage rota is working.

## Atlas scope for Q3

The big fight of the offsite. Two camps:

- Sanne + Jens: ship Berlin with the 22 doors only, freeze scope, no carrier self-rescheduling until Q4.
- Tomás + Priya: carrier self-rescheduling is the whole point for the carriers, without it they keep phoning the gate.

I sided with shipping first. Henk agreed. Carrier self-rescheduling goes behind a flag for the pilot only, see [[Feature flags]]. Tomás was quiet after that; need to check in with him (put it in my notes for [[Quarterly review Tomás]]).

## Carrier API roadmap

Short. v2 of the carrier API, rate limit stays at 120 req/min per carrier for now. Two carriers (Llorente and Frío Turia, from the [[Reunión con proveedores]]) want webhooks instead of polling. Parked for Q4 planning.

## Decisions

1. Atlas Berlin go-live moves to 18 August 2025. No new date slips without a written reason to Henk.
2. Carrier self-rescheduling: flag on for Lagerhaus Spree only.
3. Second Atlas site in Q3 is Rotterdam Waalhaven (not Venlo).
4. Hiring: one more backend engineer for Berlin, req opens 1 July.
5. Offsite format again in December, but somewhere with a working coffee machine.

## Parked

- Mobile app for drivers (Priya's idea, love it, not this year)
- Pricing change for small warehouses
- Rewriting the old dock planner. Not touching it until Atlas replaces it.

## Action items

- [ ] Yasmina: rewrite Q3 goals in the [[OKR draft 2025]] doc by Fri 20 June
- [ ] Jens: new Berlin plan with 18 Aug date, send to Ute at Lagerhaus Spree
- [ ] Tomás: flag spec for self-rescheduling, 27 June
- [ ] Sanne: Waalhaven tech survey, book for the week of 7 July
- [ ] Henk: tell the board about the slip (his job, not mine, thank god)
- [ ] Priya: dock door status component in the design system by end of July

## Spend

Venue €2,340, dinner on the 12th at Het Badhuis €612 for 11 people, train tickets for Jens €189. All claimed under the offsite code per the [[Expense policy]]. Over budget by €140, Henk shrugged.

## My own notes

Tired but good. First time this year the team argued properly instead of nodding in Slack. Jens is great in a room, much better than on calls. I talk too fast when I'm nervous, Priya told me after. Fair.
