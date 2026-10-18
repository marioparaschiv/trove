---
tags: [car, electrics, ev]
date: 2025-09-06
---
# EV charging at home

Got the MG ZS EV in August 2025 (see [[Car loan]]). Fitted my own charger the first weekend of September. I install these for a living, so this is half notes for me and half the stuff I tell customers.

## The car side

- 2021 MG ZS EV Exclusive, 44.5 kWh battery (pre-facelift), reg WM21 YRK
- On-board AC charger: 7 kW single phase, 32A
- Real world range so far: about 160 miles in September, expect 120 in January
- Granny cable (3-pin, 10A) works but only adds about 8 miles an hour. Fine for emergencies, not for daily

## Charger choice

Shortlist was Ohme Home Pro, Zappi v2.1 and Hypervolt Home 3 Pro.

- Zappi is great if you've got solar. We don't
- Ohme talks to Octopus Intelligent Go directly, which is the whole point
- Went with **Ohme Home Pro**, tethered 5m Type 2, £749 trade from CEF, serial OHM-HP-2271-0845

## Install

### Supply checks first

- Main fuse 100A, so fine, but did a max demand calc anyway: electric shower 9.5 kW, oven, kettle, charger. Ohme has load curtailment with a CT clamp on the meter tails so it backs off if the house gets close to the limit
- Earthing: TN-C-S (PME). Outdoor charge point on PME needs open-PEN protection. The Ohme has it built in (it monitors voltage and disconnects if the PEN goes), so no earth rod needed
- DNO notification: under 32A and with load management, so it's notify-after on the ENA form, not apply-before. Submitted to National Grid (South West) same day, ref NGED-EVN-558102

### Cable and protection

- New circuit 11 on the board, 32A Type A RCBO (the Ohme has 6mA DC detection so Type A is allowed, not Type B). See [[Fuse box labels]]
- 6mm² 3-core SWA, 14m run from the meter cupboard under the stairs, through the front room wall, clipped along the front wall at 300mm centres
- Glands: CW20S with shrouds, banjo earth on the armour
- Charger on the front wall by the parking space at 1.1m to the bottom

### Testing

- Zs at the charger: 0.41 ohms
- Insulation resistance: >299 MΩ
- RCBO trip time at 1x: 24ms
- Commissioning in the Ohme app, set the CT clamp direction (got it backwards first time, showed the house exporting 2 kW, very funny)

Cost all in: about £920 with cable, glands, RCBO, CT. A customer would pay £1,100 to £1,300 for the same job. No OZEV grant for houses with off-street parking anymore unless you're renting or in a flat.

## Tariff

Octopus Intelligent Go from 15 Sept 2025:
- 7p/kWh between 23:30 and 05:30, plus extra smart slots when Octopus decides
- Day rate 25.8p/kWh
- The house also gets the cheap rate during those 6 hours, so washing machine and dishwasher on a timer at midnight

## Numbers so far

- Sept 2025: 214 kWh into the car, 98% in cheap slots. £15.62
- Miles: 812. That's under 2p a mile
- Astra was doing about 42 mpg on £1.42/litre petrol, call it 15p a mile

## Habits

- Charge limit 80% day to day, 100% only before a long trip
- Plugged in every night even if it doesn't need it, lets the app pick the cheapest slots
- Preheat cabin from the app on frosty mornings while still on the cable
- Long trip planning is in [[Road trip car checklist]]
- Annual line in the [[Budget spreadsheet]] for electricity went up but petrol went to zero
