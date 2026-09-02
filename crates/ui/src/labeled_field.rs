//! A label above an arbitrary control, with an optional help line under it.
//!
//! [`crate::LabeledSelect`] is the same idea specialised to one widget; this is the version for
//! everything else (inputs, browse rows, button clusters).

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{ActiveTheme, label::Label, v_flex};

#[derive(IntoElement)]
pub struct LabeledField {
    label: SharedString,
    description: Option<SharedString>,
    children: Vec<AnyElement>,
}

impl LabeledField {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            description: None,
            children: Vec::new(),
        }
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }
}

impl ParentElement for LabeledField {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for LabeledField {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        v_flex()
            .w_full()
            .min_w(px(0.))
            .gap_1()
            .child(Label::new(self.label).text_sm())
            .children(self.children)
            .when_some(self.description, |el, description| {
                el.child(Label::new(description).text_xs().text_color(muted))
            })
    }
}
