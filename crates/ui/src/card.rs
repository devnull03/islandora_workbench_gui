//! The bordered box everything in this app groups content into.
//!
//! Eight call sites wrote out the same `p_2 / rounded(radius) / border_1 / border_color(border)`
//! stack, three of them with a `secondary` fill and one with a warning border. That is enough
//! repetition that a drifted padding value in one of them would read as a deliberate difference.
//!
//! Separation here comes from the 1px rule and the surface step, never from a shadow — the theme
//! ships `shadow: false` on purpose.

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{ActiveTheme, v_flex};

/// What the border and fill say about the card's contents.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum CardTone {
    /// A neutral container: border only, page background showing through.
    #[default]
    Plain,
    /// A row or field group that should read as inset from the page.
    Filled,
    /// Runs, but probably is not what the user meant.
    Warning,
    /// Will not save.
    Danger,
}

#[derive(IntoElement)]
pub struct Card {
    tone: CardTone,
    padding: Pixels,
    gap: Pixels,
    children: Vec<AnyElement>,
}

impl Card {
    pub fn new() -> Self {
        Self {
            tone: CardTone::Plain,
            padding: px(8.),
            gap: px(8.),
            children: Vec::new(),
        }
    }

    pub fn tone(mut self, tone: CardTone) -> Self {
        self.tone = tone;
        self
    }

    pub fn padding(mut self, padding: Pixels) -> Self {
        self.padding = padding;
        self
    }

    pub fn gap(mut self, gap: Pixels) -> Self {
        self.gap = gap;
        self
    }
}

impl Default for Card {
    fn default() -> Self {
        Self::new()
    }
}

impl ParentElement for Card {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Card {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = &cx.theme().colors;
        let border = match self.tone {
            CardTone::Plain | CardTone::Filled => colors.border,
            CardTone::Warning => colors.warning,
            CardTone::Danger => colors.danger,
        };

        v_flex()
            .w_full()
            .min_w(px(0.))
            .gap(self.gap)
            .p(self.padding)
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(border)
            .when(self.tone == CardTone::Filled, |el| el.bg(colors.secondary))
            .children(self.children)
    }
}
