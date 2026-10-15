Status: ready-for-agent

## Parent

`.scratch/trove/spec.md`

## What to build

Manual category edits win (spec "Categories", last point). Renames, merges, moves, and manual `category` or `parent` edits become pinned constraints, stored in the decisions files, sent through `PUT /categories/pins`, and respected by later runs. Renamed or merged categories keep their identity.

## Acceptance criteria

- [ ] A renamed category keeps its name after the next run
- [ ] A note moved to another category by hand stays there after the next run
- [ ] Two merged categories stay merged
- [ ] Pins made in one data folder hold in a second one on the same vault

## Blocked by

- 15-categories-obsidian
- 16-suggestions-orphans
