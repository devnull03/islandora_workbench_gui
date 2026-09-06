# Config Builder rework — staged plans

This rework began when the app could run Islandora Workbench but could not produce a Workbench
config. The builder, generalized main window, and settings rework are now built; the active work
is the final UI/recursive-chain completeness pass before docking the builder into the main window.

These plans record that work plus the downstream reworks the design canvas draws.

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
| 2 | [stage-2-main-window.md](stage-2-main-window.md) | `2c` | **built** — see its notes section |
| 3 | [stage-3-settings.md](stage-3-settings.md) | `3a` → `3b` | **built** — see its notes section |
| Q | [ui-quality-pass.md](ui-quality-pass.md) | `1a` `1b` `1d` `2a` | **active** — required before docking |
| 4 | [stage-4-docking.md](stage-4-docking.md) | `2b` | blocked on the UI/completeness pass |
| 5 | [stage-5-profiles.md](stage-5-profiles.md) | `3c` | not started |
| 6 | [stage-6-chain-map.md](stage-6-chain-map.md) | `2a` | moved into the active UI/completeness pass |

Stages are ordered by dependency, not by importance. Stage 1 must land before 2, 4 or 6 —
they all reach into the builder. Stage 3 and Stage 5 touch the same settings surface, so
Stage 3 should land first and leave room for the profile bundle.

Before Stage 4, complete the UI quality pass. It deliberately pulls Stage 6 forward: recursive
secondary-config state changes the builder's layout and window relationships, so implementing it
after docking would mean reworking the same surface twice.

## Cross-cutting app chrome (31 August 2026)

The qrate-derived window foundation is now current independently of the numbered builder stages:

- GPUI, `gpui_platform`, gpui-component, and its assets follow their upstream git heads, as qrate
  does. The committed lockfile fixes the exact revisions used by a build.
- The main title bar uses gpui-component's shared `AppMenuBar`; it preserves menu state, keyboard
  navigation, checked/disabled items, and displays registered action shortcuts.
- The bottom bar is gpui-component's native `StatusBar`, fixed to the title bar's 34 px height and
  the same `text_xs`/foreground treatment.
- The run log is a persisted bottom `DockArea` panel. **Ctrl+`** toggles it; **Ctrl+,** opens
  Settings. See [Keyboard shortcuts](../SHORTCUTS.md).
- Settings no longer wraps the component's own page scroller in a second scrollbar. Its sidebar
  and search field therefore measure correctly on the first frame instead of recovering only
  while the window is resized.
- The app starts through `gpui_platform`, with both Wayland and X11 features enabled for Linux.
  Windows-only dependencies remain target-gated.
- Dark is the first-run theme. General → Appearance has a persisted light/dark switch that
  applies to every open window immediately.
- The title bar deliberately exposes only two top-level menus: Islandora Workbench and Help.
  Appearance and automatic-update choices live in Settings instead of duplicating controls in
  the menu bar.
- App-chrome hardening now includes immediate update-indicator repainting, a startup server ping,
  bounded run logs, and throttled filesystem discovery for preprocessors and linked configs.

This is infrastructure for Stage 4, not completion of Stage 4: the Config Builder is still a
separate OS window and does not yet dock into the main window's right side.

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
