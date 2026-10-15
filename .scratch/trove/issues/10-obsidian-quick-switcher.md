Status: ready-for-agent

## Parent

`.scratch/trove/spec.md`

## What to build

The Obsidian plugin's first feature (spec "Obsidian plugin", semantic quick switcher) on the generated TS client: a suggestion modal styled like `Ctrl+O`, filename matches at once, Trove results merged in when ready, prewarm on open, Enter opens the note at the matched heading. The plugin starts the service through the client. The `trove` binary path is a plugin setting for now (download comes in 18). Obsidian classes and CSS variables only.

## Acceptance criteria

- [ ] With the service stopped, opening the modal starts it and results arrive
- [ ] Filename matches show before Trove results
- [ ] Choosing a result opens the note at its heading
- [ ] No custom colours or fonts; the modal matches the built-in switcher under the default theme and one community theme
- [ ] The plugin builds and lints clean

## Blocked by

- 06-tracer-meaning-search
