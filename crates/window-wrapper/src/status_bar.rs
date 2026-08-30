//! The status bar: three regions of registered components, with rules drawn between neighbours.
//!
//! Ported from the qrate window-wrapper. The occupancy question in [`crate::bar::BarItem`] is
//! what makes the dividers work — an item that renders to nothing would otherwise strand a rule
//! next to empty space.

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{ActiveTheme as _, divider::Divider, h_flex};

use crate::bar::{BarItem, BarItems, BarRegistry};

#[derive(Default)]
pub struct StatusBarRegistry(BarItems);

impl Global for StatusBarRegistry {}

impl BarRegistry for StatusBarRegistry {
    fn items(&self) -> &BarItems {
        &self.0
    }
    fn items_mut(&mut self) -> &mut BarItems {
        &mut self.0
    }
}

impl StatusBarRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_left(&mut self, view: impl Into<AnyView>) {
        self.0.add_left(view);
    }

    pub fn add_right(&mut self, view: impl Into<AnyView>) {
        self.0.add_right(view);
    }
}

pub struct StatusBar;

impl Default for StatusBar {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusBar {
    pub fn new() -> Self {
        Self
    }
}

impl Render for StatusBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let group = |items: &Vec<BarItem>, cx: &App| {
            let mut children: Vec<AnyElement> = Vec::new();
            for item in items.iter().filter(|item| item.occupied(cx)) {
                if !children.is_empty() {
                    children.push(Divider::vertical().h_3().into_any_element());
                }
                children.push(item.view.clone().into_any_element());
            }
            h_flex().gap_3().items_center().children(children)
        };

        let items = cx.try_global::<StatusBarRegistry>().map(|r| r.items());
        let occupied = |side: fn(&BarItems) -> &Vec<BarItem>| {
            items.is_some_and(|items| side(items).iter().any(|item| item.occupied(cx)))
        };
        let (left, centre, right) = match items {
            Some(items) => (
                group(&items.left, cx),
                group(&items.centre, cx),
                group(&items.right, cx),
            ),
            None => (h_flex(), h_flex(), h_flex()),
        };

        h_flex()
            .id("status-bar")
            .w_full()
            .flex_shrink_0()
            .px_3()
            .py_1()
            .gap_3()
            .items_center()
            .justify_between()
            .bg(cx.theme().title_bar)
            .text_xs()
            .text_color(cx.theme().foreground)
            .border_t_1()
            .border_color(cx.theme().title_bar_border)
            .child(left)
            .child(
                h_flex()
                    .flex_1()
                    .gap_3()
                    .items_center()
                    .when(
                        occupied(|items| &items.left) && occupied(|items| &items.centre),
                        |row| row.child(Divider::vertical().h_3()),
                    )
                    .child(centre),
            )
            .child(right)
    }
}
