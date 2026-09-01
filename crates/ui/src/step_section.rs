//! A numbered step in a top-to-bottom workflow: `1  Input source          optional step`.
//!
//! Component Spec §09. The main window's three steps have this exact anatomy, so the shape lives
//! here rather than being spelled out once per step.
//!
//! The index is mono and muted while the title is semibold body text, because the number is a
//! position and the title is the thing. Reversing that — a bold number beside a plain title —
//! makes a form look like a countdown.

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{ActiveTheme, StyledExt, h_flex, label::Label, v_flex};

use crate::tokens::{GAP_LG, GAP_MD};

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
        let muted = cx.theme().colors.muted_foreground;
        let mono = cx.theme().mono_font_family.clone();

        v_flex()
            .w_full()
            .flex_shrink_0()
            .gap(GAP_LG)
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap(GAP_MD)
                    .child(
                        Label::new(self.number)
                            .text_xs()
                            .font_family(mono)
                            .text_color(muted),
                    )
                    .child(Label::new(self.title).text_sm().font_semibold())
                    .when_some(self.note, |el, note| {
                        el.child(div().flex_1())
                            .child(Label::new(note).text_xs().text_color(muted))
                    }),
            )
            .children(self.children)
    }
}
