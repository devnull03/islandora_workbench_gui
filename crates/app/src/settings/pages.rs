use std::{env, path::PathBuf};

use gpui::*;
use gpui_component::{
    IconName,
    Sizable,
    button::Button,
    h_flex,
    input::InputState,
    setting::{SettingGroup, SettingItem, SettingPage, SettingField},
};

use super::Setting;
use super::custom_fields::{
    add_config_button, add_server_button, saved_configs_field, saved_servers_field, TASK_OPTIONS,
};
use super::AppSettings;
use crate::path_picker::PathPickerApp;

fn find_on_path(candidates: &[&str]) -> Option<PathBuf> {
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

fn exe_candidates(base: &'static str) -> Vec<&'static str> {
    if cfg!(windows) {
        // Avoid concat! here because `base` is not a literal.
        if base == "python" {
            vec!["python.exe", "python"]
        } else if base == "uv" {
            vec!["uv.exe", "uv"]
        } else {
            vec![base]
        }
    } else {
        vec![base]
    }
}

fn picker_with_path_button(
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

pub fn build_pages() -> Vec<SettingPage> {
    vec![
        SettingPage::new("Paths").default_open(true).groups(vec![
            SettingGroup::new()
                .title("Workbench Installation")
                .items(vec![Setting::DirPicker {
                    key: "workbench_path",
                    label: "Workbench Path",
                    description: "Path to Islandora Workbench installation directory",
                    prompt: "Select workbench directory",
                }
                .into()]),
            SettingGroup::new()
                .title("Python Environment")
                .items(vec![
                    picker_with_path_button(
                        "python_path",
                        "Python Path",
                        "Path to Python executable",
                        "Select Python executable",
                        {
                            let mut c = exe_candidates("python");
                            if !cfg!(windows) {
                                c.push("python3");
                            }
                            c
                        },
                    ),
                    picker_with_path_button(
                        "uv_path",
                        "UV Path",
                        "Path to UV package manager (optional)",
                        "Select UV executable",
                        exe_candidates("uv"),
                    ),
                    Setting::Switch {
                        key: "use_uv",
                        label: "Use UV",
                        description: "Use UV instead of pip for running workbench",
                    }
                    .into(),
                ]),
        ]),
        SettingPage::new("Servers").default_open(true).groups(vec![
            SettingGroup::new()
                .title("Saved Servers")
                .items(vec![saved_servers_field()]),
            SettingGroup::new()
                .title("Add New Server")
                .items(vec![
                    Setting::Text {
                        key: "new_server_label",
                        label: "Label",
                        description: "Display name for this server",
                    }
                    .into(),
                    Setting::Text {
                        key: "new_server_url",
                        label: "Server URL",
                        description: "Base URL for the Islandora server",
                    }
                    .into(),
                    Setting::FilePicker {
                        key: "new_credentials_file",
                        label: "Credentials File",
                        description: "Path to credentials file for API access",
                        prompt: "Select credentials file",
                    }
                    .into(),
                    add_server_button(),
                ]),
        ]),
        SettingPage::new("Task Configs").default_open(true).groups(vec![
            SettingGroup::new()
                .title("Saved Configurations")
                .items(vec![saved_configs_field()]),
            SettingGroup::new()
                .title("Add New Configuration")
                .items(vec![
                    Setting::Text {
                        key: "new_task_label",
                        label: "Label",
                        description: "Display name for this configuration",
                    }
                    .into(),
                    Setting::Dropdown {
                        key: "new_task_name",
                        label: "Task Name",
                        description: "Workbench task to perform",
                        options: TASK_OPTIONS,
                    }
                    .into(),
                    Setting::FilePicker {
                        key: "new_task_path",
                        label: "Config File",
                        description: "Path to YAML configuration file",
                        prompt: "Select config file",
                    }
                    .into(),
                    add_config_button(),
                ]),
        ]),
    ]
}
