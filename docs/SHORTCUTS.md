# Keyboard shortcuts

Workbench uses the platform shortcut key: **Ctrl** on Windows and Linux, and **Command** on
macOS. The same shortcuts appear beside their commands in the title-bar menus.

| Command | Windows and Linux | macOS |
| --- | --- | --- |
| Open Settings | `Ctrl+,` | `Command+,` |
| Toggle the run log | `` Ctrl+` `` | `` Control+` `` |
| Dock or float the Config Builder | `Ctrl+\` | `Command+\` |
| Copy the log selection (or all of it) | `Ctrl+C` | `Command+C` |
| Select the whole log | `Ctrl+A` | `Command+A` |
| Close the window | (the window's close button) | `Command+W` |
| Quit | (the menu's Quit) | `Command+Q` |

The log shortcut opens the bottom Log panel when it is closed and closes it when it is open. On
macOS it stays on Control, because Command+` is the system's "next window of this app".

Closing a window with Command+W asks the same questions as its close button: a builder with
unsaved edits asks before discarding them, and the main window refuses to close during a run.
Closing the main window on macOS leaves the app running, as Mac apps do; click its Dock icon to
bring the window back.

The builder shortcut works from a builder window (docking it into the main window's right side)
and from the docked builder (popping it out to its own window). From elsewhere in the main window
it floats the docked builder, or brings back the one that was floated out.
