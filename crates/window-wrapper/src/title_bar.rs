//! The title bar: registered components on the left and right, the window title in the centre,
//! and the run indicator beside the window controls.
//!
//! The three-region layout and the [`TitleBarRegistry`] are ported from the qrate
//! window-wrapper, so anything in the app can drop a view into the bar without this crate
//! knowing about it.

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{ActiveTheme, IconName, Sizable, TitleBar, label::Label, spinner::Spinner};

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
}

impl AppTitleBar {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
        }
    }
}

impl RenderOnce for AppTitleBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let locked = WindowLock::is_locked(cx);
        let muted = cx.theme().muted_foreground;

        let views = |items: &Vec<crate::bar::BarItem>| {
            items
                .iter()
                .filter(|item| item.occupied(cx))
                .map(|item| item.view.clone())
                .collect::<Vec<_>>()
        };
        let (left, right) = cx
            .try_global::<TitleBarRegistry>()
            .map(|registry| {
                (
                    views(&registry.items().left),
                    views(&registry.items().right),
                )
            })
            .unwrap_or_default();

        TitleBar::new()
            .text_xs()
            .text_color(cx.theme().foreground)
            .child(
                gpui_component::h_flex()
                    .flex_1()
                    .gap_1()
                    .justify_start()
                    .children(left),
            )
            .child(
                gpui_component::h_flex()
                    .justify_center()
                    .items_center()
                    .gap_1p5()
                    .when(!self.title.is_empty(), |this| {
                        this.child(Label::new(self.title.clone()).text_xs())
                    }),
            )
            .child(
                gpui_component::h_flex()
                    .flex_1()
                    .justify_end()
                    .pr_4()
                    .gap_1()
                    .items_center()
                    .children(right)
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
