# Stage 3 — Settings, reworked

**Mockups:** `3a` (what exists today, recreated from source) → `3b` (the rework)
**Status:** built — see the notes section
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

## Decisions (31 August 2026)

Both open questions were put to the user and answered:

- **The search filters rows in place** — and it turned out gpui-component's `Settings` already
  does exactly that, so the app writes none of it. See note 1.
- **Test is two stages.** `GET {server_url}` first; only if that answers,
  `GET {server_url}/islandora_workbench_integration/version` with the credentials file. They are
  reported separately because they fail separately, which is exactly what the mockup's
  "Credentials were not tested" implies.

## Notes discovered during implementation (31 August 2026)

1. **The search is gpui-component's, not ours.** `Settings` renders its own field and filters
   through `SettingItem::is_match`, which reads each item's title and description; a custom
   `SettingItem::Element` (the server and config lists) shows only when the query is empty,
   which is right for a list that is not a single setting. A hand-rolled filter was written
   first, against a `build_pages(query)` signature, and then removed — `build_pages` is back to
   `fn() -> Vec<SettingPage>`. Check the library before building a filter.
2. **No migration code was needed for schema v2.** `PersistServerConfig`'s new fields are
   `#[serde(default)]`, and "never tested, no confirmation required" is the right reading of a
   v1 blob's silence. The version constant still moves to 2 so the change is legible.
3. **`window.use_keyed_state` is what makes edit-in-place possible.** A settings item's render
   closure takes `&mut Window, &mut App` and runs fresh every frame, so it can create entities
   but not hold them. Keyed state (gpui-component's own path picker uses it) gives each row
   durable widgets with no global edit state to keep in sync.
4. **`CheckResult::at` is a unix timestamp rendered as an age**, not the mockup's `12:03`.
   A wall-clock time needs a timezone database to be honest; "checked 4 min ago" needs nothing
   and answers the same question.
5. **`upsert_server_config` preserves `last_check` when the URL and credentials file are
   unchanged.** Renaming a server does not invalidate a test result, and re-testing after every
   label edit would train people to ignore the line.
6. **The task field is a dropdown, not a text input.** `task` is the one setting Workbench
   demands and only ten values are legal.
7. **`needs_confirmation` is wired into the run, not just stored.** `run_ingest` clears
   auto-accept when the selected server carries the flag and says so in the log — a flag that
   only decorated a settings row would be worse than no flag.
8. **`builder_check_paths` was cut.** Marking which problems came from path checking, purely so
   a switch could suppress them, is more machinery than the switch is worth. Path checking stays
   always-on. `config_library_dir` and `builder_show_yaml` are wired: the latter also decides the
   builder window's opening width, or the first toggle corrects it with a visible jump.
9. **The Settings shell must not add an outer scrollbar around `Settings`.** The component already
   scrolls its page. The extra `overflow_y_scrollbar` gave its resizable sidebar an auto-height
   parent, so the search input could latch a collapsed width on first paint and only recover during
   a window resize. A definite-height `min_h_0` parent is the required layout.
10. **Settings has a window-wide shortcut.** **Ctrl+,** on Windows/Linux and **Command+,** on macOS
    dispatches the same `OpenSettings` action as the menu item, so the menu prints the shortcut too.

**Deliberately not built:** the Profiles page and the active-profile footer (Stage 5 owns both,
and there is nothing to switch between until profiles exist). `AppSettings` is now split the way
Stage 5 needs — *Workbench & Python* is machine facts, everything else is institutional.
