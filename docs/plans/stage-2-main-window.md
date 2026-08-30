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

Eight things the plan did not predict:

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
   only when the selection changes. "Edited" is relative (today / yesterday / N days ago), which
   also avoids pulling in a date-formatting crate.
8. **The mockup's `↳ 2` chain badge is folded into the summary line** as
   `runs N secondary configs` rather than drawn as a separate glyph. Add the badge when the row
   gets crowded enough to need the shorthand.

`Operation::GdriveBusy` / `WorkflowStage::GdriveProcessed` / `gdrive_log.rs` were renamed to
`Preprocessing` / `SourceProcessed` / `preprocess_log.rs` — nothing about them was ever
Sheets-specific except the name.

**Deliberately not built:** the profile switcher above step 1 (Stage 5 owns it, and it stays
hidden until a second profile exists anyway).
