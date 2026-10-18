---
tags: [work, vendors]
contract: ACM-7734-NL
---
ACME Telematics BV, the GPS / truck position feed under the ETA screen. Contract ACM-7734-NL, signed Jan 2023, runs out 31 Dec 2025. Owner on our side: me. Legal: Bas Meijer.

## Where we are

- 14,200 vehicles on the feed
- Current rate €0.82 per vehicle per month, so roughly €139.7k a year
- SLA on paper 99.5% uptime. Actual over the last 12 months: 99.1%. Two bad outages (Feb and Aug 2025), the August one took ETAs down for 3h40.
- Their API throws 503s under load, we added backoff on our side, see [[Retry backoff]].

## Their offer (2025-10-02)

Karin Wolters (their account exec) sent the renewal: €0.97 per vehicle, +18%, 1-year term. Reason given: "infrastructure costs". Also wanted to remove the uptime credits clause. No.

## Our position

- Walk-away: we have a quote from Trackwijs at €0.88, but switching is ~6 weeks of eng work for Sanne's team.
- Ask: 3-year term, €0.84, uptime at 99.7% with credits of 10% of the monthly fee per 0.1% missed.
- Give: 3-year commitment, a case study for their website.

## Outcome (2025-11-12)

Landed on €0.86 per vehicle, 3 years, uptime 99.7% with credits (5% per 0.1% missed, not 10%). Price fixed until 31 Dec 2028. About €146.5k a year. Karin pushed hard on the case study, we agreed to a quote only, no logo on stage. Bas reviewed the redlines on the 13th. How the call felt is in [[2025-11-15]].

## Next steps

- [ ] Bas: final signature version by 28 Nov
- [ ] Me: tell Finance (Marloes) the new number for the 2026 budget
- [ ] Sanne: keep the Trackwijs integration spike in the backlog anyway
- Similar talks with the Spanish carriers went much easier, see [[Reunión con proveedores]].
