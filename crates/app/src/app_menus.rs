use gpui::*;

use window_wrapper::OpenBrowser;

use config_builder::OpenConfigBuilder;

actions!(nav, [OpenSettings, ToggleLog, Quit]);
actions!(help, [CopyDebugInfo, OpenLogsFolder, ReportIssue]);

pub const REPO_URL: &str = "https://github.com/devnull03/islandora_workbench_gui";

pub fn app_menus() -> Vec<Menu> {
    vec![
        Menu {
            name: "Islandora Workbench".into(),
            disabled: false,
            items: vec![
                MenuItem::action("Settings", OpenSettings),
                MenuItem::action("Config Builder", OpenConfigBuilder),
                MenuItem::action("Toggle Log", ToggleLog),
                MenuItem::Separator,
                MenuItem::action("Quit", Quit),
            ],
        },
        Menu {
            name: "Help".into(),
            disabled: false,
            items: vec![
                MenuItem::action(
                    "Workbench Repository",
                    OpenBrowser {
                        url: "https://github.com/mjordan/islandora_workbench".into(),
                    },
                ),
                MenuItem::action(
                    "GUI Repository",
                    OpenBrowser {
                        url: REPO_URL.into(),
                    },
                ),
                MenuItem::Separator,
                MenuItem::action("Open Logs Folder", OpenLogsFolder),
                MenuItem::action("Copy Debug Info", CopyDebugInfo),
                MenuItem::action("Report an Issue", ReportIssue),
            ],
        },
    ]
}
