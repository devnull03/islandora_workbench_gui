# Bundled fonts

Loaded at startup by `ui::theme::init` via `include_bytes!`, so the app renders identically on a
machine that has neither family installed. Both are SIL Open Font License 1.1.

| File | Family | Upstream |
|---|---|---|
| `Archivo-{Regular,Medium,SemiBold}.ttf` | Archivo (400 / 500 / 600) | <https://github.com/Omnibus-Type/Archivo> |
| `IBMPlexMono-{Regular,Medium}.ttf` | IBM Plex Mono (400 / 500) | <https://github.com/IBM/plex> |

Static instances, not the variable builds: gpui's font matching across DirectWrite, CoreText, and
fontconfig handles named instances unevenly, and three weights is not worth that risk.

Archivo is the UI face; IBM Plex Mono carries anything the tool will parse — YAML, paths, keys,
commands, and numbers the user compares digit by digit.
