repo: devnull03/islandora_workbench_gui
branch: main

Reference (upstream tool, read-only): mjordan/islandora_workbench @ main — `WorkbenchConfig.py`
Reference (UI framework, read-only): longbridge/gpui-component @ main — `crates/ui/src`

## Last sync
date: 2026-08-31T07:18:06Z

### Updated in this project
- Read `crates/ui/src/theme/theme_color.rs`, `setting/fields/mod.rs`, and `lib.rs` from longbridge/gpui-component and wrote `Design Language.dc.html`: a GPUI-targeted spec with theme JSON, scales, geometry, states, and a reuse/wrap/build verdict per component.

## Sync history

### 2026-08-30T17:42:41Z
- Read `crates/app/src/app_settings/mod.rs` + `custom_fields.rs` and recreated the current Settings window (Paths / Servers / Task Configs), then designed a reworked version plus a Profiles page.
- Generalized the main window: input source, always-present Sheet URL + Ingest dir, one preprocess-script dropdown, collection nid removed.
- Added recursive `secondary_tasks` chaining and docked / floated / snapping builder-window states.
- Regrouped the canvas by subject: settings, main window, window placement, chaining, builder, value shapes.

## Screen map
| Screen | Built from |
|---|---|
| 1a Config builder — search open | `crates/app/src/components/*`, `crates/window-wrapper/src/title_bar.rs`, upstream `WorkbenchConfig.py` |
| 1b Config builder — populated + YAML panel | `crates/workbench-integration/test.config.yml`, upstream `WorkbenchConfig.py` |
| 1c YAML value-shape gallery | upstream `WorkbenchConfig.py` `get_default_config()` |
| 1d, 2a secondary_tasks chaining | `crates/workbench-integration/src/config_builder.rs` |
| 2b builder window placement | `crates/app/src/main.rs`, `crates/window-wrapper/src/*` |
| 2c main window, generalized | `crates/app/src/workspace/mod.rs` (screenshot), `app_settings/mod.rs` |
| 3a Settings today | `crates/app/src/app_settings/mod.rs`, `app_settings/custom_fields.rs` |
| 3b Settings reworked · 3c Profiles | same, plus `crates/settings/src/lib.rs` model |
| Design Language (all sections) | gpui-component `crates/ui/src/{theme,setting,list,searchable_list,dock,tree}`, `_ds` Qrate tokens |
