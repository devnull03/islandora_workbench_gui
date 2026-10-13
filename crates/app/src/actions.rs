//! Window-wide keyboard shortcuts. `secondary` is Command on macOS and Control elsewhere.

use gpui::KeyBinding;

use config_builder::ToggleBuilderDock;

use crate::app_menus::{OpenSettings, ToggleLog};

pub fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("secondary-,", OpenSettings, None),
        KeyBinding::new("secondary-`", ToggleLog, None),
        // Stage 4: dock a floating builder, or float the docked one.
        KeyBinding::new("secondary-\\", ToggleBuilderDock, None),
    ]
}
