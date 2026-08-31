//! Window-wide keyboard shortcuts. `secondary` is Command on macOS and Control elsewhere.

use gpui::KeyBinding;

use crate::app_menus::{OpenSettings, ToggleLog};

pub fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("secondary-,", OpenSettings, None),
        KeyBinding::new("secondary-`", ToggleLog, None),
    ]
}
