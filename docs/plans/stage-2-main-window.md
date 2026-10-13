# Stage 2 — Main window, generalized

**Mockup:** `2c`
**Status:** built — see the notes section
**Depends on:** Stage 1 (the builder must exist before Edit/New can open it)
**Blocks:** nothing

## Problem

Step 1 of the main window is the only institution-specific part of the app. It hard-codes a
Google Sheet URL, a collection node id, and the SACDA transform. Another institution cannot use
the window unchanged, and the collection node id is a SACDA concept leaking into app chrome.

## What changes

**Step 1 becomes a pluggable input source.** Three fields, no site knowledge:

- an **input source** picker (`Google Sheet → CSV`, plain CSV file, …) — a plugin-shaped choice;
- **Sheet URL** and **Ingest dir**, always shown;
- a single **preprocess script** dropdown, populated from a scripts folder set in Settings
  (Stage 3's *Input & preprocess* page), plus `None`. The script turns the source into
  `metadata.csv`. A site with no transform picks `None` and just presses **Process**.

**The collection node id is removed from the window** — it belongs inside the script.

**Three numbered steps**, in the order a run actually happens: *1 Input source* (marked
optional — a site that already has its CSV skips it) · *2 Config* · *3 Server*. The config row
grows a chain count (`↳ 2`), a summary line (`14 settings · runs 2 secondary configs · last
edited yesterday`), and **Edit** / **New** buttons that open the Stage 1 builder — so the
builder is reachable from the place the config is chosen.

A **Profile** switcher sits above step 1, hidden until a second profile exists (Stage 5).

## Files

- `crates/app/src/workspace/steps.rs` — the three step renderers. Split out of `mod.rs` on
  31 August 2026, along with `run.rs` (the check / run / provision pipeline); `mod.rs` now holds
  only state, wiring and the top-level scroll layout. The numbered-step chrome is
  `ui::StepSection` and the label-over-control pair is `ui::LabeledField` — add steps with those
  rather than open-coding the header row again.
- `crates/app/src/workspace/mod.rs` — `Workspace::new` (field wiring and subscriptions).
  `collection_node_id` is already gone from the window.
- `crates/workbench-integration/src/lib.rs` — `process_google_sheet_metadata` currently takes
  `node_id` and hard-codes the SACDA path. It becomes "run the selected preprocess script
  against the selected source".
- `crates/app/src/workspace/gdrive_log.rs` — the log messages are Sheets-specific; generalise
  their wording to the chosen source.
- Settings needs a `preprocess_scripts_dir` value (Stage 3 gives it a page; a bare key works
  before then).

## Canvas questions, resolved below

From the canvas, unresolved:

- **Do preprocess scripts need to declare anything** — a name, expected columns — or is "a bare
  `.py` file that writes `metadata.csv`" the whole contract? Decide before building; it changes
  whether the dropdown needs to read script metadata or just list a folder.
## Decisions (31 August 2026)

- The processing dropdown currently has one choice: **Workbench preprocessor**, the built-in
  Rust importer. It is a dropdown now so future processors do not require another main-window
  redesign.
- The processing contract is stable: a processor receives the complete source CSV and may receive
  the selected Workbench config file. It must return the path of the new metadata CSV it wrote.
  The config is optional because processing can happen before the user selects one.
- The first source adapter is **Google Sheet → CSV**. It feeds the built-in processor and returns
  the processed CSV path. The processor does not inspect the optional config yet.
- The UI keeps the input-source dropdown even with one choice. A later source adapter can add an
  option without changing the three-step window.

The collection-node control remains removed: site-specific logic belongs with the processor, not
in the shared window.

## Notes discovered during implementation (31 August 2026)

**The open question is answered: a script declares nothing.** Being a `.py` in the folder named
by `preprocess_scripts_dir` is the whole registration. The dropdown lists the folder and never
opens a file. A script is invoked

```text
python <script> --input <source.csv> --output-dir <dir> [--config <config.yml>]
```

and must write a CSV. If its last line of stdout is a path that exists that is the result,
otherwise `<dir>/metadata.csv` is — forgiving of both readings of "report the path it wrote".
The interpreter is Workbench's own (`WbInfo::python_command`, `uv run python` when UV is on),
with the workbench directory as the working directory, so a script has the dependencies
Workbench has without installing anything of its own.

Fifteen things the plan did not predict:

1. **Source and processor had to be split into two enums, not one.** `process_google_sheet_source`
   fused acquisition and transformation. `preprocess.rs` now has `InputSource` (GoogleSheet, CsvFile)
   and `Processor` (Builtin, Script, None) and dispatches the pair, so every combination works
   without a function per combination.
2. **`PreprocessResult::details` had to become `Option`.** Row and validation counts come from
   the built-in importer's `ProcessResult`; an external script is a black box. `None` is what
   "we genuinely do not know" looks like, and the log says `Processor finished.` instead of
   inventing zeroes.
3. **`PreprocessResult` gained `output`.** A script's stdout is the only report it has, so it is
   echoed into the log as `[SCRIPT] …` lines.
4. **Two input fields, not one.** Switching source must not lose the URL you already pasted, so
   the sheet URL and the CSV path are separate `InputState`s persisted under `gdrive_link` and
   `source_csv`. `Workspace::source_field` picks the one the current source uses.
5. **`WbInfo::python_command` was extracted** from `build_workbench_command`, and
   `run::workbench_info` from `run_ingest`, so a preprocess script and an ingest run resolve the
   Python environment the same way and fail the same way when it is missing.
6. **The processor list is rebuilt from a directory read, not from a settings row**, so the only
   way to notice a new script is to look. `sync_select_items` compares the values it would build
   against the ones the select holds, which keeps that to one `read_dir` per render.
7. **The config summary is cached.** `14 settings · runs 2 secondary configs · edited today`
   needs a parse of the YAML plus a stat, which must not happen once per frame — it is rebuilt
   when the selection changes or a config file is written (`ConfigFilesChanged`). "Edited" is relative (today / yesterday / N days ago), which
   also avoids pulling in a date-formatting crate.
8. **The mockup's `↳ 2` chain badge** sits beside the config dropdown in the accent colour
   (phase 6.3; the summary line still spells it out as `runs N secondary configs`). It counts
   the configs run directly after this one, like the summary.
9. **The run log is a real bottom dock.** `gpui_component::dock::DockArea` owns its open state and
   height; both persist in `AppSettings`. The native status bar's Log control and **Ctrl+`** invoke
   the same toggle action.
10. **The title-bar menus are one `AppMenuBar`, not independent dropdown buttons.** This restores
    hover-to-switch, arrow-key navigation, menu action state, focus restoration, and automatic
    shortcut hints.
11. **The title and status bars now share dimensions and typography.** The status bar uses
    gpui-component's native `StatusBar` and the title bar's 34 px height, rather than duplicating
    the bar in a hand-built flex row.
12. **The status bar owns its item typography.** Items use neutral, borderless ghost controls and
    inherit the bar's `text_xs` foreground; hover is the only general accent treatment.
13. **The server indicator pings the saved default at startup,** not only after a later settings
    mutation.
14. **The update result explicitly refreshes open windows.** Its item is absent before a result,
    so notifying that hidden entity cannot make the status bar discover it.
15. **The run log is bounded.** It retains roughly the latest 5,000 lines and trims in batches,
    preventing both the backing vector and the rendered element count from growing forever.

`Operation::GdriveBusy` / `WorkflowStage::GdriveProcessed` / `gdrive_log.rs` were renamed to
`Preprocessing` / `SourceProcessed` / `preprocess_log.rs` — nothing about them was ever
Sheets-specific except the name.

**Deliberately not built:** the profile switcher above step 1 (Stage 5 owns it, and it stays
hidden until a second profile exists anyway).

**The processed path now reaches Workbench (October 2026, plan 1.2).** Until then nothing wrote
`input_csv`, and "Open processed" assumed `{workbench}/input_data/metadata.csv` whatever the
processor reported. The workspace keeps the reported path for the session, supplies it to the
root config as `input_csv` at Check/Run, and forgets it when the source or processor changes.
See stage 1's Phase 1 notes for the full rule.

## Workbench's command-line surface in step 3 (October 2026)

Mockup `2c` draws only Check and Run Ingest. Workbench's CLI (`./workbench --help`) does more,
and parity means all of it is reachable without a terminal. Step 3 now has:

- **Cancel** while anything runs. The first press is Workbench's documented ctrl-c: SIGINT to
  the run's process group (so `uv run` and the Python under it both get it); Workbench logs it
  and exits. A second press ("Stop now") kills the group. Windows has no console SIGINT for a
  windowless child, so there Cancel kills (`taskkill /T /F`). The child's SIGINT disposition is
  reset before exec: an app started as a background job inherits SIGINT ignored, and a Python
  started that way never raises KeyboardInterrupt — found while testing under Xvfb.
  When Workbench exits but something it started still holds the output pipes, the readers get
  two seconds to drain and then the rest of the process group is killed, so the run still
  finishes instead of hanging on "Running" (M2/M3 audit).
- **Options** (folds away): `--csv_start_row` / `--csv_stop_row`, `--skip_user_prompts`, and
  `--contactsheet` on Check. Per session; never written into the config.
- **More** menu: Get CSV template (`--get_csv_template`), Print effective config
  (`--print_config`), Quick delete a node / a media (`--quick_delete_node|media <url>`, behind
  a dialog), Open the Workbench folder.
- **Last run strip**: how it ended in words ("Check passed", "Run cancelled · 2 nodes
  created"), and the follow-ups it makes possible — Open contact sheet, Open CSV template,
  Open output CSV, **Roll back…** (runs the rollback config Workbench wrote, read from this
  run's part of its log, after a confirm), **Resume from node N…** (recovery mode,
  `--recovery_mode_starting_from_node_id`, offered when a `create` stopped after creating
  node N), Open log.

The argument mapping lives in `workbench_integration::invocation` and the output parsing in
`workbench_integration::outcome`, both unit-tested against the exact strings upstream prints.
Rollback, recovery, contact sheet, CSV template and quick delete need a live Islandora to
exercise end to end; in the cloud container they are covered at the argument and parsing level,
and Check, Print config and Cancel were exercised in the GUI against real Workbench.

**M1 audit fixes (October 2026).** A code-quality/performance audit of the run surface found,
and these were fixed: Workbench exits **0** on ctrl-c, so a cancelled Check counted as passed
(and unlocked Run Ingest) and a cancelled create was not offered Resume — both now look at the
cancel flag; resuming a resumed run now keeps the original start node (recovery mode only skips
map rows at or after the start node, so a later start would re-create the first attempt's rows);
the last run is tied to the server and config it ran against and cleared when either changes,
so Roll back cannot delete node IDs on a different server; the stream sends Done only after
both output readers drain (the contact-sheet / CSV-template lines are printed just before exit
and could be lost); created nodes are also counted from the log (Workbench prints them only
without `progress_bar`); `log_file_mode: w` reads the log from 0; an "always confirm" server
ignores Skip user prompts for destructive actions; quick delete refuses a URL that is not on
the selected server; the log scan after a run moved to a background thread and the results
strip reads no files while rendering; a run whose session is dropped (app quit) is killed;
Windows `open_file` uses Explorer rather than `cmd /C start` (cmd re-parses `&`/`^`).

**Phase 6 follow-ups (October 2026).**
- 6.1 The input-source dropdown shows a mono `plugin` tag before its caret (`2c`), through a
  `tag` on `DetailSelectItem` that only the closed trigger draws.
- 6.2 Step 1 says `last run 11:52` (local time of the last Process) instead of "this session".
- 6.3 Step 2 shows `↳ N` beside the config when it runs N configs after it. The summary line
  and the badge are re-read when the builder writes a config file (`ConfigFilesChanged`
  generation), not only when the selection changes — they used to go stale after a save.
- 6.5 Closing the main window — OS close, the Linux client-side ✕ (gpui-component's closes
  without asking, so the title bar now routes it through the same check) or Quit — is refused
  while a run holds the window lock and asks first when the docked builder has unsaved edits.
  Builder windows ask the same way before closing with unsaved edits.
- 6.6 A toast (gpui-component notification, bottom-right; the library's 5 s rather than the
  spec's 4 s) reports each run outcome — Check, Run, quick delete — and nothing else.
- Preprocessing now holds the window lock like a run, and a Process with no Workbench folder
  set says so instead of leaving the step stuck in its busy state.
- **M6 audit fixes.** Quit is deferred out of the menu's dispatch (it used to run while the main
  window was mid-update, find it unreachable and quit regardless of a run or unsaved edits). It
  now refuses during a run with a visible notice, and asks once about unsaved edits in *any*
  builder, docked or floating, in a window brought to the front. Closing during a run also says
  why it is refused. Preprocessing locks profile switches but no longer the close button (it
  cannot be cancelled, so a hung script would have made the window unclosable), and the title
  bar says "Preprocessing" rather than "Ingest running". Step 2's summary and badge redraw when a
  config file is written — by a builder or by a run's rewrite. A builder whose profile was
  renamed or deleted files its config in the active profile. A failed settings backup no longer
  makes settings load as defaults.

**Docs parity pass (October 2026).** A page-by-page comparison with the Workbench docs (61
pages, 12 not applicable to a front end) measured 88% and listed the gaps; these are closed:
- *Secrets.* Workbench asks for a password, or for the key to an encrypted credentials file,
  through `getpass`, which talks to a terminal the app cannot see — the run just hung. Before
  launching, the app now follows `WorkbenchConfig.get_password` (`secrets::secret_needed`): when
  Workbench would ask, a masked dialog asks instead and the answer goes to Workbench in
  `ISLANDORA_WORKBENCH_PASSWORD` / `ISLANDORA_WORKBENCH_ENCRYPTION_KEY` for that run only —
  never saved, never logged. The server Test reports an encrypted credentials file as "not
  tested" instead of a parse failure.
- *Task-aware input.* `create_from_files`, `get_data_from_view` and `get_media_report_from_view`
  read no CSV, so Check and Run no longer demand a processed sheet for them.
- *Google Sheets worksheets.* The sheet link's `gid` (`#gid=…` or `?gid=…`) is kept in the CSV
  export URL; before, every sheet was read from its first worksheet.
- *Media use terms.* `delete_media_by_node_media_use_tids` and
  `update_media_by_node_media_use_tids` take IDs, URIs or names; a typed ID is written as a YAML
  number, which is what Workbench compares with Drupal's `target_id` (a quoted "17" matched
  nothing).
- *Exports.* After an export task the last-run strip opens `export_csv_file_path`.
- *Spreadsheets.* The second input source is "CSV or spreadsheet": an `.xlsx`, `.xlsm`, `.xls` or
  `.ods` path shows a Worksheet field (empty reads the first) and is converted to CSV in Rust
  (`calamine`) before the processor runs, so the built-in preprocessor and scripts work on it
  like any CSV. This covers Workbench's own Excel input (`excel_worksheet`) from step 1.
  The worksheet is a profile setting (`source_worksheet`).
- *CSV ID → node ID map.* The More menu runs Workbench's own
  `scripts/manage_csv_to_node_id_map.py` in its Python, all seven of its operations: export the
  map, export the rows with a duplicate value in a chosen column (`--nonunique`: CSV ID or node
  ID), merge another map into this one, remove entries before
  or after a date, remove the selected config's entries — with its secondary configs', each
  named the way Workbench records it (the `secondary_tasks` entry as written) — and remove
  entries for nodes the selected server no longer has (the dialog warns that the map is
  not split by server). The database is found the way Workbench finds it
  (`csv_id_to_node_id_map_path`, else `csv_id_to_node_id_map_dir` — the system temp folder by
  default — plus `csv_id_to_node_id_map_filename`); output goes to the log, and the window is
  locked while it runs. Loading the config to find the map happens off the UI thread, as does
  a run's preparation (loading and rewriting the config, reading the credentials file). Text dialogs (these and quick delete) now focus their field on open.
- *Islandora 7 export* is out of scope: `get_islandora_7_content.py` moved out of Workbench into
  its own repository (`mjordan/i7_import`) in October 2025.
- *Smaller parity fixes.* An export's CSV is offered after the run: the `export_csv` task's
  printed path, or the View tasks' default `<input_dir>/<config>.csv_file_with_…` when
  `export_csv_file_path` is not set; `~` is expanded in it and in the map path. An `input_csv`
  that is a Google Sheet URL is not flagged as a missing file. A sheet that answers with a
  sign-in page instead of a CSV says to share it. The ingest folder no longer gates Check, Run
  and the More menu — nothing there uses it. Spreadsheet dates are written `yyyy-mm-dd`.
- *M9 audit fixes.* `get_data_from_view`'s default CSV name carries a timestamp, so its path is
  read from Workbench's `CSV file is available at …` line rather than guessed (only the media
  report's default name is predictable); printed export paths are resolved against the
  Workbench folder. Merging a CSV ID map asks first and refuses to merge a map into itself
  (the table has no key, so every row would be duplicated). Workbench's password check is
  matched more closely: `password:` with no value counts as set; a credentials file plus
  `ISLANDORA_WORKBENCH_PASSWORD` in the environment (which Workbench misreads) is refused
  with an explanation; `~\` and USERPROFILE are honoured on Windows; the secret dialog
  ignores an empty answer. Spreadsheet time-only cells become `14:30:00` and durations
  `26:05:00`.
