# Stage 2 — Main window, generalized

**Mockup:** `2c`
**Status:** not started
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

- `crates/app/src/workspace/mod.rs` — the `Metadata` and `Server` blocks in `render`
  (roughly lines 657-900 as of this writing), plus the `collection_node_id` field and its
  subscription in `Workspace::new`.
- `crates/workbench-integration/src/lib.rs` — `process_google_sheet_metadata` currently takes
  `node_id` and hard-codes the SACDA path. It becomes "run the selected preprocess script
  against the selected source".
- `crates/app/src/workspace/gdrive_log.rs` — the log messages are Sheets-specific; generalise
  their wording to the chosen source.
- Settings needs a `preprocess_scripts_dir` value (Stage 3 gives it a page; a bare key works
  before then).

## Open questions

From the canvas, unresolved:

- **Do preprocess scripts need to declare anything** — a name, expected columns — or is "a bare
  `.py` file that writes `metadata.csv`" the whole contract? Decide before building; it changes
  whether the dropdown needs to read script metadata or just list a folder.
- Does the input-source picker need to be genuinely pluggable now, or is an enum of the two
  known sources (Sheet, CSV file) enough until a third appears?
