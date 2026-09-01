//! The one line a control is allowed to say about its own value.
//!
//! Component Spec §03. Three properties are load-bearing:
//!
//! * It sits **under the control**, never in a tooltip. A message you have to hover to find is a
//!   message nobody reads before pressing Save.
//! * The glyph gets a **fixed [`GLYPH_COL_W`] column**, so a warning's text and an error's text
//!   start at the same x down a whole page. Without it every row's prose is indented differently
//!   depending on which glyph it drew.
//! * **One message per control, worst level wins.** A stack of five is a wall.
//!
//! Level lives in the glyph and the text colour and nowhere else — §00 rules out tinting the row
//! background, which two of the mockups do. A coloured band reads as "this whole area is broken"
//! when what is broken is one value.

use gpui::*;
use gpui_component::{ActiveTheme, h_flex, label::Label};

use crate::tokens::{GAP_SM, GLYPH_COL_W};

/// What the message is telling you, and therefore whether it blocks a save.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessageLevel {
    /// Will not parse, or the path does not exist. Blocks Save.
    ///
    /// Ordered first so `min()` over a row's problems picks the worst one.
    Error,
    /// Runs, but probably is not what the user meant. Never blocks Save — §06 is explicit that
    /// only errors disable the primary action.
    Warning,
    /// Confirmation, not praise: the resolved path, the term the URL matched. Facts only.
    Info,
}

#[derive(Clone, IntoElement)]
pub struct InlineMessage {
    level: MessageLevel,
    text: SharedString,
}

impl InlineMessage {
    pub fn new(level: MessageLevel, text: impl Into<SharedString>) -> Self {
        Self {
            level,
            text: text.into(),
        }
    }

    pub fn error(text: impl Into<SharedString>) -> Self {
        Self::new(MessageLevel::Error, text)
    }

    pub fn warning(text: impl Into<SharedString>) -> Self {
        Self::new(MessageLevel::Warning, text)
    }

    pub fn info(text: impl Into<SharedString>) -> Self {
        Self::new(MessageLevel::Info, text)
    }

    pub fn level(&self) -> MessageLevel {
        self.level
    }
}

impl RenderOnce for InlineMessage {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors;
        // The glyph carries the level at full strength; the prose is a step back from it, because
        // a whole sentence in danger red is harder to read than the same sentence in body text.
        let (glyph, glyph_color, text_color) = match self.level {
            MessageLevel::Error => ("✕", colors.danger, colors.danger),
            MessageLevel::Warning => ("!", colors.warning, colors.warning),
            MessageLevel::Info => ("✓", colors.success, colors.muted_foreground),
        };

        h_flex()
            .w_full()
            .gap(GAP_SM)
            .items_start()
            .child(
                div()
                    .w(GLYPH_COL_W)
                    .flex_none()
                    .child(Label::new(glyph).text_xs().text_color(glyph_color)),
            )
            .child(
                Label::new(self.text)
                    .text_xs()
                    .text_color(text_color)
                    .flex_1()
                    .min_w(px(0.)),
            )
    }
}
