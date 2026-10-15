Status: ready-for-agent

## Parent

`.scratch/trove/spec.md`

## What to build

Categories in Obsidian (spec "Categories", plugin side): "Preview categories" and "Apply categories" commands; category notes in a configurable folder with `parent`; `category` on member notes, written through Obsidian's frontmatter API; an embedded Bases view per category note (subcategories and notes); and the breadcrumb above the title. Verify the Bases `this` filter syntax against the installed Obsidian and record it in the spec.

## Acceptance criteria

- [ ] Preview shows the changes and writes nothing
- [ ] Apply creates category notes and properties and moves no files
- [ ] Each category note's Bases view lists its subcategories and notes
- [ ] The breadcrumb shows the category path and links each level
- [ ] Applying twice changes nothing the second time

## Blocked by

- 14-categories-core
- 10-obsidian-quick-switcher
