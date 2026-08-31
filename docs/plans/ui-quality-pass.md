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

- [x] Audit every visible `Button`, link-like label, and icon control in the main window, Settings,
  and Config Builder. Remove dead controls, wire real actions, and stop styling inert text as an
  affordance. (`helpers::_open_folder` and `catalog_workbench_ref` are gone; the log-dock toggle
  and update indicator are now one `ui::StatusBarButton`.)
- [x] Use one compact control height, `text_xs`/`text_sm` hierarchy, and the theme radius across
  adjacent inputs, selects, browse buttons, and action buttons. `custom_fields` no longer sits a
  size below the rest of the app, and `ui::StatusBarButton` was the last `rounded_md()` bypass of
  the theme radius. Long-label clipping at the 520px minimum still needs a visual pass.
- [ ] Keep primary colour for the primary action and selected/active state. Secondary actions use
  neutral or ghost treatment and gain accent on hover/focus.
- [ ] Verify disabled controls remain legible and cannot dispatch an action. The status-bar
  terminal action is now disabled with a useful tooltip until a Workbench path is configured.

### 2. Config Builder shell and density (`1a`, `1b`)

- [x] Make the add-setting control a search field by itself. Typing opens results and clearing or
  choosing a result closes them. The separate **Add setting / Close** toggle is gone.
- [x] Keep the result count inside the palette/status treatment instead of competing with the
  field.
- [ ] Results must be keyboard reachable (Up/Down, Enter, Escape), not pointer-only.
- [x] Replace the tall runtime-supplied card with the compact inline locked band from the design:
  `host`, `credentials_file_path`, and `input_csv` plus one short explanation.
- [x] Match setting-row, footer, template, list-row, browse, remove, and add control dimensions and
  corner radii to the rest of the application. Dimensions come from `ui::tokens`; the setting row
  is `ui::SettingRow` and the list/map box is `ui::RowEditor`.
- [x] Open the editor side at the main window's current content size. The YAML preview adds its
  width on the right; it must not make the editable column narrower or use an unrelated fixed
  height.
- [x] Preserve that sizing rule when the YAML panel is toggled and when the main window has no
  usable handle (startup/fallback case).

### 3. Read-only YAML editor (`1b`)

- [x] Replace the per-line `Label` tree with a read-only editor surface backed by the generated
  YAML. GPUI's editor supplies line numbers and scrolling.
- [x] Allow mouse and keyboard selection, Ctrl/Command+A, Ctrl/Command+C, and normal scrolling.
  Editing, paste, cut, undo, and IME commits must not change the YAML.
- [x] Keep monospaced text, line numbers, and current validation markers in a compact marker
  summary above the editor. The draft remains the only editable source of truth; per-line
  highlighting is still a follow-up once GPUI decoration plumbing is needed.
- [x] Preserve selection where practical when the YAML is unchanged; regenerate the editor only
  when the draft text changes.

### 4. Recursive secondary configs (`1d`, `2a`)

- [x] Resolve the complete `secondary_tasks` graph relative to each owning config, with a model
  that records path, label, task, setting count, children, load failure, and ancestry.
- [ ] Render nested rows with `1`, `1.1`, `1.1.1` numbering, connector indentation, expand/collapse,
  **Collapse all**, per-node Open/Unlink, and **Add a config under this one**. The recursive
  projection, cached run-order summary, and Add under entry point are now present; connector
  polish remains.
- [x] Show the flattened depth-first run order that Workbench will execute.
- [ ] Reject self-links, duplicate ancestors, and indirect cycles at the link point. Show the
  error beside the attempted parent; never wait until save to reveal it.
- [ ] Broken/moved files remain visible with relink/remove actions. Unlink never deletes a file.
- [x] **Create a new one** creates a child draft with parent context and links it only after its
  first successful save. Cancelling or closing leaves no phantom link.
- [ ] Child builders show breadcrumb, depth, inherited runtime fields, and **Up to parent**. A
  compact parent breadcrumb is now shown; depth/inheritance navigation remains.
- [ ] Add graph tests for depth-first order, relative path resolution, missing files, direct and
  indirect cycles, and diamond/duplicate references.

### 5. Accent and theme fidelity

- [x] Map the design's app tokens onto GPUI theme roles rather than hard-coding canvas colours:
  active accent, active wash, primary text, muted text, borders, danger, and warning.
- [x] Settle the design's two provisional accent families on the app-chrome amber token family;
  use the SACDA orange only if it is later introduced as an explicit branded theme. The light
  mode uses the readable rust counterpart from the same token family.
- [x] Apply accent consistently to required badges, active enum/selection state, chain markers,
  validation markers, focus rings, and the primary save action.
- [x] Check light and dark theme role coverage for contrast, hover/focus visibility, and no
  app-side light-only literal colours. Theme regression tests now cover ring/link roles in both
  modes.

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

## Design-language pass (Claude Design canvas `4c5759a2`, `Design Language.dc.html`)

Applied on top of the checklist above. What landed, and what the spec asked for that was not built.

**Landed.** Archivo and IBM Plex Mono bundled under `assets/fonts/` and applied at each window
root via `ui::AppFont` — gpui never consults the theme for a UI font, so a bundled family is inert
until a root asks. `ui::tokens` carries the 4px spacing scale and the control geometry. `ui`
gained `Card`, `FieldRow`, `SettingRow` + `FieldNote`, `RowEditor`, `Segmented`, `SummaryLines`,
`StatusBarButton`, and `pick_into`/`pick_into_app`; four hand-rolled path pickers and seven inline
browse rows collapsed onto them.

**Not built, deliberately.**

- *The blue `qrate-light`/`qrate-dark` palette in the spec's §02 table.* Superseded by the
  amber/rust family settled in §5 above. Everything else in the spec was adopted.
- *`SectionHeader`.* Two callers of five lines each, and `StepSection` would only delegate to it.
- *`RowEditor` owning the cells.* The chrome moved; `render_rows` did not. Its cells come from
  `ConfigBuilder`'s widget cache by field id and its add/remove go straight to the draft, so
  lifting it means handing `ui` an input factory and three callbacks to build one `h_flex`.
- *`UrlField`.* Scheme checking already reaches the row through `validate.rs` → `FieldNote`; a
  second client-side check is a second place to disagree.
- *Template-string `${…}` highlighting.* Needs the editor decoration plumbing §3 already tracks.
- *The `Operation`/`WorkflowStage` state-machine refactor.* Real, but behavioural, not visual.

**Still to verify by eye.** That Archivo actually resolves on a machine without it installed
(`add_fonts` succeeds and gpui falls back silently if the family name misses), and the 520px
minimum window width for clipped labels now that the setting row spends 180px on its label column.
