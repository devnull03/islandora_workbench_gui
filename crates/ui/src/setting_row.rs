//! One setting on one line, to Component Spec §03: a fixed label column, the control and whatever
//! the control has to say about itself, and — in the builder only — the value's shape.
//!
//! The label column is a fixed [`LABEL_COL_W`] rather than sized to content so labels line up down
//! a whole page regardless of what control sits beside them; a page of 140 settings with a ragged
//! left edge is unreadable. Long keys wrap rather than widening it. [`crate::LabeledField`] is the
//! vertical counterpart, still right inside a narrow column where 180px of label would leave
//! nothing for the control.
//!
//! Two §00 rulings the mockups disagree with, resolved here:
//!
//! * The column is **180px**, not the builder mockup's 210. One width, both windows.
//! * An error shows on the **control**, never as a tinted row. A coloured band reads as "this
//!   whole area is broken" when what is broken is one value, and it collides with the zebra that
//!   §00 reserves for row-editor tables.

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{ActiveTheme, StyledExt as _, h_flex, label::Label, v_flex};

use crate::InlineMessage;
use crate::tokens::{GAP_MD, GAP_XL, GAP_XS, LABEL_COL_W, ROW_PAD_Y, TYPE_COL_W};

#[derive(IntoElement)]
pub struct SettingRow {
    label: SharedString,
    description: Option<SharedString>,
    /// The label is the literal YAML key rather than prose, so it is set in the mono family
    /// (§03). True in the builder, false in the settings window.
    mono_label: bool,
    /// Draws §03's `required` badge. Only `task` is, in the whole schema.
    required: bool,
    /// The value's shape, in the trailing column. Builder only — §00 calls this the one
    /// legitimate difference between the two windows.
    type_badge: Option<SharedString>,
    note: Option<InlineMessage>,
    /// Differs from its default — draws the accent bar down the label column.
    modified: bool,
    /// A switch or checkbox is a single short row, so the label centres against it instead of
    /// sitting at its top (§05).
    align_center: bool,
    control: AnyElement,
    /// Actions that belong to the control column's right edge: a reset, a remove.
    trailing: Vec<AnyElement>,
}

impl SettingRow {
    pub fn new(label: impl Into<SharedString>, control: impl IntoElement) -> Self {
        Self {
            label: label.into(),
            description: None,
            mono_label: false,
            required: false,
            type_badge: None,
            note: None,
            modified: false,
            align_center: false,
            control: control.into_any_element(),
            trailing: Vec::new(),
        }
    }

    /// Optional, and most rows omit it — a row must not grow taller than it needs.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn mono_label(mut self, mono: bool) -> Self {
        self.mono_label = mono;
        self
    }

    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    pub fn type_badge(mut self, badge: impl Into<SharedString>) -> Self {
        self.type_badge = Some(badge.into());
        self
    }

    pub fn note(mut self, note: impl Into<Option<InlineMessage>>) -> Self {
        self.note = note.into();
        self
    }

    pub fn modified(mut self, modified: bool) -> Self {
        self.modified = modified;
        self
    }

    pub fn align_center(mut self, center: bool) -> Self {
        self.align_center = center;
        self
    }
}

/// Children are the control column's trailing actions; the control itself is
/// [`SettingRow::new`]'s second argument.
impl ParentElement for SettingRow {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.trailing.extend(elements);
    }
}

impl RenderOnce for SettingRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors;
        let muted = colors.muted_foreground;
        let mono = cx.theme().mono_font_family.clone();
        let align_center = self.align_center;

        h_flex()
            .w_full()
            .py(ROW_PAD_Y)
            .gap(GAP_XL)
            .map(|row| {
                if align_center {
                    row.items_center()
                } else {
                    row.items_start()
                }
            })
            .child(
                h_flex()
                    .w(LABEL_COL_W)
                    .flex_none()
                    .items_start()
                    // The 2px accent bar marks a setting that differs from its default; it pairs
                    // with the per-row reset affordance rather than replacing it.
                    .child(
                        div()
                            .w(px(2.))
                            .self_stretch()
                            .when(self.modified, |el| el.bg(colors.primary)),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w(px(0.))
                            .gap(GAP_XS)
                            .child(
                                h_flex()
                                    .w_full()
                                    .gap(GAP_MD)
                                    .items_baseline()
                                    .child(
                                        Label::new(self.label)
                                            .text_sm()
                                            .when_some(self.mono_label.then_some(mono), |el, f| {
                                                el.font_family(f)
                                            }),
                                    )
                                    .when(self.required, |el| {
                                        el.child(
                                            Label::new("required")
                                                .text_xs()
                                                .font_semibold()
                                                .text_color(colors.primary),
                                        )
                                    }),
                            )
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
                    .child(
                        h_flex()
                            .w_full()
                            .gap(GAP_MD)
                            .items_center()
                            .child(div().flex_1().min_w(px(0.)).child(self.control))
                            .children(self.trailing),
                    )
                    .children(self.note),
            )
            .when_some(self.type_badge, |row, badge| {
                row.child(
                    div().w(TYPE_COL_W).flex_none().child(
                        Label::new(badge)
                            .text_xs()
                            // Faint rather than muted: it is a fact about the row you read once
                            // and then stop seeing, not something to scan for.
                            .text_color(muted.opacity(0.6))
                            .text_right(),
                    ),
                )
            })
    }
}
