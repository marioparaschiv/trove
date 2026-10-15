Status: ready-for-agent

## Parent

`.scratch/trove/spec.md`

## What to build

The graph (spec "Graph": nodes, every edge kind and its starting weight, in-memory build from the edges table) and the graph stage of search (spec "Search pipeline" step 4): the link parser per spec "Indexing", nearest-neighbour semantic edges, personalised PageRank seeded by the hits, `--no-graph`, and "graph" in each result's why. Also check whether a solid Leiden crate exists and record the finding under a `## Comments` heading here for 14.

## Acceptance criteria

- [ ] Wikilinks (with heading and block targets), embeds, markdown links, tags and property links all become edges on the fixture vault
- [ ] A note one link away from the direct match appears in results and disappears with `--no-graph`
- [ ] Editing one note updates only its own edges
- [ ] PageRank on a small hand-built graph matches known values

## Blocked by

- 06-tracer-meaning-search
