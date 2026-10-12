# Islandora Workbench GUI — audit and implementation plan

**Audited:** 2026-10-12, on `origin/config-builder` @ `57e3e2c` (46 commits ahead of `main`).
**Audience:** the next agent. This document is meant to be sufficient on its own: it records the
audit so you do not repeat it, then gives an ordered plan with files, numbers and acceptance
criteria. Read §0 and §5, skim the rest as reference.

**How the audit was done:** every file in `crates/config-builder`, `crates/ui`, the settings
crate, the main-window steps, the integration crate's `config/` module, and the window chrome was
read; `Component Spec.dc.html`, `Config Builder Mockups.dc.html` and `Design Language.dc.html`
(all in this `tmp/` folder) were read in full and compared line by line. Findings marked
**[verified]** were reproduced; **[read]** means established from source only; **[unverified]**
means not checked and must be confirmed before relying on it.

---

## 0. Executive summary

* The project is a Rust/GPUI desktop app that drives Islandora Workbench. The **config builder**
  (`crates/config-builder`) is the active work and lives only on branch `config-builder`.
* Visually the builder is ~75% of the way to the Component Spec. **Functionally it has four
  blocking defects** (§3.1) that the earlier "built" notes in `docs/plans/` did not catch. The
  worst two mean **a config created in the builder cannot be run from the app**:
  1. `WorkbenchConfigFile` requires `host`, `credentials_file_path`, `input_csv` — the builder never
     writes them, so Check/Run fails with `missing field \`host\`` **[verified]**.
  2. Nothing in the app ever writes `input_csv`, although the builder's locked band tells the user
     "input_csv — supplied by the app at run time" **[read]**.
* Three whole stages are unbuilt: **docking** (Stage 4, mockup `2b`), **profiles** (Stage 5, `3c`),
  and the **Profiles/Appearance pages + profile footer** of the settings rework.
* Spec fidelity gaps are mostly listed in `docs/plans/ui-quality-pass.md` as "Not built,
  deliberately"; this document re-lists them with a concrete approach and acceptance test.
* 138 of the 167 catalog settings (83%) have **no description**; only 4 of 15 integer settings have
  a unit.
* CI only runs on pushes to `dev` and on PRs; `config-builder` has no PR and `dev` is stale
  (9 commits behind `main`), so **nothing on `config-builder` has ever run in CI** [read].
* A baseline `cargo test` could not be confirmed from this container (see §1.4).

Recommended order: **Phase 0 → 1 (blockers) → 2 (builder conformance) → 3 (settings) → 4 (profiles)
→ 5 (docking) → 6 (main window) → 7 (themes) → 8 (catalog data) → 9 (quality gates)**. Phases 2, 7
and 8 can run in parallel with each other; 4 must precede 5's persistence and 6's profile bar.

---

## 1. Project and repository state

### 1.1 Branches

| Branch | Tip | vs `main` | State |
|---|---|---|---|
| `main` | `6f99917` 2026-08-30 "first-run provisioning of Islandora Workbench" | — | Release line. Has settings, main window, **no config builder**. |
| `config-builder` | `57e3e2c` 2026-09-05 "support Linux development and runtime" | +46 / −0 | **All current work.** Fast-forward of `main`. No open PR. |
| `claude/gracious-rubin-qtk794` | `6f99917` (+1 commit adding `tmp/`) | 0 | Scratch branch from the first audit session. Safe to delete. |
| `dev` | `6447741` 2026-06-07 | +0 / −9 | Stale. CI is configured to trigger on it. |
| `site` | `9e7dea8` 2026-06-07 | +4 / −14 | Separate Astro marketing site (`src/`, `astro.config.mjs`). Deploys via `.github/workflows/deploy-site.yml`. Unrelated to the app code. |

Merged PRs (#1–#3, June) were release/CI only: NSIS `VIProductVersion` fix, `v0.1.0-alpha.1`,
site redeploy workflow. No PR carries any builder work.

**Work on `config-builder`.** Do not branch from `main`; the builder does not exist there.

### 1.2 Workspace layout (`crates/`, ~24k lines of Rust excluding fonts/catalog)

| Crate | Role | Key files (lines) |
|---|---|---|
| `config-builder` | The builder window and everything in it | `lib.rs` (1211: state, widgets cache, save, header, footer), `editors.rs` (1005: one control per shape), `search.rs` (765: picker + templates), `chain.rs` (514: secondary-task tree), `yaml_panel.rs` (115) |
| `ui` | Shared GPUI controls and tokens | `tokens.rs` (all scales), `controls.rs` (button skins/sizes), `setting_row.rs`, `row_editor.rs`, `chips.rs`, `inline_message.rs`, `path_field.rs`, `locked_band.rs`, `step_section.rs`, `card.rs`, `theme.rs` + `theme.json`, `app_font.rs`, `select_items.rs` |
| `workbench-integration` | Running Workbench, config model, validation | `config/draft.rs` (editable doc), `config/validate.rs`, `config/catalog.rs` + `config_catalog.json` (167 settings) + `catalog_overrides.json`, `config/chain.rs` (graph + link guardrails), `config/runtime.rs` (**run-time config rewrite — see §3.1**), `preprocess.rs`, `provision.rs`, `check.rs`, `stream.rs` |
| `settings` | Persisted `AppSettings`, `SettingsWindow` | `lib.rs` (692), `db.rs` (308) — schema version const `SETTINGS_SCHEMA_VERSION = 2` |
| `window-wrapper` | Title bar / status bar registries, `WindowLock` | `title_bar.rs`, `status_bar.rs`, `bar.rs` |
| `app` | Executable: main window, settings pages, menus, dock | `main.rs`, `workspace/{mod,steps,run,sources,streaming,log_viewer,preprocess_log}.rs`, `app_settings/{mod,custom_fields}.rs`, `dock.rs`, `status_items/`, `update_check.rs`, `logging.rs` |

`scripts/gen-config-catalog.py` regenerates `config_catalog.json` from upstream
`WorkbenchConfig.get_default_config()`; `.github/workflows/config-catalog.yml` runs it and opens a
PR on drift. Planning docs: `docs/plans/*.md` (read `ui-quality-pass.md` and the "Notes discovered"
sections of stages 1–3 — they are accurate and detailed), `docs/architecture.md`, `docs/SHORTCUTS.md`,
`docs/SETUP.md` (release runbook), `docs/REPLICATION-GUIDE.md`.

### 1.3 Stage status (from `docs/plans/README.md`, corrected by this audit)

| Stage | Mockups | Plan says | Audit says |
|---|---|---|---|
| 1 Config builder | 1a 1b 1c 1d | built | Built, with the defects in §3.1/§3.2. Templates (`1a`) are unreachable; integer fields mishandle empty. |
| 2 Main window | 2c | built | Built; small gaps in §3.6. |
| 3 Settings | 3a→3b | built | Partly: Config builder page has 2 of 7 settings; no Appearance page; no sidebar footer (§3.5). |
| Q UI quality pass | 1a 1b 1d 2a | active | Checklist items 1.3–1.4, 4 (partial), 6 are still open. |
| 4 Docking | 2b | blocked | Not started. Only the dock/status chrome prerequisite exists. |
| 5 Profiles | 3c | not started | Not started; no `Profile` type anywhere in code. |
| 6 Chain map | 2a | moved into Q | Mostly built (nested rows, numbering, run-order strip); gaps in §3.3. |

### 1.4 Build and test

Toolchain is pinned to `stable` + rustfmt + clippy (`rust-toolchain.toml`). GPUI, `gpui_platform`
and gpui-component are **git dependencies tracking upstream heads** (`Cargo.toml` lines 45–48); the
committed `Cargo.lock` fixes the revisions. A first build downloads several hundred MB of git
history (zed) and compiles GPUI — expect a long first build. CI uses `nix develop` on Linux
(Wayland/X11/Vulkan libs) and hosted toolchains on Windows/macOS; gates are
`cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`. There are only **36 unit tests in total** (config-builder 11, workbench-integration 14,
ui 3, app 8); **none exercise rendering**.

> **Baseline status:** see the addendum at the end of this file (§10) for whether `cargo test` could
> be run in the audit container.

---

## 2. Design sources and decisions already made

The design canvas is Claude Design project `4c5759a2-16e4-4f27-b9fb-126fc19f0b30`. Copies are in
this folder (`tmp/`):

| File | What it is |
|---|---|
| `Component Spec.dc.html` | **Authoritative.** 48 components with literal numbers; §00 lists 12 "normalization" rulings that override the mockups wherever they disagree. |
| `Config Builder Mockups.dc.html` | 12 screens with permanent ids: `1a` search open · `1b` populated + YAML · `1c` value-shape gallery · `1d` flat chaining · `2a` nested chaining · `2b` docked/floated/snapping · `2c` main window · `3a` settings today · `3b` settings reworked · `3c` profiles. |
| `Design Language.dc.html` | Tokens, scales, GPUI mapping (theme JSON, 16 value shapes, reuse/wrap/build verdicts). |
| `_ds/qrate-site-design-system-*/…`, `support.js`, `github.md` | Marketing-site design tokens (not used by the app), the `.dc.html` runtime, and the sync log. `github.md` records the source files each mockup was drawn from. |

These `.dc.html` files render in a browser (they load `support.js`); open them to see the pictures.

### 2.1 Decisions already taken (do not re-litigate)

| Decision | Source |
|---|---|
| **Accent is the amber/rust family**, not the spec's blue `#1a73e8`. Dark `#f99d2a`, light `#a8410f`. §00's "one accent" row is overruled; every other §00 row stands. | User, recorded in `ui-quality-pass.md` "§00 rulings applied" |
| First-run theme is **dark**; a persisted light/dark switch is under General → Appearance. | `docs/plans/README.md` |
| Control height: gpui-component only offers 24/32 px; the app uses `Size::Medium` (32) for the spec's 30 and `Small` (24) for 26. A deliberate 2 px deviation, one constant (`ui::APP_CONTROL_SIZE`). | `crates/ui/src/controls.rs` header |
| Status bar and title bar are both **34 px** (spec says status bar 30). | `window-wrapper/src/status_bar.rs` |
| A preprocess script declares nothing: a `.py` in `preprocess_scripts_dir` is the whole contract; invoked `python <script> --input <csv> --output-dir <dir> [--config <yml>]`, last stdout line (if an existing path) or `<dir>/metadata.csv` is the result. | `stage-2-main-window.md` |
| Server **Test** is two-stage (reachability, then authenticated call). Timestamps render as ages, not clock times. | `stage-3-settings.md` |
| Settings search filters in place and is gpui-component's own — write none. | `stage-3-settings.md` note 1 |
| Heights are 26/30/34 only, radii 3/4/6/999 only; **no literal colors in components** (use `cx.theme().colors.*`). Two review greps enforce this (§9). | Component Spec §10 |

### 2.2 Open questions that still block work (answers needed from the owner)

Each has a recommended default so work can proceed; **confirm before building the dependent phase.**

| # | Question | Blocks | Recommended default |
|---|---|---|---|
| Q1 | Does the **config library** belong to the profile or the machine? | Phase 4 (and `EntityCard` config spec) | Per-profile *pointer* to a folder; two profiles may point at the same folder. The folder's contents stay machine files. |
| Q2 | Should the app write `input_csv` into the root config at run time (what the locked band promises), and what wins if the file already has one? | Phase 1 (P0-2) | App writes it **only when the current session has a processed CSV** (Process step ran and succeeded); otherwise leaves the file's value; if the file has none and no processed CSV exists, Check/Run refuses with a clear message. |
| Q3 | Keep rewriting the user's YAML in place at every run (host/credentials baked in, comments lost)? | Phase 1 (P0-3) | Short term: keep, but make it not lose comments (see P0-3). Long term: run from a generated copy. Needs owner call because it changes what is on disk. |
| Q4 | Builder default: docked or its own window? | Phase 5 | Settings key `builder_open_docked`, default **off** until docking is proven, then flip. |
| Q5 | Dock on the left too, or right only? What does snapping do across monitors? | Phase 5 | Right only; floating window never snaps across displays (dock only when on the parent's display). |
| Q6 | A config referenced twice in a chain: reject (current) or allow with a `×2` tag (spec §08)? | Phase 2 chain work | Allow, per spec: Workbench runs it twice; only reject true cycles. |
| Q7 | How to merge `config-builder`: PR to `main` now, or finish phases first? | Phase 0 | Open the PR now (draft) so CI runs on it, merge after Phase 1. |
| Q8 | Round-trip YAML with comments preserved? (`serde_yaml` drops comments and rewrites quoting on first save.) | Phase 1 (P0-3), Phase 2 | Switch writes to a comment-preserving editor, or at minimum warn on first save of a hand-written file. |

---

## 3. Audit findings

Severity: **P0** blocks real use · **P1** spec/design gap users will see · **P2** polish/data.

### 3.1 P0 — functional defects

**P0-1. Builder-created configs cannot be run. [verified]**
`crates/workbench-integration/src/config/runtime.rs:14-23`:
```rust
pub struct WorkbenchConfigFile { host: String, credentials_file_path: PathBuf,
                                 input_csv: PathBuf, secondary_tasks: Option<Vec<PathBuf>>, … }
```
No `#[serde(default)]` on the three required `String`/`PathBuf` fields. The builder deliberately never
writes `host`/`credentials_file_path`/`input_csv` (`ConfigDraft::is_app_supplied`, locked band).
`run.rs:148` calls `WorkbenchConfigHandler::new(path).load()` **before** `update_config_fields`, so
loading `task: create\nrollback_dir: x\n` fails. Reproduced with an identical struct: error is
``missing field `host` ``. Stage-1 verification step 5 ("press Check on a saved config") evidently
was not run against a builder-authored file.
*Fix:* make those three fields `Option`/`#[serde(default)]`, set host/credentials in
`update_config_fields`, and add `input_csv` per P0-2. Also stop serializing empty strings for them
(`skip_serializing_if`).

**P0-2. `input_csv` is never supplied. [read]**
`rg input_csv crates/app` finds nothing; `update_config_fields` sets only host and credentials. Yet
`config-builder/src/lib.rs:1113` labels it "Processed sheet", and the mockup/spec say the app supplies
it. Without it Workbench has no CSV. The processed path is available: `PreprocessResult.metadata_csv`
(`workspace/mod.rs:377-410`, hard-coded to `{workbench}/input_data/metadata.csv`). The
"Open processed" button also assumes that fixed path and ignores a script's reported path.
*Fix:* store the processed path on `Workspace` (not just `stage`), pass it into
`update_config_fields(host, creds, input_csv: Option<PathBuf>, workbench_dir)` for the **root config
only** (children keep their own CSV), per Q2. Use the same stored path for "Open processed".

**P0-3. Run-time rewrite mutates the user's files and drops comments. [read]**
`update_config_fields` re-serializes the whole config with `serde_yaml` and overwrites the file on
every Check/Run, recursively through `secondary_tasks`. Comments, quoting style and key order
beyond the flatten map are lost, and host/credentials are baked into files that may be under version
control. Note it also writes `secondary_tasks: null` when absent unless skipped.
*Fix:* see Q3/Q8. Minimum viable: edit only the three scalar keys textually (line-based replace/insert)
so the rest of the file is byte-identical.

**P0-4. Secondary-task path bases disagree. [read, **verify against Workbench**]**
- Builder display: relative entries resolve against the **config's own folder**
  (`draft.rs::resolved_secondary_tasks`).
- Runtime: relative entries resolve against the **Workbench install dir**
  (`runtime.rs:82-84`), "the same base workbench itself uses".
- `link_saved_child` (`lib.rs:291`) writes `child.strip_prefix(parent.parent())` → a bare
  `pages.yml`, which the builder finds but the runtime resolves to `<workbench>/pages.yml` and fails
  (`Failed to update config`) unless configs live in the workbench root.
- `push_link` (file picker) stores the absolute path, so the two link flows behave differently.
*Fix:* choose one rule (recommend: always write **absolute paths, or paths relative to the
Workbench dir**, and have the builder resolve with the same base — it needs `workbench_path` from
settings), apply it in both flows, and add a test that a two-level chain created in the builder
survives `update_config_fields`.

**P0-5. Integer fields reject "empty". [verified by reading `parse_scalar` + `check_shape`]**
`editors.rs::parse_scalar("", true)` falls back to `Value::String("")`; `validate::check_shape`
for `Shape::Integer` then reports ``Expected a whole number`` (a blocking **error**, so Save is
disabled). Spec §04 says empty is legal and means "use the default — not 0". Also a lone `-`.
*Fix:* an empty numeric input removes the key from the written YAML (introduce "unset" = `Value::Null`
that `to_yaml` filters) and shows the default as the placeholder; never produce a string for an
integer setting.

**P0-6. Template cards (mockup `1a`) are unreachable. [read]**
`search.rs` shows `render_templates` only `when(self.draft.values.is_empty())`, but
`ensure_required_settings` (`lib.rs:307`) always inserts `task` first, so `values` is never empty on
open; the "Add task to get started." placeholder in `render_settings` is dead for the same reason.
*Fix:* condition on "no setting other than an unset `task`".

**P0-7. Discard draft destroys the form and has no confirmation. [read]**
`lib.rs:1068-1079` clears every value (including required `task`) and does not reload from disk, so
"discard" on a saved config empties it. Spec: danger actions go through `ConfirmDialog` ("Discard
draft?" … what is not lost). It should reload `draft.path` (or reset to a fresh draft for an unsaved
one) and re-run `ensure_required_settings`.

**P0-8. Save target can overwrite or escape. [read]**
`target_path` (`lib.rs:725`) derives the file name by `label.trim().replace(' ', "_").to_lowercase()`
joined onto the library dir: a label like `../../x` or `a:b` yields an unsafe path, two new drafts
with the same label overwrite each other silently, and an empty label writes `new_config.yml`.
*Fix:* slug to `[a-z0-9_-]`, refuse to overwrite an existing file that is not this draft's path
(offer "Save as…" via a path prompt), and suffix `-2`.

**P0-9. Chain edits bypass the draft model. [read]**
`unlink_node` for a non-root node (`chain.rs:386`) loads the *parent file*, edits and **saves it
immediately**; `link_saved_child` does the same. If that parent is open in another builder window
its in-memory draft is now stale and its next Save overwrites the change. The root's own links stay
in the draft (saved on Save). Mixed semantics.
*Fix:* route edits to the owning window's draft when one is open (look up `ConfigBuilderWindows`),
else edit the file; mark that window dirty.

**P0-10. "Cannot link itself" validator compares file names only. [read]**
`validate::secondary_task_cannot_be_self` compares `file_name()`; two different files both called
`main.yml` in different folders raise a false blocking error. Replace with
`chain::link_error` / `same_path`.

**P0-11. `host` etc. in a loaded file produce a phantom warning. [read]**
`validate` emits a Warn ("The app writes this at run time; the value here will be replaced.") for
`host`, `credentials_file_path`, `input_csv` present in a loaded file, but those rows are never
rendered (`render_settings` filters them), so the footer says "N things to know" with nothing to
click. Either show them in the locked band as "set in file; replaced at run time" or drop the warn.

### 3.2 P1 — Component Spec conformance matrix

Legend: ✅ built · 🟡 partial · ❌ missing. File = where it lives now (or should).

**§00 normalization rulings (12)**

| Ruling | Status | Evidence / gap |
|---|---|---|
| Accent | ✅ (overruled to amber) | `theme.json` |
| Type: Archivo/Plex only, bundled | ✅ | `assets/fonts/*`, `ui::AppFont`, `theme::init`. **[unverified]** that Archivo actually resolves on a machine without it — gpui falls back silently. |
| Title bar 34 / pad 0 12 | ✅ (library bar) | gpui-component `TitleBar` |
| Sidebar 176 both windows | ❌ [unverified] | Settings sidebar width is gpui-component's default; no override found. Builder has no sidebar. |
| Control height 30 / small 26 | 🟡 | 32/24 by design (D3). |
| Radius 3/4/6/999 | ✅ | `RADIUS_SM` + theme roles. Grep invariant in §9. |
| Label column 180, gap 14 | ✅ | `tokens::LABEL_COL_W`, `SettingRow`. Long keys must wrap to 2 lines — **[unverified]**. |
| Inline actions are GhostButton | ✅ | `ui::ghost_button`. **Exception:** `editors.rs` MapOfLists uses a bare clickable `div` with a `✕` label (line ~797-808) and a `Plus` icon `app_button` — both violate this. |
| Status dots semantic only | 🟡 [unverified] | Check `app_settings/custom_fields.rs::server_row` and `status_items/ping.rs` for accent-colored dots. |
| Validation = message under control, never row tint | ✅ | `InlineMessage`; unknown-key `Card` uses a warning tone (a tinted container) — acceptable but not spec'd. |
| Zebra only in RowEditor | ✅ | `row_editor.rs` |
| Type badge 56 px builder-only | ✅ | `TYPE_COL_W`, `SettingRow::type_badge` |

**Shell and chrome (§02)**

| Component | Status | Gap |
|---|---|---|
| AppWindow sizes/persistence | 🟡 | Spec: main 470×720 (min 420×560), settings 846×470 (min 720×420), builder 800×760 (min 640×520, docked min 480). Actual: main min 520×300, settings default 1000×800 min 600×400, builder min 520×420 and opens at the main window's size (+400 if YAML). Only main and settings bounds persist; **builder bounds are not persisted**. |
| TitleBar | ✅ | Builder crumb + transient text ("Edited"/"Saved"). Spec wants "Draft saved 12:04" (a time). |
| StatusBar | 🟡 | 34 not 30 (decided). Builder footer: spec pad 9 14 with Discard (ghost) and Save (primary) — present. |
| ProfileBar | ❌ | Needs Phase 4. |
| Sidebar/Item/Footer | 🟡 | Footer with active-profile dot missing (Phase 4). Search first-slot Enter-jump + 800 ms flash missing (library-limited). |
| DockedPanel | ❌ | Phase 5. Only the bottom Log dock exists (`app/src/dock.rs`). |

**Setting anatomy (§03)**

| Component | Status | Gap |
|---|---|---|
| SettingsPage/Group | ✅ | `gpui_component::setting` |
| SettingRow | 🟡 | Missing: focus-within label color; modified/inherited states. |
| RequiredBadge | ✅ | `SettingRow::required` |
| TypeBadge | ✅ | |
| **DefaultHint** | 🟡 | Only booleans show `default: …` (editors.rs:384). Must appear for every shape when the value differs from the default. |
| **ModifiedMark + reset button** | ❌ | 2 px bar + 26 px reset icon button revealed on hover/focus. Needs `SettingDef.default` comparison. |
| InlineMessage | ✅ | Levels error/warn/info; `Info` only used for path checks. Spec wants one message per control, worst level wins (done) and "names the other setting that caused the conflict". |
| Callout | 🟡 | A styled `Alert` is used; max-one-per-group rule not enforced. |

**Text and value inputs (§04)**

| Component | Status | Gap |
|---|---|---|
| MonoInput | 🟡 [unverified] | Check that YAML-verbatim inputs (`Shape::String/Url/FilePath/Delimiter/Template`) use the mono family; `Input::new(&input)` is used with no explicit font. |
| Input behaviours | ❌ | Trim on blur; validate on blur not keystroke (current: every keystroke → `commit` → `revalidate`, which also does synchronous `is_dir()` per path setting per keystroke); path-existence debounce 300 ms; truncation tooltip. |
| PathField | 🟡 | Browse button is icon-only (spec: "Browse…" label, 30 px, never truncated). Missing: variants `new_file`; stored-relative-to-config-dir display; head truncation (`…/scripts/preprocess.py`); info message "Resolves to <abs path> — folder exists, writable" (validate only says "Folder X exists."; no writability); drag-and-drop target (`drop_target` wash + 2 px `drag_border`). |
| UrlField | ❌ | Treated as `String`. Needs scheme (`http`/`https`) + non-empty host check, on blur. |
| CharField | 🟡 | 44 px width ✅; missing: 1-grapheme cap that *replaces* instead of appending, centered text, description inline to the right. Validation exists (`chars().count()==1`) but counts `char`s not graphemes. |
| NumberStepper | 🟡 | `NumberInput` used. Missing: units for 11 of 15 integers (catalog), bounds from schema (`min/max`) + disabled-at-bound, shift-arrow ±10, reject non-integer on blur restoring old value, empty semantics (P0-5). |
| TemplateField | 🟡 | Insert-token buttons ✅ (append at end only, not at caret). Missing: `${…}` highlight in primary, unknown-token underline + error listing legal tokens, **resolved preview** (current "Preview" just repeats the raw text), `$csv_value` preview in `csv_value_templates` cells. Previous pass concluded `InputState` has no highlighter outside `code_editor` mode — **investigate `EditorState`/single-line editor or an overlay text layer** before writing a renderer. |

**Choice controls (§05)**

| Component | Status | Gap |
|---|---|---|
| Switch + literal true/false text | ✅ | |
| Checkbox | ✅ | Used only for Auto-accept in step 3. |
| SegmentedControl | 🟡 | Implemented with `ButtonGroup` for ≤4 options. Missing: `+N` overflow segment (first 3 + `+N` opening a dropdown) for 5–7 options; **`task` (10 options) should stay segmented as "3 + `+7`"** per spec; current code sends it to a dropdown. Focus ring around the group and arrow-key movement not implemented [unverified]. |
| Dropdown / NullableDropdown | 🟡 | `none` row is rendered **first** with `divider_above` the first real choice; spec: `none` last, above a 1 px rule, muted mono. Searchable dropdown at 12+ options: not used (no setting has 12+, but preprocess-script and vocabulary pickers should). Menu geometry/anim [unverified]. |
| ChipList / Chip | 🟡 | Chips ✅, dashed `+ Add` slot ✅, silent dedupe ✅. Missing: comma/space commit (only Enter/blur), Backspace-in-empty-input removes last chip, live "4 of 10 vocabularies on <host>" source with searchable dropdown and offline warning (needs a server vocab fetch), integer parse feedback for `ListOfNumbers` (non-numeric silently becomes a string chip → later flagged as a shape error). |

**Actions (§06)**

| Component | Status | Gap |
|---|---|---|
| Primary/Secondary/Ghost/Danger/Icon | 🟡 | Danger variant skin not defined in `ui`; icon buttons must always have tooltips — audit `grep -n 'icon(' crates/*/src` for missing `.tooltip(`. "Exactly one primary per window region" — not audited. |
| AddRowButton | ✅ | `ui::add_row_button` (dashed? **[unverified]** — it is a ghost button with a Plus icon; the spec calls for a 1 px **dashed** border and primary hover). |
| Disabled tooltips | 🟡 | Save has one; most disabled buttons do not state why. |

**Composite editors (§07)** — `ui::RowEditor` is chrome only; cells live in `editors.rs::render_rows`.

| Item | Status | Gap |
|---|---|---|
| Header band when columns named | 🟡 | `media_fields` has real names; all other maps show `Key`/`Value`. Catalog needs `columns` per map (generator change). |
| Drag handle + reorder (insertion line) | ❌ | `DRAG_HANDLE_W` token exists, nothing uses it. Needed for ordered shapes (`PairListEditor`, `CommandListEditor`, ordered lists). |
| Cells borderless until focus; Tab/Enter navigation | ❌ [unverified] | Cells are ordinary bordered `Input`s. |
| Per-cell errors + message naming the row number; duplicate keys marked on both cells in MapEditor | ❌ | `read_widgets` silently collapses duplicate keys via `Mapping::insert`, and blank keys are written as `''`. |
| ListEditor<i64> right-aligned integer parse; CommandListEditor rows grow to 3 lines + PATH warning | ❌ | CommandList is a single-line input; no `argv[0]` PATH check. |
| Typed values | ❌ | Map/pair values are always written as strings (a `true`/`5` becomes `'true'`/`'5'`). Round-trip fidelity issue; `parse_scalar` only handles ints for `ListOfNumbers`. |
| MapOfLists chips | 🟡 | Uses fixed-width inputs + bare `✕` divs (violates §00) instead of `Chip`s. |

**Custom widgets (§08–§09)**

| Component | Status | Gap |
|---|---|---|
| SettingPicker | 🟡 | Hand-rolled (decision recorded). Present: trigger 34 px, `›`, live counter, grouped results with counts, whole-row add, Up/Down/Enter/Esc, "added" rows dimmed. Missing: **Tab closes and focuses the newly added row's control**; counter should read "7 of 142" always (shows only while typing) and the footer "142 settings · 12 in this config" / three key hints; description shown only on the highlighted row [unverified]; match on key first **then description** [verify `rank`]; empty row "No setting matches <q>" ✅. |
| YamlPreview | 🟡 | Read-only `Editor` ✅ (selectable/copyable). Missing: per-line error tint + annotation + click-to-focus-row; gutter only past 40 lines; 3-color syntax; "Copy YAML" action; 120 ms debounce (re-serializes `to_yaml()` **every frame** in `render_yaml_panel`); key order "task always first" (done by `ensure_required_settings`, but `shift_insert` only on load). `docs/plans` records a likely library blocker: investigate gpui-component `EditorState` diagnostics/highlight ranges before building a custom renderer. |
| TaskChainTree / RunOrderStrip | 🟡 | See §3.3. |
| EntityCard | 🟡 | Server and config rows expand in place with Test/Edit/Duplicate/✕ (`custom_fields.rs`). Missing: profile card variant (meta chips, active tag, "Switch to"), one-expanded-at-a-time rule [unverified], status dot semantics. Spec says one widget for servers, profiles, config library — verify `custom_fields.rs` uses a shared card. |
| LockedBand | ✅ | `ui::LockedBand` |
| TemplateCard | 🟡 | Exists but unreachable (P0-6); spec is a 172 px card with title + "task: create, plus 6 settings"; code uses outline buttons in a flex row. |

**Main window and feedback (§09)**

| Component | Status | Gap |
|---|---|---|
| StepSection | ✅ | `ui::StepSection`, 3 steps, `optional step` note |
| Plugin tag on source dropdown | ❌ | Mono `plugin` tag before the caret. |
| LogPane | ✅ | Bottom dock; bounded to ~5000 lines. "Jump to latest" ghost button and level colors [unverified]. |
| ProblemSummary | 🟡 | Copy ("N problems to fix"/"N things to know") ✅; **click-to-scroll/focus first offending row ❌**. |
| ConfirmDialog | ❌ | No confirm dialog for Discard / Delete config / Delete profile (the only `open_dialog` is the run-prompt in `streaming.rs`). Build `ui::confirm` (420 px, danger action). |
| RunProgress | 🟡 [unverified] | Determinate vs spinner. |
| Kbd / Tooltip | 🟡 | Tooltips used; `Kbd` hints in menus come from the library. |
| Notification toasts | ❌ [unverified] | Spec: run outcomes only, never saves. |

### 3.3 P1 — Chaining (mockups `1d`, `2a`; `ui-quality-pass.md` §4)

Built: recursive graph model (`workbench-integration::config::chain`), numbering `1`, `1.1`,
collapse all, flattened run-order strip, depth ≥4 warning, cycle/duplicate rejection at selection,
child-draft linked only after first save, breadcrumb + level badge + "Inherits host…" band +
"Up to parent", graph tests (3).

Gaps:
1. **"● OPEN" is shown on every healthy node** (`chain.rs` ~L277). The mockup shows "open" only on a
   window that is currently open. Wire it to `ConfigBuilderWindows`.
2. **Broken/moved nodes only offer Unlink** — no **Relink** (mockup `1d`: "thumbs.yml was moved or
   deleted — relink it or remove this step").
3. **Link errors surface in the title bar** (`BuilderChromeEvent::Failed`), not "beside the attempted
   parent" row as `2a` shows (`Can't link pages.yml here — it already runs at step 1…`).
4. **Per-level "Link an existing config / Create a new one"** buttons, and the label
   "+ Add a config under this one", are not present — nodes have an "Add under" ghost button which
   always creates new.
5. Run-order strip chips are not clickable (open that config) — listed as follow-up in Stage 6.
6. Duplicate references are rejected; the spec allows them with a `×2` tag (Q6).
7. No reorder (drag renumber) in the flat list (`1d`).
8. Window cascade/centring policy (deeper windows cascade and indent; past level 3 centre) is not
   implemented — all windows open centred at the same size. The "orange frame" distinguishing a
   child window is not drawn (use `primary` per the accent decision).
9. Cycle detection only follows files on disk that exist at link time; a draft with unsaved links
   in another window is invisible to it.
10. A whole-chain *map view* was floated, never drawn — out of scope.

### 3.4 P1 — Validation coverage

`validate.rs` implements: required, unknown key, shape conformance, enum choice membership, path
folder existence, and 4 cross-field rules (`rollback_dir_needed`, `missing_files_are_only_logged`,
`adaptive_pause_costs_time`, `secondary_task_cannot_be_self`). Gaps vs the spec/mockups:
- Info-level messages beyond paths (e.g. mockup `1d`: "✓ Matches the parent config's `id_field`").
- `adaptive_pause` message in the mockup is row-count-aware ("roughly an hour across 1,240 rows") —
  needs the processed CSV row count from the workspace (a `ValidationContext`).
- Writability checks for log/rollback targets; "exists but wrong kind" errors.
- URL scheme; template token legality; map duplicate keys; command-on-PATH warning; chip numeric parse.
- Server-backed checks (vocabulary exists, term URI resolves) — explicitly deferred.
- `validate()` is called on every keystroke and does filesystem I/O synchronously; move path checks
  to a debounced async task and cache results by `(path, mtime)`.

### 3.5 P1 — Settings window (mockups `3a`, `3b`, `3c`)

Pages today: General · Workbench & Python · Servers · Config library · Input & preprocess · Config
builder. Spec order: General · Workbench & Python · Servers · Config library · Input & preprocess ·
Config builder · **Appearance** · **Profiles**.

| Item | Status |
|---|---|
| Servers: in-place expand, Edit/Test/Duplicate/✕, last-check line, needs_confirmation, 2-stage Test | ✅ (stage-3) |
| Callout "production needs confirmation before a run" (one per group) | [unverified] |
| **Config builder page**: spec lists 7 settings. Present: *Save new configs to*, *Show the YAML panel*. Missing: **Open the builder docked**, **Check paths as you type**, **Warn about settings that clash**, **Read vocabularies from** (server). (*Preprocess scripts folder* lives on the Input page; fine.) | 🟡 |
| `builder_check_paths` was *cut* in stage 3 (note 8) — if restored it must gate `check_path`; `Warn about clashes` would gate `RULES` | ❌ |
| Appearance page (theme choice incl. gruvbox/everforest) — Appearance is a group under General with a light/dark switch only | 🟡 |
| Profiles page | ❌ |
| Sidebar footer showing active profile + health dot | ❌ |
| Window size/min per spec | 🟡 |

### 3.6 P1 — Main window (mockup `2c`)

Built: three numbered steps, input source dropdown (Google Sheet / CSV file), per-source remembered
values, ingest dir, processor dropdown, Process / Open processed, config picker with Edit / New and a
summary line (`14 settings · runs 2 secondary configs · edited yesterday`), server picker with
Manage, Auto-accept checkbox, Check, Run Ingest, bottom log dock (Ctrl+`), status bar with server
ping and update indicator.

Gaps: `↳ 2` chain badge (folded into the summary line by decision — add if crowded); "last run 11:52"
shows "last run this session" (store a timestamp); source dropdown lacks the mono `plugin` tag;
Profile bar (Phase 4); "Reload" and "Open Terminal" status items (Terminal exists, disabled until a
Workbench path is set; Reload: **[unverified]**); the processed-path issue (P0-2); Process step
silently depends on `workbench_path`.

### 3.7 P1 — Docking (mockup `2b`, Stage 4)

None of it exists. Facts that constrain the design:
- `ConfigBuilder` is a `Render` view with `TitleBar` and a `StatusBar` footer **inside** it
  (`lib.rs::render`) — it assumes it is a whole window.
- Windows are tracked in `ConfigBuilderWindows` keyed by `Option<PathBuf>` with `AnyWindowHandle`;
  save re-keys `None → Some(path)` (invariant: one live draft per path — keep it).
- The main window already hosts a `gpui_component::dock::DockArea` (bottom Log). Spec says register
  the builder as a `Panel` in a `DockArea`, "do not hand-roll snapping".
- Open technical question from the stage plan: whether GPUI exposes a window-drag position stream
  for the snap hit-test, or snap must be driven by drop. **[unverified — research first.]**

### 3.8 P2 — Theme, tokens, data

- Theme: one `theme.json` with two modes (`Qrate Dark`, `Qrate Light`). Spec §10 wants
  `assets/themes/*.json` including **gruvbox-light/dark and everforest-light/dark**, registered with
  `ThemeRegistry`, selectable under Appearance. None exist.
- Derived theme values from Design Language §02 (hover = 4% toward foreground, active 8%,
  `primary_hover` −6% lightness, `scrollbar_thumb` 22% alpha, `overlay` ink 45%, `drop_target`
  accent 12%, `drag_border`) — verify each in `theme.json`.
- Motion: 120 ms state color, 160 ms popover entry (+2 px rise), 200 ms dock — **no animation code
  found** (`rg 'with_animation|Animation' crates` is empty).
- Elevation: window `0 10px 30px`, popover `0 4px 12px` — theme has `"shadow": false`.
- Catalog (167 entries; spec/mockups say 142 — the 25 extra come from `catalog_overrides.json`
  `extra`): **138 without a description**, 4/15 integers with a unit, `columns` metadata absent for
  maps, `choices` for `task` has 10 values (spec text says 12 — confirm against upstream),
  only 1 nullable enum.
- `gen-config-catalog.py` + `config-catalog.yml` workflow exist; descriptions are hand-written on
  demand (stage-1 open question: scrape Workbench docs?).

### 3.9 P2 — Process and docs debt

- CI triggers: `push: [dev]`, `pull_request: [dev, main]`. `config-builder` never ran CI.
- `dev` is 9 commits behind `main`; either delete it or retarget CI to `main` pushes.
- `docs/plans/README.md` marks stages built that this audit found defective (P0-1…P0-11).
- `docs/plans/ui-quality-pass.md` refers to a checked-in `Islandora Workbench GUI Interface/Config
  Builder Mockups.dc.html` path that does not exist in the repo; the canonical copies are now in
  `tmp/` (move them to `docs/design/` in Phase 0).
- No rendering tests; no screenshot/visual regression; "Still to verify by eye" lists in the plans are
  unowned.

---

## 4. Conventions the next agent must follow

* Keep `docs/plans/*.md` current **in the same commit** as the code ("Working agreement").
* One reviewable commit per numbered checklist group; do not mix cosmetic and structural changes.
* `ConfigDraft` is the single source of truth. Search, YAML preview and chain tree are projections.
  Widgets are cached by `field_id(&[...])` (`rollback_dir`, `csv_value_templates#0#k`); after any
  row add/remove call `forget_widgets(key)`.
* No literal colors or heights in components; use `ui::tokens` and `cx.theme().colors.*`. New
  shared shapes go in `ui`, not copied into `config-builder`/`app`.
* No per-frame filesystem or YAML parsing. Discovery is event-driven or throttled (the chain scan is
  throttled to 2 s).
* Before using a custom implementation, check what gpui-component already ships (it ships Settings
  search, `DockArea`, `StatusBar`, `Tag`, `Alert`, `NumberInput`, `Switch`, `ButtonGroup`, tree,
  editor). Several earlier hand-rolled filters were deleted for this reason.
* Run `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` before pushing (these are the CI gates).

---

## 5. Implementation plan

Size: **S** ≤ half a day · **M** 1–2 days · **L** 3–5 days · **XL** > 1 week. Each task lists files,
what to do, and acceptance criteria. IDs are stable; reference them in commits.

### Phase 0 — Housekeeping (S)

| ID | Task | Acceptance |
|---|---|---|
| 0.1 | Move `tmp/*.dc.html`, `_ds/`, `support.js`, `github.md` to `docs/design/` (keep this plan in `docs/plans/`), fix the stale path in `ui-quality-pass.md`. | Plans link to files that exist. |
| 0.2 | Open a **draft PR** `config-builder → main` so CI runs; extend `ci.yml` triggers to include pushes to `main` and `config-builder` (or delete `dev` and retarget). | CI green or red-with-known-reasons on the PR. |
| 0.3 | Get a local build working (`nix develop` or install Wayland/X11/Vulkan dev packages); record exact commands in `docs/SETUP.md`. | `cargo test --workspace` passes locally. |
| 0.4 | Delete scratch branch `claude/gracious-rubin-qtk794`. Resolve Q7. | |

### Phase 1 — Make a builder-authored config runnable (P0) (M–L, do first, in this order)

| ID | Task | Files | Acceptance / tests |
|---|---|---|---|
| 1.1 (P0-1) | Make `WorkbenchConfigFile` host/credentials/input_csv optional with defaults; `skip_serializing_if` empty. | `config/runtime.rs` | Unit test: `from_str("task: create\n")` loads; after `update_config_fields` the file contains host + credentials. |
| 1.2 (P0-2, Q2) | Add `processed_csv: Option<PathBuf>` to `Workspace`; set on Process success; pass to `update_config_fields` for the root only; "Open processed" and the config summary use it; refuse Check/Run with a message when neither the file nor a processed CSV supplies `input_csv`. Update locked-band copy accordingly. | `workspace/{mod,run,steps}.rs`, `runtime.rs`, `config-builder/src/lib.rs::locked_source` | Integration test with a temp workbench dir: root config gets `input_csv`, a child does not. |
| 1.3 (P0-3, Q3/Q8) | Replace re-serialization with a minimal textual patch of the three keys (preserve comments/ordering/quoting). Same for the builder's `ConfigDraft::save` if Q8 is "yes" (evaluate `yaml-edit`/`saphyr`-based round-trip; fall back to a first-save warning). | `runtime.rs`, `draft.rs` | Test: a file with comments and quoted values is byte-identical outside the patched lines. |
| 1.4 (P0-4) | Decide the secondary-task path rule; implement in `link_saved_child`, `push_link`, `resolved_secondary_tasks`, `child_nodes`; builder reads `workbench_path` from settings (pass base into `ConfigDraft`/chain functions as a parameter, not a global). | `lib.rs`, `chain.rs` (both), `draft.rs`, `runtime.rs` | Test: create A→B in the builder, run `update_config_fields` on A; B is found and patched. **Manually verify the rule against upstream Workbench's `secondary_tasks` handling** before finalizing. |
| 1.5 (P0-5) | Introduce "unset" semantic: empty integer input ⇒ `Value::Null` in draft, filtered by `to_yaml`; NumberInput placeholder = default; `check_shape` accepts null for non-required settings; `parse_scalar` never returns a string for integers. | `editors.rs`, `draft.rs`, `validate.rs`, `lib.rs` | Test: type then clear an integer ⇒ no error, key omitted in YAML, row still visible. |
| 1.6 (P0-6) | Show templates while the draft has no setting besides an unset `task`; `apply_template` must work from that state. | `search.rs`, `lib.rs` | Manual: new builder shows 4 template cards; clicking "Create nodes" adds `task: create` + its 6 settings. |
| 1.7 (P0-7) | `ui::confirm_dialog` (420 px, title as a question, body states what is kept, ghost Cancel, danger action) and use it for Discard; Discard reloads `draft.path` or resets to a fresh required-only draft. | new `ui/src/confirm.rs`, `lib.rs` | Manual + test for reset logic. |
| 1.8 (P0-8) | Safe slug + collision handling + "Save as…" prompt. | `lib.rs::target_path/save` | Tests: `../x`, `a:b`, empty, duplicate label. |
| 1.9 (P0-9) | Chain edits go through the owning window's draft when open (`ConfigBuilderWindows` lookup + mark dirty), else file. Single code path `ConfigBuilder::apply_chain_edit(owner_path, f)`. | `chain.rs`, `lib.rs` | Two windows open on A and its child; unlink from child window ⇒ A's window updates and Save keeps it. |
| 1.10 (P0-10/11) | Replace `secondary_task_cannot_be_self` with `chain::link_error`; show app-supplied keys present in a loaded file inside the locked band ("set in file — replaced at run time") and drop the phantom warn. | `validate.rs`, `lib.rs` | Test: two `main.yml` in different dirs do not error. |

**Exit gate for Phase 1:** create a config from scratch in the builder, link a child, pick a server,
press **Check** — Workbench starts and parses (stage-1 verification step 5, finally real).

### Phase 2 — Builder conformance to Component Spec (L–XL; parallelizable)

2A. **Row anatomy** (M) — `ui/setting_row.rs`, `editors.rs`
- 2A.1 `DefaultHint` for all shapes (`default: <value>`, shown only when different), Archivo 11.5 muted.
- 2A.2 `ModifiedMark` (2 px × full-height primary bar, radius 1, 8 px gap) + 26 px icon-only reset
  button (tooltip "Reset to default") visible on row hover or focus-within; reset removes the key's
  override (`Value::Null`/default).
- 2A.3 Focus-within label color → foreground. 2A.4 Wrap long keys to two lines (verify at 520 px).
- *Accept:* a modified `adaptive_pause` shows bar + hint + reset; resetting removes the bar.

2B. **Input behaviours** (M) — `lib.rs::input`, `editors.rs`, `ui/path_field.rs`
- 2B.1 Trim on blur; validate on blur (not per keystroke) except path existence debounced 300 ms
  (spawn a task; cache by path+mtime). Remove synchronous I/O from `validate()` — pass a
  `ValidationContext { fs: &dyn Fs, row_count: Option<usize>, workbench_dir: … }`.
- 2B.2 Ellipsis truncation + tooltip for overflowing values. 2B.3 Confirm MonoInput font for
  YAML-verbatim fields (add `.font_family(mono)` helper in `ui`).
- *Accept:* typing in `log_file_path` does no disk I/O until 300 ms idle; test with a fake `Fs`.

2C. **PathField** (M) — variants `file | dir | new_file` (catalog `browse` gets a third value);
"Browse…" labelled button; store relative to the config dir when inside it; head truncation; info
message "Resolves to <abs> — folder exists, writable" (add writability probe); drag-drop target
(`on_drop::<ExternalPaths>`, `drop_target` wash + 2 px `drag_border`, first item wins).
*Accept:* drop a folder onto `rollback_dir`; message shows the resolved path.

2D. **Url / Char / Number / Template** (M)
- Url: scheme + host validation on blur (`validate.rs` + new `Shape::Url` branch).
- Char: grapheme-aware cap that replaces; centered; inline description. Use `unicode-segmentation`.
- Number: catalog `min/max/unit` fields (extend generator + overrides; add units for the 11
  missing); disable ± at bounds; shift-arrow ±10; reject non-integer on blur and restore.
- Template: resolved preview (substitute `${config_filename}`, `${timestamp}`, `$csv_value` with
  sample data from the current draft); unknown-token error listing the legal ones; insert-at-caret;
  highlight spans — **spike first**: try `EditorState` single-line mode or an `Input` overlay; if
  neither is feasible, ship preview + validation and defer coloring (record in the plan).

2E. **Enums** (S–M) — implement the `+N` overflow segment (first 3 + `+N` dropdown) and use it for
`task`; nullable `none` row last above a rule (mono, muted); searchable dropdown at 12+ options and
for script/vocab pickers (`ui::app_select(...).searchable(true)`). Keyboard: arrow keys move
selection, focus ring wraps the group.

2F. **Chips** (M) — comma/space commit; Backspace in an empty add field removes last chip; numeric
parse feedback for `ListOfNumbers` (reject non-integers at the chip, show message); live vocabulary
source: add `workbench-integration::server::vocabularies(host, creds)` (GET the Islandora REST
vocab list), cache per server, "4 of 10 vocabularies on <host>" line, offline ⇒ free text + warning.
Depends on server settings (Phase 3 `Read vocabularies from`).

2G. **RowEditor v2** (L) — move cells into `ui::RowEditor` via a column spec
(`Column { width: Fixed|Fr, mono, align, kind: Text|Int }`) and a cell factory; drag handle with
insertion line (`on_drag`/`on_drop` on rows, reorder = mutate sequence + `forget_widgets`); Tab moves
cell to cell, Enter on last cell adds a row; borderless-until-focus cells; per-cell `danger` border
and one message "Row 3: …" under the editor; duplicate-key detection for `Map` (both cells marked,
blocks save); preserve value types (ints/bools) on read-back; `CommandList`: multi-line rows (up to
3), warn if `argv[0]` not on PATH (`find_on_path` exists in `settings`); `MapOfLists`: use `Chip`s +
dashed add slot (remove the bare `✕` div and `Plus` button); named columns from catalog `columns`.
*Accept:* each of the 6 container shapes round-trips `test.config.yml` including types, and
duplicates in `media_fields` block Save with a row-numbered message.

2H. **SettingPicker polish** (S–M) — always-visible counter ("7 of 142"), footer with hints and
"142 settings · N in this config", Tab behaviour (close + focus new row control: keep a
`FocusHandle` per field id), description only on highlighted row, verify rank order (key prefix →
key substring → description).

2I. **YamlPreview v2** (M–L, spike first) — see if gpui-component `EditorState` exposes
diagnostics/decoration/highlight APIs. If yes: error-line tint + annotation + click-to-focus (needs
a `yaml_line → setting_key` map built alongside `to_yaml`), gutter only past 40 lines, three-color
scheme, "Copy YAML" button, 120 ms debounce and caching (do not call `to_yaml()` in `render`; store
`yaml_text` on edit). If not, keep the plain editor and add the marker list as clickable chips that
focus rows.

2J. **Footer / ProblemSummary / chrome** (S) — click a counter ⇒ scroll to and focus the first
offending row (needs per-row `ScrollHandle` anchors + `FocusHandle`s); "Draft saved HH:MM" using local
time (add `time` or `chrono` — fine for display here; the settings crate avoided it, so keep age
strings if you prefer); Save disabled tooltip states the first error.

2K. **Chain gaps** (M–L) — Phase-2K tasks map 1:1 to §3.3 items 1–8:
(1) bind "● OPEN" to open windows; (2) Relink for broken nodes (file picker → replace path in the
owning draft); (3) show link error under the attempted parent row (store `link_error: Option<(PathBuf, String)>`
in `ConfigBuilder`); (4) per-level "Link an existing config" / "Create a new one" + "+ Add a config
under this one"; (5) clickable run-order chips; (6) allow duplicates, show `×N`; (7) drag to reorder
the flat list (reuse the `RowEditor` reorder primitive from 2G); (8) cascade windows by depth
(`Bounds` offset = depth × 24 px) and centre past level 3; draw a primary-colored frame on child
windows.

2L. **Validation expansion** (M) — implement §3.4 items; add `ValidationContext`; rules table stays a
plain slice. New rules to consider from mockups: `parent_id` ↔ parent `id_field` info message;
`adaptive_pause` × row count; `timestamp_rollback` without `rollback_dir` (exists);
`allow_missing_files` (exists). Every rule needs a unit test.

2M. **Window sizing/persistence** (S) — builder default 800×760, min 640×520 (docked 480);
persist bounds per kind under `builder_window_bounds`; stop deriving size from main window.

### Phase 3 — Settings window (M–L)

| ID | Task | Notes / acceptance |
|---|---|---|
| 3.1 | Complete **Config builder** page: `builder_open_docked`, `builder_check_paths` (gates path checks), `builder_warn_clashes` (gates `RULES`), `builder_vocab_server` (server select, default "the server picked for the run"). | Each setting changes behaviour observably; unit tests for the two gates. |
| 3.2 | Split **Appearance** into its own page (after Config builder), listing the themes from Phase 7. | |
| 3.3 | Sidebar width 176; sidebar footer (6 px dot + "Profile: <name>") — dot success/danger/muted from the active profile's server `last_check`. Needs Phase 4 for the label; ship the dot with "Default" until then. | |
| 3.4 | Unify servers/configs (and later profiles) on one `ui::EntityCard` with collapsed/expanded states, meta chips, hover-revealed ghost actions, one-expanded-at-a-time. | Audit `custom_fields.rs` first; may already be close. |
| 3.5 | Verify/limit Callouts (max one per group); status dots semantic only; `Enter` in search jumps to first match with 800 ms label flash if the library allows (else record as library-limited). | |
| 3.6 | Window defaults: settings 846×470 min 720×420. | |

### Phase 4 — Profiles (Stage 5, mockup `3c`) (XL) — **needs Q1**

1. **Model** (`crates/settings/src/lib.rs`): `Profile { id, name, servers, default_server, input_source +
   its values, preprocess_scripts_dir, config_library_dir, task_configs, builder defaults,
   auto_accept, confirmation rules }`; `AppSettings { machine: MachineSettings, profiles: Vec<Profile>,
   active_profile: ProfileId }`. Machine-only: workbench path, python/uv, theme, window sizes,
   credential *files*.
2. **Migration**: `SETTINGS_SCHEMA_VERSION` 2 → 3; wrap today's flat settings into a profile named
   `Default` (serde-default every new field; add a migration unit test with a v2 JSON fixture).
   Keep `settings.get(key)` call sites compiling by adding profile-aware accessors
   (`AppSettings::profile(cx)`), then migrate call sites crate by crate.
3. **Profiles page**: cards with Rename · Duplicate · Export… · Switch to · ✕ (ConfirmDialog) ·
   New profile · Import…, meta chips (`2 servers`, `6 configs`, `source: Google Sheet`, `3 scripts`),
   active tag; In-the-profile / On-this-machine two-column explainer; one Callout about export.
4. **Export/Import** `.profile.yml`: paths and settings, **never passwords/credential contents**;
   import asks for the credentials file path and validates referenced folders; tests for secret
   exclusion.
5. **ProfileBar** above step 1: eyebrow `Profile`, dropdown (26 px), `Manage profiles` ghost button;
   hidden while exactly one profile exists; switching reloads servers, source, library, builder
   defaults atomically and never cancels a run in flight (the run keeps its profile id).
6. Sidebar footer shows the active profile (Phase 3.3).
*Accept:* two profiles with different servers; switch while a run is active ⇒ run keeps old profile;
export file contains no credential contents; v2 settings file migrates.

### Phase 5 — Docking (Stage 4, mockup `2b`) (XL) — **needs Q4/Q5, research first**

1. **Spike (S):** in a scratch binary, register a `Panel` in `DockArea`, drag it out to a window and
   back; read `gpui_component::dock` for tab-drag-out / re-dock / drop hit-test hooks (grep `drop`,
   `TabPanel`, `DockPlacement`, `Tiles`). Decide: library snapping vs custom `WindowLock`-adjacent
   overlay.
2. **Split view from window**: `ConfigBuilderView` (no `TitleBar`, no window-bound reads; footer
   becomes the panel footer) and a thin `ConfigBuilderWindow` host that adds title bar and
   `Root`. Keep `ConfigBuilderWindows` semantics (one live draft per path; re-key on first save).
3. **Docked state**: right pane default 480 px, min 420, max 60%, divider handle 6 px (`table_head`
   surface, `col-resize` cursor, hover primary @ 30%); main window controls compress to a rail when
   docked; "Pop out" button in the panel tab strip; `Ctrl+\` toggles docked/floated
   (add to `actions.rs`, `app_menus.rs`, `docs/SHORTCUTS.md`).
4. **Floated state**: real window with its own chrome; parent shows a dashed placeholder with
   "Dock" ghost button.
5. **Snapping**: dashed target + label (`right half · 52%`, "Release to dock") when the floating
   window is dragged within 24 px of the parent's right edge; fade on leave. If GPUI exposes no
   drag stream, implement drop-driven docking (on window-moved-end) and note it.
6. **Persistence**: `builder_dock_side`, `builder_dock_width`, machine-scoped (not profile).
7. Child (chain) windows stay separate windows even when the root is docked.
*Accept:* dock → edit → float → move to second monitor → dock again keeps the same draft; closing
the main window with a dirty docked draft prompts.

### Phase 6 — Main window follow-ups (S–M)

6.1 Source dropdown mono `plugin` tag. 6.2 Store `last_processed_at: SystemTime`; render
"last run 11:52" (local time). 6.3 `↳ N` chain badge beside the config name when the row is wide.
6.4 Status bar "Reload" item (re-read config + servers) if kept. 6.5 Block closing a main window
with unsaved builder drafts or while `WindowLock` is held (builder windows do not use
`on_window_should_close`). 6.6 Show a toast only for run outcomes (verify).

### Phase 7 — Themes and motion (M)

7.1 Split `theme.json` into `assets/themes/qrate-dark.json` and `qrate-light.json`; add the four
extra palettes (gruvbox/everforest × light/dark) generated from one template; register through
`ThemeRegistry`; persist choice in `AppSettings` (machine scope). 7.2 Add the Design Language §02
derived values and elevations (set `"shadow": true` selectively or apply shadows per component).
7.3 Motion: 120/160/200 ms ease-out where gpui supports `with_animation`; keep optional — do not
let it block. 7.4 Verify Archivo/Plex resolve with no system fonts (run in a container without them;
screenshot).

### Phase 8 — Catalog data (M, parallel)

8.1 Descriptions for the 138 undocumented settings: scrape upstream Workbench docs (script under
`scripts/`, output merged into `catalog_overrides.json`), then human-review. 8.2 Add `unit`,
`min`, `max` for integers, `columns` for maps, and `default_tokens`/legal tokens for templates to the
schema in `gen-config-catalog.py` and `catalog.rs`. 8.3 Confirm `task` choices (10 vs 12) against
upstream and update spec copy/tests. 8.4 Add `browse: new_file` where applicable. 8.5 Replace the
hard-coded `"142"` wording anywhere it appears (counter is computed).

### Phase 9 — Quality gates (M, continuous)

9.1 **Logic tests**: validation rules (each), `parse_scalar`/`rows_of`/`read_widgets` round-trips for all 16
shapes using `test.config.yml`, slug function, chain edit routing, migration v2→v3, profile export
secrecy, path rule (P0-4).
9.2 **GPUI view tests** if feasible with `gpui::TestAppContext` (settings/builder construct without
panicking, templates visible on a new draft, discard resets). Spike once; if it works, add a
smoke test per window.
9.3 **Visual verification checklist** (record results in the PR): both themes × minimum window width
(520 px / spec 640 px for builder) × typical size; no clipped labels, equal adjacent control
heights, no unreachable actions; Archivo resolution; picker not clipped in a short window;
twelve-value enum rendering (`task`).
9.4 CI: add a Linux job that builds and runs unit tests; keep `-D warnings`.
9.5 Docs: update every stage file's "Status" and "Notes discovered" in the same commits; replace
`docs/plans/README.md` table with this plan's statuses.

---

## 6. Dependency graph (what blocks what)

```
Phase 0 ──► Phase 1 ──┬──► Phase 2 (2A–2M) ──┐
                      │                      ├──► Phase 9 (continuous)
                      ├──► Phase 3 ──► Phase 4 ──► Phase 5 (persistence, profile bar) ──► Phase 6
                      └──► Phase 8 (feeds 2D, 2F, 2G)    Phase 7 (feeds 3.2)
```
Hard dependencies: 1.4 before any chain work (2K); 2G (reorder primitive) before 2K(7); Phase 3.1
before 2F's live vocabularies; Phase 4 before ProfileBar/sidebar footer; Phase 5 spike before
committing to the dock UX; Q1 before Phase 4; Q2 before 1.2; Q3/Q8 before 1.3.

---

## 7. Quick wins (≤ 1 hour each; good first commits)

1. P0-6 template condition. 2. P0-10 self-link validator. 3. Remove the phantom host warn (P0-11).
4. Slug function (P0-8). 5. Show `default:` hint for all shapes (2A.1). 6. Nullable `none` row order
(2E). 7. Counter always visible in the picker (2H). 8. "Draft saved HH:MM". 9. Builder bounds
persistence (2M). 10. `ConfigBuilder` min size 640×520.

## 8. Risk register

| Risk | Mitigation |
|---|---|
| gpui/gpui-component are unpinned git heads — an upstream API change can break the build between sessions. | Commit `Cargo.lock` changes deliberately; do not run `cargo update` casually; note the rev in PR descriptions. |
| Token/size deviations (32 vs 30 px) will be flagged by a literal reading of the spec. | Documented in §2.1; do not "fix". |
| Editor highlighting/diagnostics may not be exposed by the library (YAML panel, template tokens). | Spikes in 2D/2I with explicit fallbacks. |
| Profile migration corrupts existing settings. | Version bump, `serde(default)`, backup the old file as `settings.v2.json` before first write, fixture test. |
| Writing YAML textually (1.3) is fiddly. | Constrain to three scalar top-level keys; fuzz with the sample configs. |
| Snapping may be impossible without a window-drag stream. | Fallback to drop-driven docking; decided in the Phase 5 spike. |

## 9. Review greps and commands

```bash
# Spec §10 invariants (expect only theme.rs and its parse test / a 1px divider and the 34px bar)
grep -rnE '#[0-9a-fA-F]{6}' crates/*/src --include=*.rs
grep -rnE '\.h\(px\(' crates/*/src --include=*.rs
# Bare clickable text / div handlers that should be ghost buttons
grep -rn 'on_click' crates/config-builder/src | grep -v 'Button\|button'
# Icon buttons without tooltips
grep -rn 'icon(IconName' crates/*/src --include=*.rs | grep -v tooltip
# Gate the catalog
python scripts/gen-config-catalog.py --workbench <path to islandora_workbench>
# CI gates
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

## 10. Addendum — build/test baseline in the audit container

See the final lines appended below by the audit session (whether `cargo test --workspace --no-run`
completed in the cloud container, and with what errors).
