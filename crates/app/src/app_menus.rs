use gpui::*;

use window_wrapper::OpenBrowser;

use config_builder::OpenConfigBuilder;

actions!(nav, [OpenSettings, Quit]);

pub fn app_menus() -> Vec<Menu> {
    vec![
        Menu {
            name: "Islandora Workbench".into(),
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
            name: "Github".into(),
            items: vec![
                MenuItem::submenu(Menu {
                    name: "Islandora Workbench".into(),
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
                    items: vec![
                        MenuItem::action(
                            "Repository",
                            OpenBrowser {
                                url: "https://github.com/devnull03/islandora_workbench_gui".into(),
                            },
                        ),
                        MenuItem::action(
                            "Issues",
                            OpenBrowser {
                                url: "https://github.com/devnull03/islandora_workbench_gui/issues"
                                    .into(),
                            },
                        ),
                    ],
                }),
            ],
        },
    ]
}
