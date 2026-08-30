# Stage 3 — Settings, reworked

**Mockups:** `3a` (what exists today, recreated from source) → `3b` (the rework)
**Status:** not started
**Depends on:** Stage 1 (the *Config builder* page holds the defaults Stage 1 had to guess)
**Blocks:** Stage 5 (profiles bundle these settings)

## Problem

`crates/app/src/app_settings/mod.rs` has three pages — Paths, Servers, Task Configs — and the
page / group / row anatomy is good. What sits inside strains:

- a saved config is only a label, a task name and a path to a YAML someone wrote elsewhere;
- adding an entry is a *separate group* from the list of entries, so each page reads as two
  unrelated forms;
- entries can be removed but not edited or renamed — a typo in a URL means delete and retype;
- everything is global to the machine: one server list, one config list, so a dev credential and
  a production host can be paired by accident;
- nothing here can be handed to another institution; setup lives in a document instead of a file.

## What changes

**Keep** the page / group / row anatomy and the window shell. Change what sits inside.

1. **A list and its add form are one thing.** Rows expand in place to edit; the last row is
   always `+ Add`. This removes the `new_server_*` / `new_task_*` staging keys in
   `AppSettings::values` and the separate "Add New …" groups.
2. **Each server carries its own credentials file and its last check result** (`Last checked
   12:03 — HTTP 530, host unreachable. Credentials were not tested.`), so a bad pairing is
   visible before a run rather than during one. Per-server actions: Edit · Test · Duplicate ·
   remove. A server can be marked *needs confirmation before a run* — destructive tasks on it
   always prompt, whatever the auto-accept setting says.
3. **Pages are named after what you are doing**, with a search field for when you do not know
   which page a setting is on:
   General · Workbench & Python · Servers · Config library · Input & preprocess ·
   Config builder · Profiles.
   Workbench path, Python and UV move to **Workbench & Python** — they are facts about the
   machine, so they sit apart from anything institutional.
4. **The builder gets its own page.** Every default Stage 1 had to hard-code becomes a setting:
   open the builder docked · show the YAML panel · check paths as you type · warn about settings
   that clash · read vocabularies from (server) · save new configs to (folder) · preprocess
   scripts folder.

The footer shows the **active profile** at all times — the one piece of Stage 5 that has to be
visible everywhere, so nobody edits the wrong institution's servers.

## Files

- `crates/app/src/app_settings/mod.rs` — `build_pages()`, wholesale.
- `crates/app/src/app_settings/custom_fields.rs` — `saved_servers_field` / `saved_configs_field`
  become editable-in-place lists; `add_server_button` / `add_config_button` fold into the list's
  last row.
- `crates/settings/src/lib.rs` — `ServerConfig` gains a last-check result and a
  needs-confirmation flag; the `new_*` staging keys go away. `AppSettings::add_*_config` become
  add/update pairs.
- `crates/settings/src/db.rs` + `SETTINGS_SCHEMA_VERSION` (`crates/settings/src/lib.rs:12`) —
  a migration, since the persisted JSON shape changes.
- `crates/settings/src/lib.rs::SettingsWindow` — add the search field to the shell.

## Open questions

- The search field: filter the visible rows in place, or jump to the matching page? The mockup
  shows the field but not the result state.
- What does **Test** actually do — HEAD on the server URL, or a real authenticated call with the
  credentials file? The mockup's failure text ("HTTP 530, host unreachable. Credentials were not
  tested.") implies two stages, reachability then auth.
