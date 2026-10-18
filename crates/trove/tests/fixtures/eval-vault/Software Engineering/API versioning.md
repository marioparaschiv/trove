---
tags: [api]
date: 2025-08-04
---

## Scheme

Date-based versions in a header, Stripe style: `Kontor-Version: 2024-09-01`. Pinned per API key at creation, overridable per request. Supported right now: 2023-05-15, 2024-02-01, 2024-09-01, 2025-04-10.

## Why not /v2 in the URL

We tried in 2023. /v1 and /v2 diverged on 60-odd routes, bug fixes had to land twice, and half the partners hardcoded /v1 forever. A date header lets us ship small breaking changes without a big-bang migration for anyone.

## Version transforms

Responses are rendered at the latest version internally, then walked back through a chain of transforms down to the caller's pinned version. Example: in 2025-04-10 `amount` became an object `{ "value": 1250, "currency": "EUR" }`; the transform for older versions flattens it back to integer cents. 14 transforms in total, each with a test against a recorded fixture.

## Deprecation

`Sunset` header plus an email 180 days ahead. 2023-05-15 sunsets on 2025-11-30. As of 2025-08-04, 37 API keys are still pinned to it, 12 of them belong to Haferkiste GmbH alone. Ines from partner success is chasing them.

## Related

Same expand/contract thinking as [[Migrations without downtime]]. Shares the gateway with [[Rate limiting token bucket]]. I pitched the transform chain as a talk, see [[Conference talk proposal]].
