//! Persisted theme selection: a palette (Qrate, Gruvbox, Everforest) and light or dark. Both
//! are machine settings — they follow the screen, not the institution.

use gpui::{App, SharedString};
use gpui_component::{
    Theme, ThemeMode,
    setting::{SettingField, SettingItem},
};
use settings::AppSettings;

pub const DARK_THEME_KEY: &str = "dark_theme";
pub const PALETTE_KEY: &str = "theme_palette";

fn palette_key(cx: &App) -> SharedString {
    AppSettings::get(cx)
        .values
        .get(PALETTE_KEY)
        .map(|value| value.text())
        .filter(|key| !key.is_empty())
        .unwrap_or_else(|| ui::theme::PALETTES[0].key.into())
}

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
    ui::theme::install(ui::theme::palette(&palette_key(cx)), cx);
    apply(dark_theme(cx), cx);
}

pub fn palette_setting() -> SettingItem {
    let options = ui::theme::PALETTES
        .iter()
        .map(|p| (SharedString::from(p.key), SharedString::from(p.label)))
        .collect();
    SettingItem::new(
        "Palette",
        SettingField::dropdown(options, palette_key, |key, cx| {
            AppSettings::set_text(PALETTE_KEY, key.clone(), cx);
            ui::theme::install(ui::theme::palette(&key), cx);
            cx.refresh_windows();
        }),
    )
    .description("Qrate is the design's own; Gruvbox and Everforest keep its layout in their hues.")
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
