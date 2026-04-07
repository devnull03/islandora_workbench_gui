mod custom_fields;
mod pages;

use std::collections::HashMap;
use std::sync::Arc;

use gpui::*;
use gpui_component::{
    ActiveTheme, StyledExt, TitleBar,
    button::Button,
    h_flex,
    input::InputState,
    label::Label,
    scroll::ScrollableElement,
    setting::{Settings, SettingItem, SettingField},
    v_flex,
};

use crate::path_picker::PathPickerApp;
use pages::build_pages;

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
            Setting::Text { key, label, description } => SettingItem::new(
                label,
                SettingField::input(
                    move |cx: &App| AppSettings::get(cx).values.get(key).map(|v| v.text()).unwrap_or_default(),
                    move |val: SharedString, cx: &mut App| {
                        AppSettings::get_mut(cx).values.insert(key.into(), Val::Text(val));
                    },
                ),
            )
            .description(description),

            Setting::Switch { key, label, description } => SettingItem::new(
                label,
                SettingField::switch(
                    move |cx: &App| AppSettings::get(cx).values.get(key).map(|v| v.bool()).unwrap_or(false),
                    move |val: bool, cx: &mut App| {
                        AppSettings::get_mut(cx).values.insert(key.into(), Val::Bool(val));
                    },
                ),
            )
            .description(description),

            Setting::Dropdown { key, label, description, options } => {
                let opts: Vec<(SharedString, SharedString)> = options
                    .iter()
                    .map(|(k, v)| ((*k).into(), (*v).into()))
                    .collect();
                SettingItem::new(
                    label,
                    SettingField::dropdown(
                        opts,
                        move |cx: &App| AppSettings::get(cx).values.get(key).map(|v| v.text()).unwrap_or_default(),
                        move |val: SharedString, cx: &mut App| {
                            AppSettings::get_mut(cx).values.insert(key.into(), Val::Text(val));
                        },
                    ),
                )
                .description(description)
            }

            Setting::FilePicker { key, label, description, prompt } => {
                build_path_picker(key, label, description, prompt, true, false)
            }

            Setting::DirPicker { key, label, description, prompt } => {
                build_path_picker(key, label, description, prompt, false, true)
            }
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
            let want = AppSettings::get(cx).values.get(key).map(|v| v.text()).unwrap_or_default();
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
                    AppSettings::get_mut(cx).values.insert(key.into(), Val::Text(val));
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

#[derive(Clone)]
pub struct TaskConfig {
    pub label: SharedString,
    pub task_name: SharedString,
    pub file_path: SharedString,
}

#[derive(Clone)]
pub struct ServerConfig {
    pub label: SharedString,
    pub server_url: SharedString,
    pub credentials_file: SharedString,
}

// --- App Settings ---

#[derive(Clone, Default)]
pub struct AppSettings {
    pub values: HashMap<String, Val>,
    pub task_configs: Vec<TaskConfig>,
    pub server_configs: Vec<ServerConfig>,
}

impl Global for AppSettings {}

impl AppSettings {
    pub fn get(cx: &App) -> &Self { cx.global::<Self>() }
    pub fn get_mut(cx: &mut App) -> &mut Self { cx.global_mut::<Self>() }

    pub fn add_task_config(cx: &mut App) {
        let s = Self::get(cx);
        let label = s.values.get("new_task_label").map(|v| v.text()).unwrap_or_default();
        let task_name = s.values.get("new_task_name").map(|v| v.text()).unwrap_or_default();
        let file_path = s.values.get("new_task_path").map(|v| v.text()).unwrap_or_default();

        if label.is_empty() || task_name.is_empty() || file_path.is_empty() {
            return;
        }

        let s = Self::get_mut(cx);
        s.task_configs.push(TaskConfig { label, task_name, file_path });
        s.values.remove("new_task_label");
        s.values.remove("new_task_name");
        s.values.remove("new_task_path");
    }

    pub fn remove_task_config(index: usize, cx: &mut App) {
        let s = Self::get_mut(cx);
        if index < s.task_configs.len() {
            s.task_configs.remove(index);
        }
    }

    pub fn add_server_config(cx: &mut App) {
        let s = Self::get(cx);
        let label = s.values.get("new_server_label").map(|v| v.text()).unwrap_or_default();
        let server_url = s.values.get("new_server_url").map(|v| v.text()).unwrap_or_default();
        let credentials_file = s.values.get("new_credentials_file").map(|v| v.text()).unwrap_or_default();

        if label.is_empty() || server_url.is_empty() || credentials_file.is_empty() {
            return;
        }

        let s = Self::get_mut(cx);
        s.server_configs.push(ServerConfig { label, server_url, credentials_file });
        s.values.remove("new_server_label");
        s.values.remove("new_server_url");
        s.values.remove("new_credentials_file");
    }

    pub fn remove_server_config(index: usize, cx: &mut App) {
        let s = Self::get_mut(cx);
        if index < s.server_configs.len() {
            s.server_configs.remove(index);
        }
    }
}

// --- Settings Window State ---

#[derive(Clone, Default)]
pub struct SettingsWindowHandle {
    pub handle: Option<AnyWindowHandle>,
}

impl Global for SettingsWindowHandle {}

// --- Settings Window ---

pub struct SettingsWindow;

impl SettingsWindow {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .child(TitleBar::new().child(Label::new("Settings").font_semibold()))
            .child(
                div()
                    .flex_1()
                    .overflow_y_scrollbar()
                    .child(Settings::new("app-settings").pages(build_pages()))
            )
            .child(
                h_flex()
                    .p_4()
                    .justify_end()
                    .border_t_1()
                    .border_color(cx.theme().colors.border)
                    .child(
                        Button::new("close")
                            .outline()
                            .label("Close")
                            .on_click(move |_: &gpui::ClickEvent, window: &mut Window, cx: &mut App| {
                                cx.update_global::<SettingsWindowHandle, _>(|state, _| {
                                    state.handle = None;
                                });
                                window.remove_window();
                            })
                    )
            )
    }
}
