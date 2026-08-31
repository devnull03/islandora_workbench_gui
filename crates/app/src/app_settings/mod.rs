//! Islandora Workbench–specific settings pages (mockup `3b`).
//!
//! **Pages are named after what you are doing**, and split along one line: facts about *this
//! machine* (where Python lives) sit apart from anything *institutional* (which servers, which
//! configs), because Stage 5 bundles the second kind into a switchable profile and must not drag
//! the first kind along. See `docs/plans/stage-5-profiles.md`.
//!
//! Search is gpui-component's: `Settings` renders the field and filters through
//! `SettingItem::is_match`, which reads each item's title and description. Custom items
//! (the server and config lists) show only when the query is empty, which is the right
//! behaviour for a list that is not a single setting.

mod custom_fields;

use gpui_component::setting::{SettingGroup, SettingPage};

use settings::{Setting, picker_with_path_button};

use custom_fields::{saved_configs_field, saved_servers_field};

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

pub fn build_pages(_cx: &gpui::App) -> Vec<SettingPage> {
    vec![
        SettingPage::new("General").default_open(true).groups(vec![
            SettingGroup::new()
                .title("Appearance")
                .items(vec![crate::theming::appearance_setting()]),
            SettingGroup::new().title("Runs").items(vec![
                Setting::Switch {
                    key: "auto_accept_prompts",
                    label: "Auto-accept prompts",
                    description: "Answer Workbench's confirmation prompts automatically. \
                                  Servers marked \"always confirm\" still prompt.",
                }
                .into(),
            ]),
            // A group rather than a page of its own: one switch about the app itself belongs
            // beside the other app-level preference, not behind another tab.
            SettingGroup::new().title("Updates").items(vec![
                Setting::Switch {
                    key: crate::update_check::AUTO_UPDATE_KEY,
                    label: "Check for updates on startup",
                    description: "Asks the release feed once per launch whether a newer version \
                                  exists, and says so in the status bar. Nothing is downloaded \
                                  or installed — you choose when to upgrade.",
                }
                .into(),
            ]),
        ]),
        // Machine facts. Nothing here belongs to an institution, so nothing here moves when
        // profiles arrive.
        SettingPage::new("Workbench & Python")
            .default_open(true)
            .groups(vec![
                SettingGroup::new()
                    .title("Workbench Installation")
                    .items(vec![
                        Setting::DirPicker {
                            key: "workbench_path",
                            label: "Workbench Path",
                            description: "Path to the Islandora Workbench installation directory",
                            prompt: "Select workbench directory",
                        }
                        .into(),
                    ]),
                SettingGroup::new().title("Python Environment").items(vec![
                    picker_with_path_button(
                        "python_path",
                        "Python Path",
                        "Path to the Python executable",
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
                        "Path to the UV package manager (optional)",
                        "Select UV executable",
                        exe_candidates("uv"),
                    ),
                    Setting::Switch {
                        key: "use_uv",
                        label: "Use UV",
                        description: "Run Workbench through UV instead of Python directly",
                    }
                    .into(),
                ]),
            ]),
        SettingPage::new("Servers").default_open(true).groups(vec![
            SettingGroup::new()
                .title("Servers")
                .items(vec![saved_servers_field()]),
        ]),
        SettingPage::new("Config library")
            .default_open(true)
            .groups(vec![
                SettingGroup::new()
                    .title("Saved configurations")
                    .items(vec![saved_configs_field()]),
            ]),
        SettingPage::new("Input & preprocess")
            .default_open(true)
            .groups(vec![SettingGroup::new().title("Preprocess scripts").items(
                vec![
                    Setting::DirPicker {
                        key: "preprocess_scripts_dir",
                        label: "Scripts Folder",
                        description: "Every .py file here is offered as a preprocess script. \
                                      A script is called with --input, --output-dir and \
                                      optionally --config, and must write a CSV.",
                        prompt: "Select preprocess scripts folder",
                    }
                    .into(),
                ],
            )]),
        // Every default the config builder had to hard-code in Stage 1 becomes a setting here.
        SettingPage::new("Config builder")
            .default_open(true)
            .groups(vec![SettingGroup::new().title("Defaults").items(vec![
                    Setting::DirPicker {
                        key: "config_library_dir",
                        label: "Save new configs to",
                        description:
                            "Where \"Save to library\" writes a config that has no file yet",
                        prompt: "Select the config library folder",
                    }
                    .into(),
                    Setting::Switch {
                        key: "builder_show_yaml",
                        label: "Show the YAML panel",
                        description: "Open the builder with the read-only YAML preview visible",
                    }
                    .into(),
                ])]),
    ]
}
