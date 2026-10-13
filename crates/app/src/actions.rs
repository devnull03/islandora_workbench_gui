//! Window-wide keyboard shortcuts. `secondary` is Command on macOS and Control elsewhere.
//! macOS also gets its standard Cmd+Q and Cmd+W, and the log keeps Control+` there because
//! Cmd+` is the system's "next window".

use gpui::KeyBinding;

use config_builder::ToggleBuilderDock;

use crate::app_menus::{CloseWindow, OpenSettings, Quit, ToggleLog};

pub fn key_bindings() -> Vec<KeyBinding> {
    let mut bindings = vec![
        KeyBinding::new("secondary-,", OpenSettings, None),
        KeyBinding::new("ctrl-`", ToggleLog, None),
        // Stage 4: dock a floating builder, or float the docked one.
        KeyBinding::new("secondary-\\", ToggleBuilderDock, None),
    ];
    if cfg!(target_os = "macos") {
        bindings.extend([
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("cmd-w", CloseWindow, None),
        ]);
    }
    bindings
}
