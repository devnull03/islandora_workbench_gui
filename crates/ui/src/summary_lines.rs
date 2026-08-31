//! A row's collapsed identity: a name, then the muted detail lines under it.
//!
//! Both the settings lists and the secondary-config chain draw this. It ellipsises rather than
//! wraps because the detail is usually a path, and a wrapped path costs a row of height on every
//! entry in a long list.

use gpui::*;
use gpui_component::{ActiveTheme, StyledExt, label::Label, v_flex};

#[derive(IntoElement)]
pub struct SummaryLines {
    title: SharedString,
    lines: Vec<SharedString>,
}

impl SummaryLines {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            lines: Vec::new(),
        }
    }

    pub fn line(mut self, line: impl Into<SharedString>) -> Self {
        self.lines.push(line.into());
        self
    }

    pub fn lines(mut self, lines: impl IntoIterator<Item = SharedString>) -> Self {
        self.lines.extend(lines);
        self
    }
}

impl RenderOnce for SummaryLines {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let muted = cx.theme().colors.muted_foreground;
        v_flex()
            // The zero min-width is what lets the ellipsis happen at all inside a flex row.
            .flex_1()
            .min_w(px(0.))
            .overflow_hidden()
            .child(Label::new(self.title).font_semibold())
            .children(self.lines.into_iter().map(move |line| {
                div()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(Label::new(line).text_xs().text_color(muted))
            }))
    }
}
