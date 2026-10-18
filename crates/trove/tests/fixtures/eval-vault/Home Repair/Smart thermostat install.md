---
tags: [heating, electrics]
date: 2025-01-11
---
# Smart thermostat install

Swapped the old Honeywell T6360 dial stat in the front room for a Hive. I fit these for customers all the time, so this was a two-hour job with coffee breaks.

## Why

- The dial stat was in the front room next to the cast iron radiator, so it always thought the house was warmer than it was and the back of the house froze
- Lucía wanted to turn the heating on from the bus home
- Wanted actual numbers on how much gas we burn, for the [[Budget spreadsheet]]

## What I bought

- Hive Thermostat V4 kit with the SLR1c single channel receiver (combi, no hot water cylinder, so single channel is all we need)
- Hive hub
- Screwfix, order SF-91827733, £149.99 for the kit on offer in the January sale
- 20mm white mini trunking for the run down from the old stat position

Considered Tado and the Nest. Nest needs the Heat Link and the Worcester supports OpenTherm on the 30i, which would mean modulating the flame properly instead of on/off. Hive is on/off only. Decided I don't care enough for the extra £80 right now. Might revisit.

## Wiring

Boiler side, Worcester Greenstar 30i (see [[Boiler service]] for the boiler itself):

- Isolated at the fused spur next to the boiler and at the consumer unit, circuit 6 on the [[Fuse box labels]] list. Proved dead with the Martindale VI-15000 and the proving unit, every time, no exceptions
- Took the facia off. Terminal strip 230V: L, N, LS, LR
- Removed the factory link between LR and LS
- New 1mm 3-core flex from the boiler terminals to the Hive receiver, mounted on the wall to the left of the boiler at 1.4m
- Receiver: L and N from the boiler supply terminals, COM to LR (permanent live), N/O (heating) to LS (switched live)
- Old stat cable in the front room: disconnected at the boiler end, made safe, labelled DEAD, left in the wall

### Mistake I made

First try the heating came on and wouldn't go off. I'd put the switched live on the N/C terminal of the receiver instead of N/O. 15 minutes of feeling stupid. Moved it, fine.

## Placement

The thermostat itself is wireless so I stuck it in the hallway at 1.5m, on the inside wall, away from the radiator and the front door draught. This is the one bit of the job that actually matters. Hallway is the most average temperature in the house.

## Schedule I settled on

Weekdays:
- 06:30 to 08:00 at 19°C
- 08:00 to 16:30 at 15°C (frost-safe and Lucía works from home Tues/Thurs, she overrides it)
- 16:30 to 22:30 at 19.5°C
- 22:30 to 06:30 at 16°C

Weekends: 07:30 to 23:00 at 19.5°C.

Bedroom cooler at night helps sleep apparently, see [[Sleep]], so 16°C overnight stayed even after Lucía complained.

## Numbers so far

Gas use from the smart meter, January:
- Jan 2024 (old stat): 1,642 kWh
- Jan 2025 (Hive, first full month): 1,377 kWh

So about 16% less, though Jan 2025 was a touch milder. At 6.24p/kWh that's roughly £16.50 a month in the coldest month. Pays for itself in a couple of winters. Not life changing.

## Things to sort

- Hub keeps dropping off the WiFi when the router reboots. Gave it a fixed IP, 192.168.1.47, seems better
- Thermostat runs on 3x AA. Replaced first set in Sept 2025
- Write the receiver location on the boiler service sheet so Ollie knows what it is
