//! The title bar: menus on the left, the window title in the centre, registered components and
//! the run indicator on the right.
//!
//! The three-region layout and the [`TitleBarRegistry`] are ported from the qrate
//! window-wrapper, so anything in the app can drop a view into the bar without this crate
//! knowing about it.

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme, IconName, Sizable, TitleBar,
    button::{Button, ButtonVariants},
    label::Label,
    menu::{DropdownMenu, PopupMenu},
    spinner::Spinner,
};

use crate::WindowLock;
use crate::bar::{BarItems, BarRegistry};

#[derive(Default)]
pub struct TitleBarRegistry(BarItems);

impl Global for TitleBarRegistry {}

impl BarRegistry for TitleBarRegistry {
    fn items(&self) -> &BarItems {
        &self.0
    }
    fn items_mut(&mut self) -> &mut BarItems {
        &mut self.0
    }
}

#[derive(IntoElement)]
pub struct AppTitleBar {
    title: SharedString,
    items: Vec<OwnedMenu>,
}

impl AppTitleBar {
    pub fn new(cx: &App) -> Self {
        let menu_items = cx.get_menus().unwrap_or_default();
        // let mut items: Vec<OwnedMenu> = vec![];
        // for menu in menu_items.into_iter() {
        //     items.push(menu.owned());
        // }
        Self {
            items: menu_items,
            title: SharedString::default(),
        }
    }

    pub fn with_owned(items: Vec<OwnedMenu>) -> Self {
        Self {
            items,
            title: SharedString::default(),
        }
    }

    fn convert_menu(menu_spec: OwnedMenu) -> impl IntoElement {
        let button_id: SharedString = format!("menu-btn-{}", menu_spec.name).into();
        Button::new(button_id)
            .small()
            .ghost()
            .compact()
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

impl AppTitleBar {
    /// The window title, shown centred. Empty by default so secondary windows can stay bare.
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = title.into();
        self
    }
}

impl RenderOnce for AppTitleBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let locked = WindowLock::is_locked(cx);
        let muted = cx.theme().muted_foreground;

        let mut menus = gpui_component::h_flex().flex_1().gap_1().justify_start();
        for item in self.items.clone() {
            menus = menus.child(Self::convert_menu(item)).cursor_pointer();
        }

        let registered = cx
            .try_global::<TitleBarRegistry>()
            .map(|r| {
                r.items()
                    .right
                    .iter()
                    .filter(|item| item.occupied(cx))
                    .map(|item| item.view.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        TitleBar::new()
            .text_xs()
            .text_color(cx.theme().foreground)
            .child(menus)
            .child(
                gpui_component::h_flex()
                    .justify_center()
                    .items_center()
                    .gap_1p5()
                    .when(!self.title.is_empty(), |this| {
                        this.child(Label::new(self.title.clone()).text_xs().text_color(muted))
                    }),
            )
            .child(
                gpui_component::h_flex()
                    .flex_1()
                    .justify_end()
                    .pr_4()
                    .gap_1()
                    .items_center()
                    .children(registered)
                    // The run indicator is not a registered item: it belongs to the window lock,
                    // which this crate owns, and it must always sit closest to the controls.
                    .when(locked, |right| {
                        right
                            .child(
                                Spinner::new()
                                    .small()
                                    .icon(IconName::LoaderCircle)
                                    .color(muted),
                            )
                            .child(Label::new("Ingest running").text_xs().text_color(muted))
                    }),
            )
    }
}
