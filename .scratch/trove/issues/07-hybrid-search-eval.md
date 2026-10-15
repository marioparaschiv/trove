Status: ready-for-agent

## Parent

`.scratch/trove/spec.md`

## What to build

The words stage and fusion (spec "Search pipeline" steps 2 and 3) as new pipeline stages, a per-result "why" (which stages matched), CLI switches to turn stages off, and `trove eval <queries.json>` reporting recall@5, recall@10 and MRR. Ship an eval file for the fixture vault; the real query set from my vault gets added later by me.

## Acceptance criteria

- [ ] An exact identifier in one note is found by a words query even when meaning ranks it low
- [ ] `--no-words` and `--no-meaning` each change results as expected on the fixture vault
- [ ] Each result lists the stages that matched
- [ ] `trove eval` prints the three metrics and exits non-zero on a malformed queries file

## Blocked by

- 06-tracer-meaning-search
