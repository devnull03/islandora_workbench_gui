//! A numbered step in a top-to-bottom workflow: `1  Input source          optional step`.
//!
//! The main window's three steps (mockup `2c`) all have this exact anatomy, so the shape lives
//! here rather than being spelled out once per step.

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{ActiveTheme, StyledExt, h_flex, label::Label, v_flex};

#[derive(IntoElement)]
pub struct StepSection {
    number: SharedString,
    title: SharedString,
    /// Right-aligned muted text on the header row, e.g. `optional step`.
    note: Option<SharedString>,
    children: Vec<AnyElement>,
}

impl StepSection {
    pub fn new(number: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        Self {
            number: number.into(),
            title: title.into(),
            note: None,
            children: Vec::new(),
        }
    }

    pub fn note(mut self, note: impl Into<SharedString>) -> Self {
        self.note = Some(note.into());
        self
    }
}

impl ParentElement for StepSection {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for StepSection {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        v_flex()
            .w_full()
            .flex_shrink_0()
            .gap_2()
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .child(Label::new(self.number).text_xs().text_color(muted))
                    .child(Label::new(self.title).text_sm().font_semibold())
                    .when_some(self.note, |el, note| {
                        el.child(div().flex_1())
                            .child(Label::new(note).text_xs().text_color(muted))
                    }),
            )
            .children(self.children)
    }
}
