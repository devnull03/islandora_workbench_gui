//! The settings the app fills in for you, stated as facts.
//!
//! Component Spec §08. An eyebrow and an aside, then one pill per key: the key in mono, and where
//! its value comes from in prose.
//!
//! The spec is emphatic on one point and it is the reason this is not just a disabled row group:
//! *"Not interactive, not focusable, no disabled styling — these are facts, not disabled
//! controls."* A greyed-out input invites you to work out how to un-grey it. A pill does not.

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{ActiveTheme, h_flex, label::Label, v_flex};

use crate::card::CardTone;
use crate::tokens::{GAP_MD, GAP_SM};
use crate::{Card, app_tag};

#[derive(IntoElement)]
pub struct LockedBand {
    title: SharedString,
    aside: SharedString,
    /// `(key, where its value comes from)`.
    entries: Vec<(SharedString, SharedString)>,
}

impl LockedBand {
    pub fn new(title: impl Into<SharedString>, aside: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            aside: aside.into(),
            entries: Vec::new(),
        }
    }

    pub fn entry(mut self, key: impl Into<SharedString>, source: impl Into<SharedString>) -> Self {
        self.entries.push((key.into(), source.into()));
        self
    }
}

impl RenderOnce for LockedBand {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors;
        let muted = colors.muted_foreground;
        let mono = cx.theme().mono_font_family.clone();

        Card::new()
            .tone(CardTone::Filled)
            .gap(GAP_SM)
            .child(
                h_flex()
                    .w_full()
                    .gap(GAP_MD)
                    .items_baseline()
                    .child(
                        Label::new(self.title.to_uppercase())
                            .text_xs()
                            .font_family(mono.clone())
                            .text_color(muted),
                    )
                    .child(Label::new(self.aside).text_xs().text_color(muted)),
            )
            .child(h_flex().w_full().gap(GAP_SM).flex_wrap().children(
                self.entries.into_iter().map(|(key, source)| {
                    app_tag()
                        .py(px(4.))
                        .gap(GAP_SM)
                        .items_baseline()
                        .flex_none()
                        .bg(colors.background)
                        .child(Label::new(key).text_xs().font_family(mono.clone()))
                        .child(Label::new(source).text_xs().text_color(muted))
                }),
            ))
    }
}

/// The problem and warning counters that head the builder's footer.
///
/// §09, on the copy and on the counting: the words are "N problems to fix" and "N things to
/// know" — never "errors" and "warnings", which name the machine's categories rather than the
/// reader's job. A zero-count counter is **omitted**, not rendered as `0`; "0 problems to fix" is
/// a sentence that makes you stop and read it to learn nothing.
#[derive(IntoElement)]
pub struct ProblemSummary {
    errors: usize,
    warnings: usize,
}

impl ProblemSummary {
    pub fn new(errors: usize, warnings: usize) -> Self {
        Self { errors, warnings }
    }
}

impl RenderOnce for ProblemSummary {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors;

        let counter = |glyph: &'static str, color: Hsla, text: String| {
            h_flex()
                .gap(GAP_SM)
                .items_center()
                .child(Label::new(glyph).text_xs().text_color(color))
                .child(Label::new(text).text_sm().text_color(color))
        };

        v_flex().child(
            h_flex()
                .gap(px(14.))
                .items_center()
                // Both counters empty is itself worth one line: silence would read as "the
                // validator has not run yet".
                .when(self.errors == 0 && self.warnings == 0, |el| {
                    el.child(counter(
                        "✓",
                        colors.muted_foreground,
                        "No problems".to_string(),
                    ))
                })
                .when(self.errors > 0, |el| {
                    el.child(counter(
                        "✕",
                        colors.danger,
                        format!("{} problem{} to fix", self.errors, plural(self.errors)),
                    ))
                })
                .when(self.warnings > 0, |el| {
                    el.child(counter(
                        "!",
                        colors.warning,
                        format!("{} thing{} to know", self.warnings, plural(self.warnings)),
                    ))
                }),
        )
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}
