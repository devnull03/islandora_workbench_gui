//! One setting on one line: a fixed label column, then the control and whatever the control has
//! to say about itself.
//!
//! The label column is a fixed [`LABEL_COL_W`] rather than sized to content so that labels line up
//! down a whole page regardless of what control sits beside them — a page of 140 settings with a
//! ragged left edge is unreadable. [`crate::LabeledField`] is the vertical counterpart, still
//! right inside a narrow column where 180px of label would leave nothing for the control.
//!
//! The validation message lives under the control, not in a tooltip: a message you have to hover
//! to find is a message the user does not read before hitting Save.

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{ActiveTheme, h_flex, label::Label, v_flex};

use crate::tokens::{GAP_XL, GAP_XS, LABEL_COL_W};

/// What the row has to say about its current value. Error blocks the save; warning does not.
#[derive(Clone)]
pub enum FieldNote {
    /// This will not parse, or the path does not exist.
    Error(SharedString),
    /// This runs, but probably is not what you meant.
    Warning(SharedString),
}

#[derive(IntoElement)]
pub struct SettingRow {
    label: SharedString,
    description: Option<SharedString>,
    note: Option<FieldNote>,
    /// Differs from its default — draws the accent bar down the label column.
    modified: bool,
    control: AnyElement,
    /// Right-aligned actions and tags: a type label, a remove glyph, a reset.
    trailing: Vec<AnyElement>,
}

impl SettingRow {
    pub fn new(label: impl Into<SharedString>, control: impl IntoElement) -> Self {
        Self {
            label: label.into(),
            description: None,
            note: None,
            modified: false,
            control: control.into_any_element(),
            trailing: Vec::new(),
        }
    }

    /// Optional, and most rows omit it — a row must not grow taller than it needs.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn note(mut self, note: impl Into<Option<FieldNote>>) -> Self {
        self.note = note.into();
        self
    }

    pub fn modified(mut self, modified: bool) -> Self {
        self.modified = modified;
        self
    }
}

/// Children are the trailing actions; the control is [`SettingRow::new`]s second argument.
impl ParentElement for SettingRow {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.trailing.extend(elements);
    }
}

impl RenderOnce for SettingRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = &cx.theme().colors;
        let muted = colors.muted_foreground;
        let accent = colors.primary;
        let (note_text, note_color) = match &self.note {
            Some(FieldNote::Error(msg)) => (Some(msg.clone()), colors.danger),
            Some(FieldNote::Warning(msg)) => (Some(msg.clone()), colors.warning),
            None => (None, muted),
        };

        h_flex()
            .w_full()
            .gap(GAP_XL)
            .items_start()
            .child(
                h_flex()
                    .w(LABEL_COL_W)
                    .flex_none()
                    .items_start()
                    // The 2px accent bar marks a setting that differs from its default; it pairs
                    // with the per-item reset affordance rather than replacing it.
                    .child(
                        div()
                            .w(px(2.))
                            .self_stretch()
                            .when(self.modified, |el| el.bg(accent)),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w(px(0.))
                            .gap(GAP_XS)
                            .child(Label::new(self.label).text_sm())
                            .when_some(self.description, |el, description| {
                                el.child(Label::new(description).text_xs().text_color(muted))
                            }),
                    ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w(px(0.))
                    .gap(px(5.))
                    .child(self.control)
                    .when_some(note_text, |el, msg| {
                        el.child(Label::new(msg).text_xs().text_color(note_color))
                    }),
            )
            .children(self.trailing)
    }
}
