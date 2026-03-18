use gpui::*;
use gpui_component::{
    TitleBar,
    button::Button,
    menu::{DropdownMenu, PopupMenu},
};

pub struct AppTitleBar {
    title: String,
    items: Vec<OwnedMenu>,
}

impl AppTitleBar {
    pub fn new(title: String, menu_items: Vec<Menu>) -> Self {
        let mut items: Vec<OwnedMenu> = vec![];
        for menu in menu_items {
            items.push(menu.owned());
        }
        Self { title, items }
    }

    pub fn with_owned(title: String, items: Vec<OwnedMenu>) -> Self {
        Self { title, items }
    }

    pub fn build(self) -> impl IntoElement {
        let mut menu_container = gpui_component::h_flex().gap_1().justify_start();

        for item in self.items {
            menu_container = menu_container.child(Self::convert_menu(item)).cursor_pointer();
        }

        TitleBar::new().child(menu_container)
    }

    fn convert_menu(menu_spec: OwnedMenu) -> impl IntoElement {
        Button::new("menu-btn")
            .label(menu_spec.name.clone())
            .dropdown_menu(move |mut menu, window, cx| {
                for item in menu_spec.items.clone() {
                    match item {
                        OwnedMenuItem::Action { name, action, .. } => {
                            menu = menu.menu(name.clone(), action.boxed_clone());
                        }
                        OwnedMenuItem::Submenu(submenu) => {
                            menu = *Self::convert_submenu(submenu, menu, window, cx);
                        }
                        OwnedMenuItem::Separator => {
                            menu = menu.separator();
                        }
                        _ => {}
                    }
                }
                menu
            })
    }

    fn convert_submenu(
        submenu_spec: OwnedMenu,
        parent_menu: PopupMenu,
        window: &mut Window,
        cx: &mut Context<'_, PopupMenu>,
    ) -> Box<PopupMenu> {
        let items = submenu_spec.items.clone();
        Box::new(parent_menu.submenu(
            submenu_spec.name.clone(),
            window,
            cx,
            move |mut submenu, window, cx| {
                for item in items.clone() {
                    match item {
                        OwnedMenuItem::Action { name, action, .. } => {
                            submenu = submenu.menu(name.clone(), action.boxed_clone());
                        }
                        OwnedMenuItem::Submenu(sub_submenu) => {
                            submenu = *Self::convert_submenu(sub_submenu, submenu, window, cx);
                        }
                        OwnedMenuItem::Separator => {
                            submenu = submenu.separator();
                        }
                        _ => {}
                    }
                }
                submenu
            },
        ))
    }
}
