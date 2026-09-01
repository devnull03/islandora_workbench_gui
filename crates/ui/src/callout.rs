//! A note about a whole group, as opposed to [`crate::InlineMessage`], which is about one value.
//!
//! Component Spec §03. Two rules, both of which exist to stop this becoming a banner:
//!
//! * **Never tinted by level.** Only the glyph carries the level. A tinted box competes with the
//!   controls it is explaining, and there is nowhere left to go when something is actually wrong.
//! * **At most one per group.** A page with three callouts is a page with a structure problem,
//!   and the third one is not read.
//!
//! It sits at the *end* of the group it explains, not the top: you read the settings, then the
//! caveat about them.

use gpui::*;
use gpui_component::{ActiveTheme, h_flex, label::Label};

use crate::MessageLevel;
use crate::tokens::{GAP_MD, GAP_SM, GLYPH_COL_W};

#[derive(IntoElement)]
pub struct Callout {
    level: MessageLevel,
    text: SharedString,
}

impl Callout {
    pub fn new(level: MessageLevel, text: impl Into<SharedString>) -> Self {
        Self {
            level,
            text: text.into(),
        }
    }

    pub fn warning(text: impl Into<SharedString>) -> Self {
        Self::new(MessageLevel::Warning, text)
    }

    pub fn info(text: impl Into<SharedString>) -> Self {
        Self::new(MessageLevel::Info, text)
    }
}

impl RenderOnce for Callout {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors;
        let (glyph, glyph_color) = match self.level {
            MessageLevel::Error => ("✕", colors.danger),
            MessageLevel::Warning => ("!", colors.warning),
            MessageLevel::Info => ("✓", colors.success),
        };

        h_flex()
            .w_full()
            .px(GAP_MD)
            .py(GAP_SM)
            .gap(GAP_MD)
            .items_start()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(colors.border)
            .bg(colors.table_head)
            .child(
                div()
                    .w(GLYPH_COL_W)
                    .flex_none()
                    .child(Label::new(glyph).text_xs().text_color(glyph_color)),
            )
            .child(
                Label::new(self.text)
                    .text_xs()
                    .text_color(colors.muted_foreground)
                    .flex_1()
                    .min_w(px(0.)),
            )
    }
}
