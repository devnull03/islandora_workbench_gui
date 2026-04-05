use std::collections::HashMap;

use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::{
    ActiveTheme, IconName, Sizable, StyledExt, TitleBar,
    button::{Button, ButtonVariants},
    h_flex,
    input::InputState,
    label::Label,
    scroll::ScrollableElement,
    setting::{Settings, SettingPage, SettingGroup, SettingItem, SettingField},
    v_flex,
};

use std::sync::Arc;

use crate::path_picker::PathPickerApp;

// --- Setting Definitions ---

#[derive(Clone)]
pub enum SettingType {
    Text,
    Switch,
    Dropdown(&'static [(&'static str, &'static str)]),
    FilePicker { files: bool, directories: bool, prompt: &'static str },
}

#[derive(Clone)]
pub struct SettingDef {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub setting_type: SettingType,
}

pub const WORKBENCH_SETTINGS: &[SettingDef] = &[
    SettingDef {
        key: "workbench_path",
        label: "Workbench Path",
        description: "Path to Islandora Workbench installation directory",
        setting_type: SettingType::FilePicker {
            files: false,
            directories: true,
            prompt: "Select workbench directory",
        },
    },
];

pub const PYTHON_SETTINGS: &[SettingDef] = &[
    SettingDef {
        key: "python_path",
        label: "Python Path",
        description: "Path to Python executable",
        setting_type: SettingType::FilePicker {
            files: true,
            directories: false,
            prompt: "Select Python executable",
        },
    },
    SettingDef {
        key: "uv_path",
        label: "UV Path",
        description: "Path to UV package manager (optional)",
        setting_type: SettingType::FilePicker {
            files: true,
            directories: false,
            prompt: "Select UV executable",
        },
    },
    SettingDef {
        key: "use_uv",
        label: "Use UV",
        description: "Use UV instead of pip for running workbench",
        setting_type: SettingType::Switch,
    },
];

pub const NEW_TASK_SETTINGS: &[SettingDef] = &[
    SettingDef {
        key: "new_task_label",
        label: "Label",
        description: "Display name for this configuration",
        setting_type: SettingType::Text,
    },
    SettingDef {
        key: "new_task_name",
        label: "Task Name",
        description: "Workbench task to perform",
        setting_type: SettingType::Dropdown(&[
            ("create", "create - Create new nodes"),
            ("create_from_files", "create_from_files - Create from files"),
            ("update", "update - Update existing nodes"),
            ("delete", "delete - Delete nodes"),
            ("add_media", "add_media - Add media to nodes"),
            ("update_media", "update_media - Update existing media"),
            ("delete_media", "delete_media - Delete media"),
            ("delete_media_by_node", "delete_media_by_node - Delete media by node"),
            ("export_csv", "export_csv - Export content to CSV"),
            ("get_data_from_view", "get_data_from_view - Get data from view"),
        ]),
    },
    SettingDef {
        key: "new_task_path",
        label: "Config File",
        description: "Path to YAML configuration file",
        setting_type: SettingType::FilePicker { files: true, directories: false, prompt: "Select config file" },
    },
];

// --- Setting Value Storage ---

#[derive(Clone)]
pub enum SettingValue {
    Text(SharedString),
    Bool(bool),
}

impl SettingValue {
    pub fn as_text(&self) -> SharedString {
        match self {
            SettingValue::Text(s) => s.clone(),
            SettingValue::Bool(b) => if *b { "true" } else { "false" }.into(),
        }
    }

    pub fn as_bool(&self) -> bool {
        match self {
            SettingValue::Bool(b) => *b,
            SettingValue::Text(s) => s == "true",
        }
    }
}

// --- Task Config ---

#[derive(Clone)]
pub struct TaskConfig {
    pub label: String,
    pub task_name: String,
    pub file_path: String,
}

// --- App Settings ---

#[derive(Clone)]
pub struct AppSettings {
    values: HashMap<String, SettingValue>,
    pub task_configs: Vec<TaskConfig>,
}

impl Default for AppSettings {
    fn default() -> Self {
        let mut values = HashMap::new();
        // Initialize with defaults
        values.insert("workbench_path".into(), SettingValue::Text("".into()));
        values.insert("python_path".into(), SettingValue::Text("".into()));
        values.insert("uv_path".into(), SettingValue::Text("".into()));
        values.insert("use_uv".into(), SettingValue::Bool(false));
        values.insert("new_task_label".into(), SettingValue::Text("".into()));
        values.insert("new_task_name".into(), SettingValue::Text("".into()));
        values.insert("new_task_path".into(), SettingValue::Text("".into()));

        Self {
            values,
            task_configs: vec![],
        }
    }
}

impl Global for AppSettings {}

impl AppSettings {
    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    pub fn global_mut(cx: &mut App) -> &mut Self {
        cx.global_mut::<Self>()
    }

    pub fn get_text(&self, key: &str) -> SharedString {
        self.values.get(key).map(|v| v.as_text()).unwrap_or_default()
    }

    pub fn set_text(&mut self, key: &str, val: SharedString) {
        self.values.insert(key.to_string(), SettingValue::Text(val));
    }

    pub fn get_bool(&self, key: &str) -> bool {
        self.values.get(key).map(|v| v.as_bool()).unwrap_or(false)
    }

    pub fn set_bool(&mut self, key: &str, val: bool) {
        self.values.insert(key.to_string(), SettingValue::Bool(val));
    }

    pub fn add_task_config(cx: &mut App) {
        let settings = Self::global(cx);
        let label = settings.get_text("new_task_label").to_string();
        let task_name = settings.get_text("new_task_name").to_string();
        let file_path = settings.get_text("new_task_path").to_string();

        if label.is_empty() || task_name.is_empty() || file_path.is_empty() {
            return;
        }

        let settings = Self::global_mut(cx);
        settings.task_configs.push(TaskConfig {
            label,
            task_name,
            file_path,
        });
        settings.set_text("new_task_label", "".into());
        settings.set_text("new_task_name", "".into());
        settings.set_text("new_task_path", "".into());
    }

    pub fn remove_task_config(index: usize, cx: &mut App) {
        let settings = Self::global_mut(cx);
        if index < settings.task_configs.len() {
            settings.task_configs.remove(index);
        }
    }
}

// --- Build SettingItems from definitions ---

fn build_setting_items(defs: &'static [SettingDef]) -> Vec<SettingItem> {
    defs.iter().map(|def| {
        let key = def.key;
        let item = match &def.setting_type {
            SettingType::Text => SettingItem::new(
                def.label,
                SettingField::input(
                    move |cx: &App| AppSettings::global(cx).get_text(key),
                    move |val: SharedString, cx: &mut App| {
                        AppSettings::global_mut(cx).set_text(key, val);
                    },
                )
            ),
            SettingType::Switch => SettingItem::new(
                def.label,
                SettingField::switch(
                    move |cx: &App| AppSettings::global(cx).get_bool(key),
                    move |val: bool, cx: &mut App| {
                        AppSettings::global_mut(cx).set_bool(key, val);
                    },
                )
            ),
            SettingType::Dropdown(options) => {
                let opts: Vec<(SharedString, SharedString)> = options
                    .iter()
                    .map(|(k, v)| ((*k).into(), (*v).into()))
                    .collect();
                SettingItem::new(
                    def.label,
                    SettingField::dropdown(
                        opts,
                        move |cx: &App| AppSettings::global(cx).get_text(key),
                        move |val: SharedString, cx: &mut App| {
                            AppSettings::global_mut(cx).set_text(key, val);
                        },
                    )
                )
            }
            SettingType::FilePicker { files, directories, prompt } => {
                let files = *files;
                let directories = *directories;
                let prompt: SharedString = (*prompt).into();
                SettingItem::new(
                    def.label,
                    SettingField::render(move |options, window, cx| {
                        // Same pattern as gpui-component `StringField`: window-scoped keyed state.
                        let input = window.use_keyed_state(
                            SharedString::from(format!(
                                "path-picker-{}-{}-{}",
                                options.page_ix, options.group_ix, options.item_ix
                            )),
                            cx,
                            |window, cx| {
                                let want = AppSettings::global(cx).get_text(key);
                                InputState::new(window, cx)
                                    .placeholder("No file selected...")
                                    .default_value(want)
                            },
                        );
                        let want = AppSettings::global(cx).get_text(key);
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
                                AppSettings::global_mut(cx).set_text(key, val);
                            }),
                        }
                    }),
                )
            }
        };
        item.description(def.description)
    }).collect()
}

fn render_task_config_item(idx: usize, config: &TaskConfig, cx: &App) -> impl IntoElement {
    h_flex()
        .gap_2()
        .items_center()
        .p_2()
        .w_full()
        .rounded_md()
        .bg(cx.theme().colors.secondary)
        .child(
            v_flex()
                .flex_1()
                .overflow_hidden()
                .child(
                    div().max_w(px(250.)).overflow_hidden().text_ellipsis()
                        .child(Label::new(config.label.clone()).font_semibold())
                )
                .child(
                    div().max_w(px(250.)).overflow_hidden().text_ellipsis()
                        .child(Label::new(format!("Task: {}", config.task_name))
                            .text_xs().text_color(cx.theme().colors.muted_foreground))
                )
                .child(
                    div().max_w(px(250.)).overflow_hidden().text_ellipsis()
                        .child(Label::new(config.file_path.clone())
                            .text_xs().text_color(cx.theme().colors.muted_foreground))
                )
        )
        .child(
            Button::new(SharedString::from(format!("remove-{}", idx)))
                .icon(IconName::Close).ghost().xsmall()
                .on_click(move |_, _, cx| AppSettings::remove_task_config(idx, cx))
        )
}

fn build_saved_configs_item() -> SettingItem {
    SettingItem::new(
        "Configurations",
        SettingField::render(|_options, _window, cx| {
            let configs = AppSettings::global(cx).task_configs.clone();
            
            v_flex()
                .gap_2()
                .w_full()
                .children(configs.iter().enumerate().map(|(idx, config)| {
                    render_task_config_item(idx, config, cx)
                }))
                .when(configs.is_empty(), |this| {
                    this.child(Label::new("No configurations added yet.")
                        .text_color(cx.theme().colors.muted_foreground))
                })
        })
    )
    .layout(Axis::Vertical)
}

fn build_add_button_item() -> SettingItem {
    SettingItem::new(
        "",
        SettingField::render(|_options, _window, _cx| {
            h_flex()
                .justify_end()
                .w_full()
                .child(
                    Button::new("add-config")
                        .primary().small()
                        .label("Add Configuration")
                        .on_click(|_, _, cx| AppSettings::add_task_config(cx))
                )
        })
    )
}

fn build_pages() -> Vec<SettingPage> {
    vec![
        SettingPage::new("Paths")
            .default_open(true)
            .groups(vec![
                SettingGroup::new()
                    .title("Workbench Installation")
                    .items(build_setting_items(WORKBENCH_SETTINGS)),
                SettingGroup::new()
                    .title("Python Environment")
                    .items(build_setting_items(PYTHON_SETTINGS)),
            ]),
        SettingPage::new("Task Configs")
            .default_open(true)
            .groups(vec![
                SettingGroup::new()
                    .title("Saved Configurations")
                    .items(vec![build_saved_configs_item()]),
                SettingGroup::new()
                    .title("Add New Configuration")
                    .items({
                        let mut items = build_setting_items(NEW_TASK_SETTINGS);
                        items.push(build_add_button_item());
                        items
                    }),
            ]),
    ]
}

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
                            .on_click(move |_, window, _cx| window.remove_window())
                    )
            )
    }
}
