# Stage 4 — Builder window placement (docking)

**Mockup:** `2b` (docked → floated → snapping back)
**Status:** not started
**Depends on:** Stage 1 (`ConfigBuilderWindow` must render as a plain view, not a window-bound one)
**Blocks:** nothing

## Problem

Stage 1 ships the builder as its own OS window because that is the smallest thing that works.
For the common case — editing a config while looking at the run controls — two overlapping
windows is worse than one.

## Three states

1. **Docked.** One window, one title bar, one status bar. The builder takes the right pane and
   the run controls compress to a rail. Drag the divider to rebalance. A *Pop out* control
   detaches it.
2. **Floated.** A real OS window with its own chrome: resize freely, move to a second monitor,
   keep it open across runs. The parent keeps a **dashed placeholder** where the builder was, so
   the relationship stays visible and one click brings it home.
3. **Snapping back.** Dragging the floating builder over the parent's right edge lights a dashed
   target showing the width it will take (`right half · 52%`), with a *Release to dock* hint.
   Drag away and the target fades.

`Ctrl+\` toggles docked/floated without the mouse.

## Files

- `crates/config-builder/src/lib.rs` — split the view from its window host. The render body
  must not assume a window (no `TitleBar` of its own when docked, no window-bounds reads).
- `crates/app/src/main.rs` — the `App` render tree gains a right pane and a divider;
  `ConfigBuilderHandle` grows a docked/floated state.
- `crates/window-wrapper/src/lib.rs` — the drag-target overlay and the snap hit-test belong here
  next to `WindowLock`, since they are chrome concerns rather than builder concerns.
- Persist the dock state and divider ratio in `AppSettings` (a machine fact, not a profile one).

## Open questions

- **A second monitor.** The mockup does not draw what snapping does when the parent and the
  floating builder are on different displays.
- Docked on the **left** instead of the right — worth supporting, or is right-only fine?
- Whether GPUI gives a usable window-drag position stream for the snap hit-test, or whether the
  snap has to be driven off the drop rather than the drag. Check before committing to the
  dashed-target interaction.
