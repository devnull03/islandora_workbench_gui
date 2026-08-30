//! Persisted preferences, reusable setting field builders, path picker widgets, and a generic
//! settings-window shell (`SettingsWindow`). Product-specific pages live in `app`.

pub mod path_picker;

mod db;

pub use db::{SettingsWriter, data_dir, load_app_settings};
use gpui_component::Sizable;

/// Increment when the persisted SQLite JSON schema (`db::PersistSettings`) changes.
pub const SETTINGS_SCHEMA_VERSION: u32 = 2;

/// `AppSettings` value key for the Settings window's last size (a JSON [`MainWindowBounds`]).
pub const SETTINGS_WINDOW_BOUNDS_KEY: &str = "settings_window_bounds";

use std::path::PathBuf;
use std::sync::Arc;
use std::{collections::HashMap, env};

use gpui::*;
use gpui_component::{
    IconName, StyledExt, TitleBar,
    button::Button,
    h_flex,
    input::InputState,
    label::Label,
    scroll::ScrollableElement,
    setting::{SettingField, SettingItem, SettingPage, Settings},
    v_flex,
};
use serde::{Deserialize, Serialize};

use crate::path_picker::PathPickerApp;

// --- Setting Field Enum ---

pub enum Setting {
    Text {
        key: &'static str,
        label: &'static str,
        description: &'static str,
    },
    Switch {
        key: &'static str,
        label: &'static str,
        description: &'static str,
    },
    Dropdown {
        key: &'static str,
        label: &'static str,
        description: &'static str,
        options: &'static [(&'static str, &'static str)],
    },
    FilePicker {
        key: &'static str,
        label: &'static str,
        description: &'static str,
        prompt: &'static str,
    },
    DirPicker {
        key: &'static str,
        label: &'static str,
        description: &'static str,
        prompt: &'static str,
    },
}

impl From<Setting> for SettingItem {
    fn from(setting: Setting) -> Self {
        match setting {
            Setting::Text {
                key,
                label,
                description,
            } => SettingItem::new(
                label,
                SettingField::input(
                    move |cx: &App| {
                        AppSettings::get(cx)
                            .values
                            .get(key)
                            .map(|v| v.text())
                            .unwrap_or_default()
                    },
                    move |val: SharedString, cx: &mut App| {
                        AppSettings::set_text(key, val, cx);
                    },
                ),
            )
            .description(description),

            Setting::Switch {
                key,
                label,
                description,
            } => SettingItem::new(
                label,
                SettingField::switch(
                    move |cx: &App| {
                        AppSettings::get(cx)
                            .values
                            .get(key)
                            .map(|v| v.bool())
                            .unwrap_or(false)
                    },
                    move |val: bool, cx: &mut App| {
                        AppSettings::set_bool(key, val, cx);
                    },
                ),
            )
            .description(description),

            Setting::Dropdown {
                key,
                label,
                description,
                options,
            } => {
                let opts: Vec<(SharedString, SharedString)> = options
                    .iter()
                    .map(|(k, v)| ((*k).into(), (*v).into()))
                    .collect();
                SettingItem::new(
                    label,
                    SettingField::dropdown(
                        opts,
                        move |cx: &App| {
                            AppSettings::get(cx)
                                .values
                                .get(key)
                                .map(|v| v.text())
                                .unwrap_or_default()
                        },
                        move |val: SharedString, cx: &mut App| {
                            AppSettings::set_text(key, val, cx);
                        },
                    ),
                )
                .description(description)
            }

            Setting::FilePicker {
                key,
                label,
                description,
                prompt,
            } => build_path_picker(key, label, description, prompt, true, false),

            Setting::DirPicker {
                key,
                label,
                description,
                prompt,
            } => build_path_picker(key, label, description, prompt, false, true),
        }
    }
}

fn build_path_picker(
    key: &'static str,
    label: &'static str,
    description: &'static str,
    prompt: &'static str,
    files: bool,
    directories: bool,
) -> SettingItem {
    let prompt: SharedString = prompt.into();
    SettingItem::new(
        label,
        SettingField::render(move |options, window, cx| {
            let want = AppSettings::get(cx)
                .values
                .get(key)
                .map(|v| v.text())
                .unwrap_or_default();
            let input = window.use_keyed_state(
                SharedString::from(format!(
                    "path-picker-{}-{}-{}",
                    options.page_ix, options.group_ix, options.item_ix
                )),
                cx,
                |window, cx| {
                    InputState::new(window, cx)
                        .placeholder("No file selected...")
                        .default_value(want.clone())
                },
            );
            input.update(cx, |state, cx| {
                if state.value() != want {
                    state.set_value(want.to_string(), window, cx);
                }
            });
            PathPickerApp {
                layout: options.layout,
                field_size: options.size,
                button_size: Some(options.size),
                button_id: SharedString::from(format!("browse-{}", key)),
                files,
                directories,
                prompt: prompt.clone(),
                input,
                on_pick: Arc::new(move |val, cx| {
                    AppSettings::set_text(key, val, cx);
                }),
            }
        }),
    )
    .description(description)
}

// --- Setting Value ---

#[derive(Clone)]
pub enum Val {
    Text(SharedString),
    Bool(bool),
}

impl Val {
    pub fn text(&self) -> SharedString {
        match self {
            Val::Text(s) => s.clone(),
            Val::Bool(b) => if *b { "true" } else { "false" }.into(),
        }
    }

    pub fn bool(&self) -> bool {
        match self {
            Val::Bool(b) => *b,
            Val::Text(s) => s == "true",
        }
    }
}

// --- Config Types ---

#[derive(Clone, Default)]
pub struct TaskConfig {
    pub label: SharedString,
    pub task_name: SharedString,
    pub file_path: SharedString,
}

#[derive(Clone, Default)]
pub struct ServerConfig {
    pub label: SharedString,
    pub server_url: SharedString,
    pub credentials_file: SharedString,
    /// Destructive tasks on this server always prompt, whatever `auto_accept_prompts` says.
    /// A production host earns this; a scratch VM does not.
    pub needs_confirmation: bool,
    pub last_check: Option<CheckResult>,
}

/// What the last **Test** found, shown on the server's row so a bad pairing is visible before a
/// run rather than during one (mockup `3b`).
///
/// Reachability and credentials are separate because they fail separately: an unreachable host
/// says nothing about whether the password is right, and reporting them as one verdict would
/// claim knowledge we do not have.
#[derive(Clone, Debug, PartialEq)]
pub struct CheckResult {
    /// Unix seconds. Rendered as an age ("checked 2 min ago") rather than a clock time, which
    /// would need a timezone database to be honest about.
    pub at: u64,
    pub reachable: bool,
    /// `None` when the host was unreachable, so the credentials were never sent anywhere.
    pub credentials_ok: Option<bool>,
    pub message: SharedString,
}

impl CheckResult {
    pub fn now(
        reachable: bool,
        credentials_ok: Option<bool>,
        message: impl Into<SharedString>,
    ) -> Self {
        Self {
            at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            reachable,
            credentials_ok,
            message: message.into(),
        }
    }

    /// `just now` · `4 min ago` · `2 h ago` · `3 days ago`.
    pub fn age(&self) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        match now.saturating_sub(self.at) {
            s if s < 60 => "just now".to_string(),
            s if s < 3600 => format!("{} min ago", s / 60),
            s if s < 86_400 => format!("{} h ago", s / 3600),
            s => format!("{} days ago", s / 86_400),
        }
    }

    /// True only when both stages passed.
    pub fn is_ok(&self) -> bool {
        self.reachable && self.credentials_ok == Some(true)
    }
}

/// Last main window size and display, for restore on launch (position is not persisted).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MainWindowBounds {
    pub width: f32,
    pub height: f32,
    /// `PlatformDisplay::id` as `u32`; matched at startup against [`App::displays`].
    #[serde(default)]
    pub display_id: Option<u32>,
}

impl MainWindowBounds {
    pub fn capture_from_window(window: &Window, cx: &App) -> Self {
        let b = window.bounds();
        Self {
            width: b.size.width.into(),
            height: b.size.height.into(),
            display_id: window.display(cx).map(|d| u32::from(d.id())),
        }
    }
}

// --- App Settings ---

pub fn picker_with_path_button(
    key: &'static str,
    label: &'static str,
    description: &'static str,
    prompt: &'static str,
    path_candidates: Vec<&'static str>,
) -> SettingItem {
    let prompt: SharedString = prompt.into();
    SettingItem::new(
        label,
        SettingField::render(move |options, window, cx| {
            let want = AppSettings::get(cx)
                .values
                .get(key)
                .map(|v| v.text())
                .unwrap_or_default();

            let input = window.use_keyed_state(
                SharedString::from(format!(
                    "path-picker-pathbtn-{}-{}-{}",
                    options.page_ix, options.group_ix, options.item_ix
                )),
                cx,
                |window, cx| {
                    InputState::new(window, cx)
                        .placeholder("No file selected...")
                        .default_value(want.clone())
                },
            );

            input.update(cx, |state, cx| {
                if state.value() != want {
                    state.set_value(want.to_string(), window, cx);
                }
            });

            let on_pick_key = key;
            let on_path_key = key;
            let path_candidates = path_candidates.clone();

            h_flex()
                .gap_2()
                .w_full()
                .child(PathPickerApp {
                    layout: options.layout,
                    field_size: options.size,
                    button_size: Some(options.size),
                    button_id: SharedString::from(format!("browse-{}", key)),
                    files: true,
                    directories: false,
                    prompt: prompt.clone(),
                    input: input.clone(),
                    on_pick: std::sync::Arc::new(move |val, cx| {
                        AppSettings::set_text(on_pick_key, val, cx);
                    }),
                })
                .child(
                    Button::new(SharedString::from(format!("get-from-path-{}", key)))
                        .outline()
                        .icon(IconName::Redo2)
                        .tooltip("Get from PATH")
                        .with_size(options.size)
                        .on_click(move |_, _, cx| {
                            if let Some(p) = find_on_path(&path_candidates) {
                                AppSettings::set_text(
                                    on_path_key,
                                    p.to_string_lossy().to_string().into(),
                                    cx,
                                );
                            }
                        }),
                )
        }),
    )
    .description(description)
}

pub struct AppSettings {
    pub values: HashMap<String, Val>,
    pub task_configs: Vec<TaskConfig>,
    pub server_configs: Vec<ServerConfig>,
    pub main_window_bounds: Option<MainWindowBounds>,
    pub default_task_config: Option<SharedString>,
    pub default_server: Option<SharedString>,
    /// Schema version last read from disk (`settings_version` in JSON). See [`SETTINGS_SCHEMA_VERSION`].
    #[allow(dead_code)] // migrations / future UI; written on each save via `db`
    pub settings_schema_version: u32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            values: HashMap::new(),
            task_configs: Vec::new(),
            server_configs: Vec::new(),
            main_window_bounds: None,
            default_task_config: None,
            default_server: None,
            settings_schema_version: SETTINGS_SCHEMA_VERSION,
        }
    }
}

impl Global for AppSettings {}

#[derive(Clone, Default)]
pub struct SettingsPersistence {
    pub writer: Option<SettingsWriter>,
}

impl Global for SettingsPersistence {}

impl AppSettings {
    pub fn get(cx: &App) -> &Self {
        cx.global::<Self>()
    }
    pub fn get_mut(cx: &mut App) -> &mut Self {
        cx.global_mut::<Self>()
    }

    fn main_window_resolved_display(&self, cx: &App) -> Option<DisplayId> {
        self.main_window_bounds
            .as_ref()
            .and_then(|b| b.display_id)
            .and_then(|raw| {
                cx.displays()
                    .into_iter()
                    .find(|d| u32::from(d.id()) == raw)
                    .map(|d| d.id())
            })
    }

    /// Window size and target display for startup. The frame is always **centered** on the
    /// remembered display (or primary); window **position is not restored** from disk.
    /// Invalid/missing size falls back to 600×800 centered the same way.
    pub fn main_window_startup_placement(&self, cx: &App) -> (Bounds<Pixels>, Option<DisplayId>) {
        let display = self.main_window_resolved_display(cx);
        const MIN_W: f32 = 400.0;
        const MIN_H: f32 = 250.0;
        const DEFAULT_W: f32 = 600.0;
        const DEFAULT_H: f32 = 800.0;
        if let Some(b) = &self.main_window_bounds
            && b.width.is_finite()
            && b.height.is_finite()
            && b.width >= MIN_W
            && b.height >= MIN_H
        {
            let bounds = Bounds::centered(display, size(px(b.width), px(b.height)), cx);
            return (bounds, display);
        }
        let bounds = Bounds::centered(display, size(px(DEFAULT_W), px(DEFAULT_H)), cx);
        (bounds, display)
    }

    /// Single mutation entrypoint so we can trigger persistence.
    pub fn update<R>(cx: &mut App, f: impl FnOnce(&mut Self) -> R) -> R {
        let r = {
            let s = cx.global_mut::<Self>();
            f(s)
        };
        if let Some(writer) = cx.global::<SettingsPersistence>().writer.clone() {
            let snapshot = cx.global::<Self>();
            writer.enqueue_save(snapshot);
        }
        r
    }

    pub fn set_text(key: &'static str, val: SharedString, cx: &mut App) {
        Self::update(cx, |s| {
            s.values.insert(key.into(), Val::Text(val));
        });
    }

    pub fn set_bool(key: &'static str, val: bool, cx: &mut App) {
        Self::update(cx, |s| {
            s.values.insert(key.into(), Val::Bool(val));
        });
    }

    /// Append a config, or replace the one at `index`. One entry point for both because the
    /// list's last row is the add form — the only difference is whether a row already exists.
    pub fn upsert_task_config(index: Option<usize>, config: TaskConfig, cx: &mut App) {
        if config.label.is_empty() || config.task_name.is_empty() || config.file_path.is_empty() {
            return;
        }
        Self::update(cx, |s| match index {
            Some(i) if i < s.task_configs.len() => s.task_configs[i] = config,
            _ => s.task_configs.push(config),
        });
    }

    pub fn remove_task_config(index: usize, cx: &mut App) {
        Self::update(cx, |s| {
            if index < s.task_configs.len() {
                s.task_configs.remove(index);
            }
        });
    }

    /// As [`Self::upsert_task_config`]. `last_check` is preserved across an edit that does not
    /// change the URL or the credentials file — editing a label does not invalidate a test.
    pub fn upsert_server_config(index: Option<usize>, mut config: ServerConfig, cx: &mut App) {
        if config.label.is_empty() || config.server_url.is_empty() {
            return;
        }
        Self::update(cx, |s| match index {
            Some(i) if i < s.server_configs.len() => {
                let old = &s.server_configs[i];
                let same_target = old.server_url == config.server_url
                    && old.credentials_file == config.credentials_file;
                if same_target && config.last_check.is_none() {
                    config.last_check = old.last_check.clone();
                }
                s.server_configs[i] = config;
            }
            _ => s.server_configs.push(config),
        });
    }

    pub fn remove_server_config(index: usize, cx: &mut App) {
        Self::update(cx, |s| {
            if index < s.server_configs.len() {
                s.server_configs.remove(index);
            }
        });
    }

    /// Record what a [`CheckResult`] found, without disturbing anything the user has typed.
    pub fn set_server_check(index: usize, result: CheckResult, cx: &mut App) {
        Self::update(cx, |s| {
            if let Some(server) = s.server_configs.get_mut(index) {
                server.last_check = Some(result);
            }
        });
    }

    pub fn set_default_task_config(label: Option<SharedString>, cx: &mut App) {
        Self::update(cx, |s| {
            s.default_task_config = label;
        });
    }

    pub fn set_default_server(label: Option<SharedString>, cx: &mut App) {
        Self::update(cx, |s| {
            s.default_server = label;
        });
    }
}

// --- Settings Window State ---

#[derive(Clone, Default)]
pub struct SettingsWindowHandle {
    pub handle: Option<AnyWindowHandle>,
}

impl Global for SettingsWindowHandle {}

// --- Settings Window ---

pub struct SettingsWindow {
    /// Takes `&App` so a page can build itself from live state — the Servers and Task Configs
    /// pages list what is currently saved, which a context-free builder cannot see. Re-invoked
    /// every render.
    pub build_pages: fn(&App) -> Vec<SettingPage>,
    /// Persists the window's size (debounced through the settings writer) so it reopens where it
    /// was left.
    _bounds_sub: Subscription,
    /// Re-render when any setting changes, so a page built from live state rebuilds as soon as
    /// that state moves rather than on the next unrelated repaint. Adding a server and seeing the
    /// list above stay stale is the bug this fixes.
    _settings_sub: Subscription,
}

impl SettingsWindow {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        build_pages: fn(&App) -> Vec<SettingPage>,
    ) -> Self {
        window.set_window_title("Settings — Islandora Workbench");
        let _bounds_sub = cx.observe_window_bounds(window, |_this, window, cx| {
            let bounds = MainWindowBounds::capture_from_window(window, cx);
            if let Ok(json) = serde_json::to_string(&bounds) {
                AppSettings::set_text(SETTINGS_WINDOW_BOUNDS_KEY, json.into(), cx);
            }
        });
        let _settings_sub = cx.observe_global::<AppSettings>(|_this, cx| cx.notify());
        Self {
            build_pages,
            _bounds_sub,
            _settings_sub,
        }
    }

    /// Where to open the Settings window: the size it was last left at, centred on the display
    /// the main window is on. Falls back to a sensible default the first time.
    pub fn startup_placement(cx: &App) -> (Bounds<Pixels>, Option<Size<Pixels>>) {
        const MIN: Size<Pixels> = Size {
            width: px(600.0),
            height: px(400.0),
        };
        let saved = AppSettings::get(cx)
            .values
            .get(SETTINGS_WINDOW_BOUNDS_KEY)
            .map(|v| v.text())
            .and_then(|json| serde_json::from_str::<MainWindowBounds>(&json).ok())
            // A saved size below the window minimum would be honoured as an opening size the user
            // cannot reproduce by dragging, so treat it as no saved size at all.
            .filter(|b| {
                b.width.is_finite()
                    && b.height.is_finite()
                    && px(b.width) >= MIN.width
                    && px(b.height) >= MIN.height
            })
            .map(|b| size(px(b.width), px(b.height)))
            .unwrap_or_else(|| size(px(1000.0), px(800.0)));
        (Bounds::centered(None, saved, cx), Some(MIN))
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .child(TitleBar::new().child(Label::new("Settings").font_semibold()))
            .child(
                // `Settings` draws its own search field and filters through
                // `SettingItem::is_match`, so this window adds none of its own.
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scrollbar()
                    .child(Settings::new("app-settings").pages((self.build_pages)(cx))),
            )
    }
}

pub fn find_on_path(candidates: &[&str]) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for dir in env::split_paths(&path) {
        for name in candidates {
            let p = dir.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}
