# Stage 5 — Profiles

**Mockup:** `3c`
**Status:** built (October 2026) — see *Built* below
**Depends on:** Stage 2 (input source) and Stage 3 (the settings surface being bundled)
**Blocks:** nothing

## Problem

Every setting is global to the machine: one server list, one config list, one input step. That
is exactly right for one person ingesting for one institution, and it breaks in three ordinary
situations:

1. **Dev and production side by side.** Same config library, different hosts and credentials.
   Nothing currently stops a config written for dev being run against production.
2. **Another institution.** They need their own servers, sources and configs, and none of ours.
   Without profiles the app has to be re-set-up by hand and ours has to be deleted.
3. **A second machine.** Onboarding today means following a setup document. With profiles it
   means opening a file.

## Why a bundle rather than more settings

The dangerous mistakes are **mismatches** — a dev credential with a production host, our
transform script against someone else's sheet. Grouping the institutional settings into one
named thing that switches **atomically** removes that class of mistake. A longer settings list
does not.

## The split

**In the profile:** servers and credential paths · input source and its values · preprocess
scripts folder · config library and builder defaults · auto-accept and confirmation rules.

**On this machine:** workbench install path · Python and UV paths · theme and window sizes ·
the credential *files themselves*.

## Paying the cost

Profiles are one more concept for someone who only ever needs one. So:

- the app ships with a single profile called **Default**;
- **Profiles** is the last item in the settings sidebar;
- the switcher in the main window stays **hidden until a second profile exists**.

Nobody meets the idea until they need it.

## Export

**Export** writes a `.profile.yml` with paths and settings but **never passwords**. The person
importing it supplies their own credentials file. This replaces the setup document with a file.

## Files

- `crates/settings/src/lib.rs` — `AppSettings` splits into machine-scoped and profile-scoped
  halves; a `Profile` struct; an active-profile pointer.
- `crates/settings/src/db.rs` + `SETTINGS_SCHEMA_VERSION` (`crates/settings/src/lib.rs:12`) —
  a migration that wraps today's flat settings into a `Default` profile.
- `crates/app/src/app_settings/mod.rs` — the Profiles page (rename · duplicate · export ·
  switch · new · import), with per-profile summary counts (`2 servers · 6 configs ·
  source: Google Sheet · 3 scripts`).
- `crates/app/src/workspace/mod.rs` — the profile switcher above step 1, hidden at count 1.
- `crates/window-wrapper/src/status_bar.rs` — the active profile in the footer, always visible.

## Open questions

**Answer this before building:** does the **config library** belong to the profile or to the
machine? The mockup draws it as institutional (in the profile). But if the user keeps one folder
of YAML shared across profiles, it should move out. The canvas flags this as unresolved.

## Built (October 2026)

Q1 answered with the plan's recommended default: a profile holds a *pointer* to a config
library folder (`config_library_dir`) and its config list; the YAML files stay machine files.

- **Model** (`settings::profiles`): the active profile *is* `AppSettings`'s ordinary fields, so
  no reader changed; other profiles are `Profile` snapshots. `PROFILE_KEYS` lists what is
  institutional (input source and values, ingest folder, preprocessor and scripts folder,
  library folder, auto-accept, builder switches) plus servers, configs and their defaults;
  everything else (Workbench/Python/uv, theme, window sizes, update checks) stays with the
  machine. Switching snapshots the outgoing profile and loads the incoming one; the list keeps
  its order (`active_profile_index`).
- **Migration**: settings schema 2 → 3. A v2 file has no profiles and loads as one profile,
  "Default", holding everything it had (fixture test in `db.rs`).
- **Settings → Profiles**: a card per profile (name, `active` tag, "2 servers · 6 configs ·
  source: CSV file") with Switch to · Rename · Duplicate · Export… · ✕ (confirm dialog; the
  active profile cannot be deleted), New profile, Import…, the In the profile / On this machine
  explainer and one note about export. The Settings window now renders a dialog layer, which
  it lacked.
- **Export/Import** `.profile.yml`: servers carry their credentials *file path*, never its
  contents (tested); machine keys are never exported, and unknown keys are dropped on import.
- **Profile bar** above step 1: hidden while there is one profile; a dropdown and Manage
  profiles. Switching re-seeds every step field and list and drops the old profile's processed
  CSV and last run. It is refused while a run is in progress (a run never changes profile under
  it) rather than letting the run keep a stale profile id.
- The Settings footer names the active profile.
- Also: the config-library add row in Settings offered 10 hard-coded tasks; it now uses the
  catalogue's 18.

**M4 audit fixes (October 2026).**
- A switch replaces the config, server and preprocess-script lists outright, even with empty
  ones — a new, empty profile used to keep showing the previous profile's servers, and picking
  one bypassed its "always confirm". The script list is rebuilt from the new profile's scripts
  folder before its script is selected, so the selection no longer falls back to the built-in.
  Lists are compared by label *and* file/URL.
- A builder files its config into the library of the profile it was opened under, even after a
  switch (`AppSettings::upsert_library_entry`).
- Settings rows open for editing, and their fields, are keyed by profile generation: a switch
  closes them instead of letting a save write one profile's values into another's.
- A server Test result is matched back by URL and credentials file and dropped if the profile
  changed meanwhile. Results survive switching away and back (a session cache, never exported).
- Switching is refused during preprocessing too (it now holds the window lock), and the main
  window redraws as soon as a switch is made from Settings.
- Settings written by a newer schema are copied aside (`app_settings_v1_from_vN`) before this
  build loads them, since its next save would drop what it does not know.
- Profiles is now the last Settings page (`3c`), and Manage profiles opens Settings on it
  (`OpenProfiles`; an already-open Settings window is brought forward on its current page).
  A switch also clears step 2's `↳ N` badge with the rest of the config summary.
- Still open (Low): the profile bar is inside the scrolling column rather than its own slot,
  card facts are one line rather than chips, import always adds ("X 2") rather than offering
  to replace.
