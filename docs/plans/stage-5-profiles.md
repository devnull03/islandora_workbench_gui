# Stage 5 — Profiles

**Mockup:** `3c`
**Status:** not started
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
