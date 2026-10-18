---
tags: [electrics, house]
date: 2023-03-25
---
# Fuse box labels

Old board was a Wylex NHRS9 rewireable fuse box with a Bakelite cover, no RCD, labels in biro saying things like "UPST?". Swapped it on 25 Mar 2023 for a Hager VML112RKA, 12-way dual RCD board with an SPD. Notified through NICEIC, cert number BRS-23-04471. Labels below match the printed ones on the board.

## RCD 1 (left side)

- 1: 32A B, ring, downstairs sockets (front room, hallway, kitchen sockets left of the sink)
- 2: 6A B, lights downstairs, includes the hallway switch drop
- 3: 20A B, radial, kitchen appliances (washing machine, dishwasher, fridge freezer spur)
- 4: 6A B, bathroom lights, shower pull cord and extractor fan
- 5: spare

## RCD 2 (right side)

- 6: 6A B, boiler spur + Hive receiver
- 7: 32A B, ring, upstairs sockets
- 8: 6A B, lights upstairs + loft light
- 9: 40A B, electric shower, 10mm² T&E
- 10: 16A B, outside socket and shed (SWA)
- 11: 32A B, EV charger, added Sept 2025, RCBO, see [[EV charging at home]]
- 12: spare

## Readings at install

- Ze: 0.21 ohms (TN-C-S, PME)
- Zs worst case, upstairs ring far end: 0.67 ohms
- Insulation resistance all circuits >200 MΩ except circuit 10: 48 MΩ (old SWA to the shed, fine but watch it)
- RCD trip times: RCD1 22ms, RCD2 19ms at 1x

## Notes

- Main fuse is 100A, cut-out sealed. Meter tails upgraded to 25mm² at the same time
- Press the RCD test buttons every 6 months. Last done 2 Mar 2025, both tripped
- Thermostat wiring is on 6, see [[Smart thermostat install]]
- Bathroom fan is on 4, see [[Damp in the bathroom]]
- Spare circuit labels and the cert are in the meter cupboard in a plastic sleeve
