

pub fn app_menus(cx: &AppContext) -> Vec<Menu> {
    vec![
        Menu {
            label: "Zed",
            items: vec![
                MenuItem::action("About Zed…", zed_actions::About),
                MenuItem::separator(),
                MenuItem::action("Settings", zed_actions::OpenSettings),
            ],
        },
        Menu {
            label: "File",
            items: vec![
                MenuItem::action("New File", workspace::actions::NewFile),
                MenuItem::action("Open…", workspace::actions::Open),
            ],
        },
    ]
}


