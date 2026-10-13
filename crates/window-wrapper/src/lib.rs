pub mod bar;
pub mod status_bar;
pub mod title_bar;

pub use bar::{BarItem, BarItems, BarRegistry};
use gpui::*;
pub use gpui_component::TitleBar;

use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = this_app)]
#[serde(deny_unknown_fields)]
pub struct OpenBrowser {
    pub url: String,
}

/// Global flag set by the workspace while a check or ingest run is in progress.
/// The title bar reads this to show a status indicator; main registers a close
/// handler that blocks the window from being closed while locked.
#[derive(Clone, Default)]
pub struct WindowLock {
    pub locked: bool,
    /// What the title bar says while locked.
    pub label: SharedString,
    /// Whether the window refuses to close. A Workbench run does (closing would orphan it); a
    /// preprocess, which has no way to be stopped, does not — it only blocks profile switches.
    pub blocks_close: bool,
}

impl Global for WindowLock {}

impl WindowLock {
    /// A Workbench run (or provisioning): locked, and the window will not close.
    pub fn set(locked: bool, cx: &mut App) {
        Self::set_with(locked, "Ingest running", true, cx);
    }

    /// Preprocessing: locked against profile switches, but the window may still close.
    pub fn set_preprocessing(locked: bool, cx: &mut App) {
        Self::set_with(locked, "Preprocessing", false, cx);
    }

    /// A run's config is being prepared: locked against profile switches and builder saves,
    /// which would race the config rewrite, for the moment it takes.
    pub fn set_preparing(locked: bool, cx: &mut App) {
        Self::set_with(locked, "Preparing a run", false, cx);
    }

    fn set_with(locked: bool, label: &'static str, blocks_close: bool, cx: &mut App) {
        // set_global (not global_mut) pushes NotifyGlobalObservers so observe_global fires.
        cx.set_global(Self {
            locked,
            label: label.into(),
            blocks_close: locked && blocks_close,
        });
    }

    pub fn blocks_close(cx: &App) -> bool {
        cx.try_global::<Self>().is_some_and(|l| l.blocks_close)
    }

    pub fn label(cx: &App) -> SharedString {
        cx.try_global::<Self>()
            .map(|l| l.label.clone())
            .unwrap_or_default()
    }

    pub fn is_locked(cx: &App) -> bool {
        cx.try_global::<Self>().map(|l| l.locked).unwrap_or(false)
    }
}
