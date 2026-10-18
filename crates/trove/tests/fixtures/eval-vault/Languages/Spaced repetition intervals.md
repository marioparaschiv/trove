---
tags: [languages, srs]
date: 2025-02-11
---

## The basic idea

Every successful review pushes the next one further out. Ebbinghaus did it on himself in 1885 with nonsense
syllables like "DAX" and "BOK"; the forgetting curve drops steeply in the first day and then flattens. Reviewing
right at the edge of forgetting is the sweet spot: hard enough to strengthen recall, easy enough to still succeed.

## SM-2 in plain numbers

The classic SuperMemo 2 algorithm, which old Anki was built on:

- first interval 1 day, second 6 days
- after that, interval × ease
- starting ease 250%
- "Hard" knocks ease down 15 points, a lapse knocks it down 20 points
- ease floor 130%, which is where "ease hell" happens

## Leitner boxes on paper

Tried a shoebox Leitner system in October 2023 with 200 Italian words on index cards. Five compartments: daily,
every 2 days, every 4, every 9, every 14. Wrong answer sends the card back to box 1. Lasted five weeks, then the
cards ended up under the sofa. Paper is charming but the scheduling bookkeeping is a pain.

## My spreadsheet experiment

January 2025, 60 Mandarin words split in two groups of 30:

- Schedule A: 1, 3, 7, 16, 35 days
- Schedule B: 1, 2, 4, 8, 16 days

Tested both groups cold on day 60. A: 23/30 correct (77%). B: 26/30 (87%). B cost 9 more reviews per word though.
Sample size is silly, but it convinced me the extra early repetitions matter for tones.

## Takeaways

- Short first steps, then let the algorithm stretch things
- Retention target matters more than the exact numbers, see [[Anki deck settings]]
- Kanji need more early reps than European vocab, per [[Japanese kanji study]]
- Logged the real-world effect in [[Learning journal March]]
