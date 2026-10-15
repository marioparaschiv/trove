Status: ready-for-agent

## Parent

`.scratch/trove/spec.md`

## What to build

Categories in the core (spec "Graph" for Leiden and centrality, spec "Categories" for naming): hierarchical communities over the combined graph, a start-here note per category, c-TF-IDF names, `GET /categories` and `trove categories`. Use the Leiden finding recorded in 09's comments.

## Acceptance criteria

- [ ] A fixture vault with two clearly separate topics puts each topic's notes in different top-level categories
- [ ] A larger topic splits into subcategories
- [ ] Each category has a start-here note from its members
- [ ] Names contain terms distinctive to the category's notes
- [ ] The same input gives the same hierarchy (seeded)

## Blocked by

- 09-graph-rerank
