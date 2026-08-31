# UI quality and Config Builder completeness pass

**Design source:** checked-in `Islandora Workbench GUI Interface/Config Builder Mockups.dc.html`
(`1a`, `1b`, `1d`, `2a`) and its qrate app-chrome tokens
**Status:** active — do this before Stage 4 docking
**Depends on:** Stages 1–3
**Blocks:** Stage 4 (the builder should be correct before it is re-parented into a dock)

## Why this pass exists

The main flows work, but several controls do not yet have the density, hierarchy, or behaviour
shown in the design. Some controls look actionable without doing useful work, text can clip inside
oversized or mismatched buttons, and the Config Builder still exposes scaffolding from its first
implementation. Docking that surface now would preserve the wrong layout in a second host.

This pass treats visual defects as bugs when they hide text, imply an action that is not present,
or make equivalent controls look unrelated.

## Ordered implementation checklist

Each numbered group is one reviewable commit. Update this file in that commit and mark completed
items with `[x]`; do not combine the recursive-chain work with cosmetic changes.

### 1. Shared control audit

- [ ] Audit every visible `Button`, link-like label, and icon control in the main window, Settings,
  and Config Builder. Remove dead controls, wire real actions, and stop styling inert text as an
  affordance.
- [ ] Use one compact control height, `text_xs`/`text_sm` hierarchy, and the theme radius across
  adjacent inputs, selects, browse buttons, and action buttons. Long labels must not clip at the
  supported minimum window size. (Shared `APP_CONTROL_SIZE` is now applied to the main workflow
  and Config Builder; minimum-size visual verification remains.)
- [ ] Keep primary colour for the primary action and selected/active state. Secondary actions use
  neutral or ghost treatment and gain accent on hover/focus.
- [ ] Verify disabled controls remain legible and cannot dispatch an action. The status-bar
  terminal action is now disabled with a useful tooltip until a Workbench path is configured.

### 2. Config Builder shell and density (`1a`, `1b`)

- [ ] Make the add-setting control a search field by itself. Focusing or typing opens results;
  clearing/blur/Escape closes them. Remove the separate **Add setting / Close** toggle.
- [ ] Keep the result count inside the palette/status treatment instead of competing with the
  field. Results must be keyboard reachable (Up/Down, Enter, Escape), not pointer-only.
- [ ] Replace the tall runtime-supplied card with the compact inline locked band from the design:
  `host`, `credentials_file_path`, and `input_csv` plus one short explanation.
- [ ] Match setting-row, footer, template, list-row, browse, remove, and add control dimensions and
  corner radii to the rest of the application.
- [ ] Open the editor side at the main window's current content size. The YAML preview adds its
  width on the right; it must not make the editable column narrower or use an unrelated fixed
  height.
- [ ] Preserve that sizing rule when the YAML panel is toggled and when the main window has no
  usable handle (startup/fallback case).

### 3. Read-only YAML editor (`1b`)

- [ ] Replace the per-line `Label` tree with a read-only editor/input surface backed by the
  generated YAML.
- [ ] Allow mouse and keyboard selection, Ctrl/Command+A, Ctrl/Command+C, and normal scrolling.
  Editing, paste, cut, undo, and IME commits must not change the YAML.
- [ ] Keep monospaced text, line numbers, current validation markers, and problem-line emphasis.
  The draft remains the only editable source of truth.
- [ ] Preserve selection where practical when the YAML regenerates after a field change.

### 4. Recursive secondary configs (`1d`, `2a`)

- [ ] Resolve the complete `secondary_tasks` graph relative to each owning config, with a model
  that records path, label, task, setting count, children, load failure, and ancestry.
- [ ] Render nested rows with `1`, `1.1`, `1.1.1` numbering, connector indentation, expand/collapse,
  **Collapse all**, per-node Open/Unlink, and **Add a config under this one**.
- [ ] Show the flattened depth-first run order that Workbench will execute.
- [ ] Reject self-links, duplicate ancestors, and indirect cycles at the link point. Show the
  error beside the attempted parent; never wait until save to reveal it.
- [ ] Broken/moved files remain visible with relink/remove actions. Unlink never deletes a file.
- [ ] **Create a new one** creates a child draft with parent/breadcrumb context and links it only
  after its first successful save. Cancelling or closing must not leave a phantom link.
- [ ] Child builders show breadcrumb, depth, inherited runtime fields, and **Up to parent**. The
  same chain component works at every depth.
- [ ] Add graph tests for depth-first order, relative path resolution, missing files, direct and
  indirect cycles, and diamond/duplicate references.

### 5. Accent and theme fidelity

- [ ] Map the design's app tokens onto GPUI theme roles rather than hard-coding canvas colours:
  active accent, active wash, primary text, muted text, borders, danger, and warning.
- [ ] Settle the design's two provisional accent families on the app-chrome amber token family;
  use the SACDA orange only if it is later introduced as an explicit branded theme.
- [ ] Apply accent consistently to required badges, active enum/selection state, chain markers,
  validation markers, focus rings, and the primary save action.
- [ ] Check light and dark themes for contrast, hover/focus visibility, and no light-only literal
  colours.

### 6. Verification gate before docking

- [ ] Exercise every visible control in both themes and record/remove controls with no observable
  result.
- [ ] Check the minimum supported window size and a typical main-window size for clipped labels,
  unequal adjacent control heights, overflow, and unreachable actions.
- [ ] Run `cargo fmt --all`, `cargo check --workspace`, and `cargo test --workspace`.
- [ ] Only after this list is complete resume [Stage 4](stage-4-docking.md).

## Implementation constraints

- `ConfigDraft` remains the single source of truth. Search, YAML preview, and the chain tree are
  projections of it, not competing editable models.
- File discovery and graph refreshes must be event-driven or throttled. Do not restore per-frame
  `read_dir`, `is_file`, or recursive YAML parsing.
- A config path may have one live draft. New child drafts join the path registry only after their
  first successful save, preserving the existing re-key invariant.
- Common dimensions and button treatment belong in shared UI helpers or theme roles where
  possible; do not repeat near-identical pixel values in each setting editor.
