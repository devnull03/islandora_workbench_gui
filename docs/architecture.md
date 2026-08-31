# Workspace architecture

The workspace uses a small number of broad ownership boundaries. Application-specific UI stays in
the executable crate. Only genuinely reusable controls and the self-contained config-builder
feature live in separate UI crates.

## Crate map

| Crate | Owns |
|---|---|
| `app` | Startup, menus, workspace, settings UI, status bar, and desktop/platform helpers |
| `config-builder` | Builder state and UI, editors, search, YAML preview, chaining, dialogs, and window lifecycle |
| `ui` | Small reusable GPUI controls, including path pickers and dropdown item models |
| `settings` | Persisted settings model and settings-window primitives built from `ui` controls |
| `workbench-integration` | Workbench commands, processing, config catalog/drafts, and validation |
| `window-wrapper` | Shared title bar, status bar, and window locking/chrome |

## Dependency direction

```text
workbench-integration        window-wrapper
            |                         /
            +---------- config-builder
                         /       \
                    settings ----- ui
                         \       /
                          +-- app
```

`config-builder` is a complete feature boundary. Callers use its public window/action API and
do not manage builder state themselves. `app` owns the product-specific screens and operating-
system integration because those pieces are not shared independently.
