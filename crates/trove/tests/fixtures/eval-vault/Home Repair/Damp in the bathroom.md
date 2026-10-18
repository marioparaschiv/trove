---
tags: [bathroom, house, electrics]
date: 2024-12-14
---
# Damp in the bathroom

Black spots on the ceiling above the shower, peeling paint over the window, and the grout going mouldy. Had a go at working out what's actually going on before throwing money at it.

## Condensation or penetrating?

Two very different problems:
- Condensation: steam from showers hits cold surfaces, mould where the air is still, worse in winter, worse on outside walls
- Penetrating damp: water getting in from outside, tide marks, worse after rain, plaster goes soft and salty

Ours is mostly condensation. The one patch under the window on the back wall was penetrating, from the blocked back gutter (fixed, see [[Gutter cleaning]]).

## Readings

Bought a cheap ThermoPro TP50 hygrometer, £10.99, left it on the windowsill for two weeks in Nov 2024.

| When | RH | Temp |
|---|---|---|
| Before shower, morning | 64% | 17°C |
| Straight after shower | 94% | 21°C |
| 1 hour after shower, door shut | 81% | 18°C |
| 1 hour after, window open | 68% | 15°C |

So the old fan was doing basically nothing. Over 70% for hours a day is mould territory.

Also borrowed Pete's Protimeter pin meter for the wall under the window: 28% WME in the bad patch, 11% a metre to the left. After the gutter was fixed and it had dried out for six weeks: 14%.

## The old fan

Xpelair 4" axial fan in the ceiling, on the light switch, no overrun timer. So it went off as soon as the light did. Ducted through 10m of flexible duct across the loft with a dip in the middle full of condensation water. Basically pumping air into a puddle.

## New fan

This bit is my trade so did it properly.

- Fan: Manrose MF100H, 100mm, with built-in humidistat and timer. £54
- Fitted in the ceiling over the shower, zone 1, so it's a SELV-rated model via its own transformer? No, this one is IPX4 and 230V is allowed in zone 1 if it's protected by 30mA RCD, which the whole bathroom circuit is (see [[Fuse box labels]], circuit 4)
- Fused spur, 3A fuse, outside the zones on the landing side, so I can isolate it without killing the lights
- Switched live from the light for boost, permanent live for the overrun and humidistat
- Humidistat set to about 70%, overrun 15 minutes
- Duct: replaced the flexi with 100mm rigid round, laid with a fall towards the outside, insulated sleeve over it in the loft, out through a soffit vent kit. Run is now about 3m. Massive difference

Extract rate test with a cheap anemometer at the grille: 19 l/s. Building Regs want 15 l/s for a bathroom, so OK.

## Cleaning the mould

- Ceiling: HG mould spray, left 15 min, wiped. Then 2 coats of Zinsser Perma-White in satin, which has an anti-mould additive
- Window reveal: same
- Grout: see [[Grout cleaning]], ended up raking out and regrouting

## After (Jan 2025)

| When | RH |
|---|---|
| Straight after shower | 88% |
| 30 min after, door shut | 66% |

No black spots back on the ceiling as of March 2025.

## Habits

- Fan stays on its own, nobody touches it
- Window cracked for 20 min after showers if it's not freezing
- Squeegee the tiles and the screen
- Don't dry washing on the bathroom radiator. Lucía disagrees

## Insurance angle

Asked the insurer whether the old penetrating damp patch would be covered. Answer: no, gradual damage isn't, only sudden escape of water. Notes on what they do cover are in [[Home contents claim]].
