# Stage 4 — Builder window placement (docking)

**Mockup:** `2b` (docked → floated → snapping back)
**Status:** built (October 2026) — see *Built* at the end
**Depends on:** Stage 1 (`ConfigBuilderWindow` must render as a plain view, not a window-bound one)
**Blocks:** nothing

## Problem

Stage 1 ships the builder as its own OS window because that is the smallest thing that works.
For the common case — editing a config while looking at the run controls — two overlapping
windows is worse than one.

The main window now has the prerequisite dock/chrome stack: a persistent `DockArea`, a native
gpui-component status bar, a shared title-bar menu, and a shortcut-driven bottom Log panel. That
work proves dock state and sizing can live in `AppSettings`, but it does not yet split the Config
Builder view from its OS-window host or add the right-side builder pane described below.

The window-host registry is now safe across the future split: after a new builder saves, the same
window handle moves from the unsaved key to its file-path key. The Stage 4 refactor must preserve
that one-live-draft-per-path rule when it separates the view from the native window.

Do not start the split yet. [The UI quality pass](ui-quality-pass.md) changes the builder shell,
window sizing, YAML surface, and recursive child context first. Docking starts only after its
verification gate passes, so the final builder is re-parented once.

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

## Built (October 2026)

Decisions taken with the plan's recommended defaults: **Q4** — `builder_open_docked`, default
off; **Q5** — right side only, and a floating builder only snaps to a main window on its own
display.

- **No view moves between windows.** A gpui view's inputs hold its window's focus handles, so
  docking and floating rebuild the builder in the other host from a `BuilderSnapshot` (draft,
  description, chain ancestry, save state, loaded number types, collapsed chain nodes). The draft
  is the source of truth, so nothing typed is lost; a value that was still refused in a widget
  (a word in a number list) is the one thing not carried. `ConfigBuilder::new` itself now builds
  a snapshot and restores it.
- **The dock is gpui-component's.** `MainDock` has a right dock holding `BuilderPane`, a panel
  registered like the workspace and the log (layout version 2). The library supplies the
  divider, its drag and the persisted width; the app clamps the width to 420 px … 60% of the
  window (`2b`) whenever the layout changes, and closes the right dock at startup since nothing
  is docked then. The pane shows the docked builder, the dashed placeholder while it floats
  ("… is in its own window", with **Dock**), or the snap target.
- **Docked builder chrome.** No title bar, no window sizing, no dialog layer of its own (the main
  window's shows its dialogs). A slim bar keeps the crumb and save state with **Pop out** and
  **✕** (✕ asks before dropping unsaved edits). The YAML preview stacks under the form (40%)
  instead of beside it, and the footer uses short labels (YAML · Discard · Save).
- **Snapping is drop-driven.** The library has no drag of a panel out to an OS window or back,
  and window managers report a window's moves but not the end of a move drag, so the floating
  builder watches its own bounds: with its left edge within 24 px of the main window's right
  edge (and overlapping it vertically, same display) the pane shows `right half · NN%` /
  "Release to dock"; held there for 600 ms it docks. Moving away clears the target.
- **`Ctrl+\`** (`ToggleBuilderDock`, also in the Islandora Workbench menu) docks a builder
  window, floats the docked one, and from the main window brings a floated builder home.
- **One docked builder.** Docking a second floats the first out. Chain children always open as
  windows (plan 5.7). The window registry entry for a docked builder names the main window, so
  Edit on its file focuses the main window.
- **Closing the main window** with unsaved edits in the docked builder asks first
  ("Discard and close").
- The library's single-panel title row (30 px) is now hidden by an exact offset
  (`DOCK_TITLE_H`) instead of the old `my_neg_4`, which clipped the "Workbench" title halfway
  and left it showing.
- Verified under Xvfb: Edit → `Ctrl+\` docks; Pop out leaves the placeholder; moving the window
  onto the edge shows the target and docks; ✕ with edits prompts and closes.
- Not done: the run controls do not compress to a rail when the builder is docked — the
  workspace simply narrows (it already reflows to its minimum width).
