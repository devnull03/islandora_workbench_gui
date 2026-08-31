//! Persisted light/dark theme selection.

use gpui::App;
use gpui_component::{
    Theme, ThemeMode,
    setting::{SettingField, SettingItem},
};
use settings::AppSettings;

pub const DARK_THEME_KEY: &str = "dark_theme";

fn dark_theme(cx: &App) -> bool {
    AppSettings::get(cx)
        .values
        .get(DARK_THEME_KEY)
        .map(|value| value.bool())
        .unwrap_or(true)
}

fn apply(dark: bool, cx: &mut App) {
    Theme::change(
        if dark {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        },
        None,
        cx,
    );
    cx.refresh_windows();
}

/// Apply the saved preference at startup. A missing value deliberately defaults to dark because
/// the application design and its primary theme were authored on a dark ground.
pub fn init(cx: &mut App) {
    if !AppSettings::get(cx).values.contains_key(DARK_THEME_KEY) {
        AppSettings::set_bool(DARK_THEME_KEY, true, cx);
    }
    apply(dark_theme(cx), cx);
}

pub fn appearance_setting() -> SettingItem {
    SettingItem::new(
        "Dark theme",
        SettingField::switch(dark_theme, |dark, cx| {
            AppSettings::set_bool(DARK_THEME_KEY, dark, cx);
            apply(dark, cx);
        }),
    )
    .description("Use the dark application palette. Turn this off for the light palette.")
}
