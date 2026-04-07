use gpui_component::setting::{SettingGroup, SettingPage};

use super::Setting;
use super::custom_fields::{
    add_config_button, add_server_button, saved_configs_field, saved_servers_field, TASK_OPTIONS,
};

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
                    Setting::FilePicker {
                        key: "python_path",
                        label: "Python Path",
                        description: "Path to Python executable",
                        prompt: "Select Python executable",
                    }
                    .into(),
                    Setting::FilePicker {
                        key: "uv_path",
                        label: "UV Path",
                        description: "Path to UV package manager (optional)",
                        prompt: "Select UV executable",
                    }
                    .into(),
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
