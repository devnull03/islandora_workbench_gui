use gpui::*;

use window_wrapper::{
    OpenBrowser, 
    // Menu,
    // MenuItem,
};

pub fn app_menus() -> Vec<Menu> {
    vec![
        Menu {
            name: "App".into(),
            items: vec![
                MenuItem::action("About", OpenBrowser { url: "https://example.com".into() }),
                MenuItem::separator(),
                MenuItem::submenu(Menu {
                    name: "Github".into(),
                    items: vec![
                        MenuItem::action("Repository", OpenBrowser { url: "https://github.com/zed-industries/zed".into() }),
                        MenuItem::action("Issues", OpenBrowser { url: "https://github.com/zed-industries/zed/issues".into() }), 
                    ]
                })
            ],
        }
        // Menu {
        //     label: "File",
        //     items: vec![
        //         MenuItem::action("New File", workspace::actions::NewFile),
        //         MenuItem::action("Open…", workspace::actions::Open),
        //     ],
        // },
    ]
}