//! Islandora Workbench–specific settings pages and custom setting fields.

mod custom_fields;

use gpui_component::setting::{SettingGroup, SettingPage};

use settings::{Setting, picker_with_path_button};

use custom_fields::{
    TASK_OPTIONS, add_config_button, add_server_button, saved_configs_field, saved_servers_field,
};

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

pub fn build_pages() -> Vec<SettingPage> {
    vec![
        SettingPage::new("Paths").default_open(true).groups(vec![
            SettingGroup::new()
                .title("Workbench Installation")
                .items(vec![
                    Setting::DirPicker {
                        key: "workbench_path",
                        label: "Workbench Path",
                        description: "Path to Islandora Workbench installation directory",
                        prompt: "Select workbench directory",
                    }
                    .into(),
                ]),
            SettingGroup::new().title("Python Environment").items(vec![
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
            SettingGroup::new().title("Add New Server").items(vec![
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
        SettingPage::new("Task Configs")
            .default_open(true)
            .groups(vec![
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
