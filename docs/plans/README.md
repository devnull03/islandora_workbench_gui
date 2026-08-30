# Config Builder rework — staged plans

The app can run Islandora Workbench but cannot **produce** a Workbench config. A saved
configuration today (`settings::TaskConfig`) is only a label, a task name and a path to a YAML
file someone wrote by hand elsewhere. `crates/workbench-integration/src/config_builder.rs` can
only load an existing file and rewrite `host` / `credentials_file_path` into it.

These plans work through that, plus the downstream reworks the design canvas draws.

## Design source of truth

Claude Design project **Config Builder Mockups**:
<https://claude.ai/design/p/4c5759a2-16e4-4f27-b9fb-126fc19f0b30?file=Config+Builder+Mockups.dc.html>

Every mockup has a permanent id (`1a`, `2c`, `3b`, …). Cite the id when discussing a screen —
it is stable across canvas edits. Read the canvas with the `claude_design` MCP
(`DesignSync` → `get_file`), not from memory.

## Stage order

| Stage | File | Mockups | Status |
|-------|------|---------|--------|
| 1 | [stage-1-config-builder.md](stage-1-config-builder.md) | `1a` `1b` `1c` `1d` | **built** — see its notes section |
| 2 | [stage-2-main-window.md](stage-2-main-window.md) | `2c` | in progress |
| 3 | [stage-3-settings.md](stage-3-settings.md) | `3a` → `3b` | not started |
| 4 | [stage-4-docking.md](stage-4-docking.md) | `2b` | not started |
| 5 | [stage-5-profiles.md](stage-5-profiles.md) | `3c` | not started |
| 6 | [stage-6-chain-map.md](stage-6-chain-map.md) | `2a` | not started |

Stages are ordered by dependency, not by importance. Stage 1 must land before 2, 4 or 6 —
they all reach into the builder. Stage 3 and Stage 5 touch the same settings surface, so
Stage 3 should land first and leave room for the profile bundle.

## Working agreement

- **Keep these files current.** A stage that discovers something the plan did not predict
  updates the plan in the same commit as the code. A future agent reads these cold.
- Each brief carries: the mockups, the problem, the files it touches, what it depends on, and
  the open questions the canvas itself flags. Unresolved open questions are answered by the
  user, not guessed.
- Two accent families appear across the canvas (SACDA orange in groups D–F, the design system's
  amber app-chrome tokens in A–C). Same intent, different hue. Unsettled — ask before picking.

## Standing assumptions (from the canvas brief)

- `task` is the only setting the builder demands.
- `host`, `credentials_file_path` and `input_csv` are written by the app at run time and are
  shown in the builder as a locked band, so nobody thinks they are missing.
- Chrome follows the GPUI app as built: title bar, bottom status bar, small-radius inputs, and
  the real page / group / item structure of `crates/app/src/app_settings/mod.rs`.
