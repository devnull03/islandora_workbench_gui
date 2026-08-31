use gpui::*;

use window_wrapper::OpenBrowser;

use config_builder::OpenConfigBuilder;

actions!(nav, [OpenSettings, ToggleLog, Quit]);
actions!(
    help,
    [CheckForUpdates, CopyDebugInfo, OpenLogsFolder, ReportIssue]
);

pub const REPO_URL: &str = "https://github.com/devnull03/islandora_workbench_gui";

pub fn app_menus() -> Vec<Menu> {
    vec![
        Menu {
            name: "Islandora Workbench".into(),
            disabled: false,
            items: vec![
                MenuItem::action("Settings", OpenSettings),
                MenuItem::action("Config Builder", OpenConfigBuilder),
                MenuItem::action(
                    "About",
                    OpenBrowser {
                        url: "https://example.com".into(),
                    },
                ),
                MenuItem::Separator,
                MenuItem::action("Quit", Quit),
            ],
        },
        Menu {
            name: "View".into(),
            disabled: false,
            items: vec![MenuItem::action("Toggle Log", ToggleLog)],
        },
        Menu {
            name: "Github".into(),
            disabled: false,
            items: vec![
                MenuItem::submenu(Menu {
                    name: "Islandora Workbench".into(),
                    disabled: false,
                    items: vec![
                        MenuItem::action(
                            "Repository",
                            OpenBrowser {
                                url: "https://github.com/mjordan/islandora_workbench".into(),
                            },
                        ),
                        MenuItem::action(
                            "Issues",
                            OpenBrowser {
                                url: "https://github.com/mjordan/islandora_workbench/issues".into(),
                            },
                        ),
                    ],
                }),
                MenuItem::submenu(Menu {
                    name: "GUI".into(),
                    disabled: false,
                    items: vec![
                        MenuItem::action(
                            "Repository",
                            OpenBrowser {
                                url: REPO_URL.into(),
                            },
                        ),
                        MenuItem::action(
                            "Issues",
                            OpenBrowser {
                                url: format!("{REPO_URL}/issues"),
                            },
                        ),
                    ],
                }),
            ],
        },
        // The three things a bug report needs, in the order a report is assembled: read the log,
        // copy the machine details, then file it with those details already in the body.
        Menu {
            name: "Help".into(),
            disabled: false,
            items: vec![
                // Opens the releases page rather than reporting a result inline: the check that
                // runs at startup is the one that can answer without making anyone wait.
                MenuItem::action("Check for Updates", CheckForUpdates),
                MenuItem::Separator,
                MenuItem::action("Open Logs Folder", OpenLogsFolder),
                MenuItem::action("Copy Debug Info", CopyDebugInfo),
                MenuItem::action("Report an Issue", ReportIssue),
            ],
        },
    ]
}
