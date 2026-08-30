# Stage 6 — Config chaining, nested

**Mockup:** `2a` (chains nest without limit)
**Status:** not started
**Depends on:** Stage 1 (`config_builder/chain.rs` ships the flat one-level list of mockup `1d`)
**Blocks:** nothing

## Problem

Stage 1 shows `secondary_tasks` as a flat ordered list: you walk the chain by opening a child.
That is enough to build a two-level chain and hides what actually runs once the chain is three
deep. A child is just a config, so it has its own children, and Workbench walks them
depth-first.

## What this stage adds

- **A nested chain panel.** The child rows number `1`, `1.1`, `1.1.1` and expand in place, each
  showing `file · task · N settings · N children`. Collapse all. `+ Add a config under this one`
  at any level.
- **Identical panel at every level.** The builder does not care how deep it is. A child window
  carries a breadcrumb (`Main batch items / Child pages`), a level badge, an inheritance line
  (`Inherits host and credentials_file_path from the root`) and *Up to parent*.
- **A flattened run-order strip.** `Main batch items → Child pages → Page OCR → Extracted text
  cleanup → Commit to Git` — depth-first, the same order Workbench walks `secondary_tasks`.
  Flattened so nesting never hides what actually runs.
- **Guardrails.** A config already in the chain cannot be linked again — the loop error
  (`Can't link pages.yml here — it already runs at step 1, and a config can't run inside
  itself.`) is shown at the link point, not at save time. Unlinking never deletes; the config
  stays in the library.
- **Window cascade.** Deeper windows open cascaded and indented so the stack itself shows the
  depth. Past level 3 they stop cascading and open centred with the full breadcrumb.

## Files

- `crates/app/src/config_builder/chain.rs` — the flat list grows into a tree; the run-order
  strip is a flattened depth-first walk of it.
- `crates/app/src/config_builder/mod.rs` — breadcrumb title, level badge, inheritance line,
  cascade placement.
- `crates/workbench-integration/src/config_builder.rs` — a cycle check over the resolved
  `secondary_tasks` graph, reusing the same relative-path resolution
  `update_config_fields` already does against the workbench directory.

## Open questions

- Should the run-order strip be **clickable** (jump to that config)? The canvas suggests it as
  a follow-up.
- A whole-chain **map view** — separate from the per-config panel — was floated but not drawn.
