//! Read-only YAML preview (mockup `1b`): the file exactly as it will be written, line
//! numbered, with the lines carrying a problem marked.
//!
//! It is a preview, not a second editor. One editable representation of the config is enough;
//! two that can disagree is a bug generator.

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme, StyledExt, h_flex, label::Label, scroll::ScrollableElement, v_flex,
};
use workbench_integration::validate::Severity;

use super::ConfigBuilder;

impl ConfigBuilder {
    pub(super) fn render_yaml_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let yaml = self.draft.to_yaml();
        // A line belongs to the setting whose key starts it; continuation lines inherit it, so
        // a problem on a list setting marks the whole block.
        let mut current_key = String::new();
        let lines: Vec<(usize, String, Option<Severity>)> = yaml
            .lines()
            .enumerate()
            .map(|(i, text)| {
                if let Some(key) = text.split(':').next()
                    && !text.starts_with([' ', '-', '\t'])
                    && !key.is_empty()
                {
                    current_key = key.to_string();
                }
                let severity = self.problems_for(&current_key).map(|p| p.severity).max();
                (i + 1, text.to_string(), severity)
            })
            .collect();

        v_flex()
            .w(px(340.))
            .h_full()
            .border_l_1()
            .border_color(cx.theme().colors.border)
            .child(
                h_flex()
                    .w_full()
                    .p_2()
                    .gap_2()
                    .items_center()
                    .child(Label::new("YAML preview").text_sm().font_semibold())
                    .child(
                        Label::new("read-only")
                            .text_xs()
                            .text_color(cx.theme().muted_foreground),
                    ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .w_full()
                    .p_2()
                    .gap_0()
                    .overflow_y_scrollbar()
                    .font_family(cx.theme().mono_font_family.clone())
                    .when(lines.is_empty(), |this| {
                        this.child(
                            Label::new("Nothing added yet.")
                                .text_xs()
                                .text_color(cx.theme().muted_foreground),
                        )
                    })
                    .children(lines.into_iter().map(|(n, text, severity)| {
                        let color = match severity {
                            Some(Severity::Error) => cx.theme().colors.danger,
                            Some(Severity::Warn) => cx.theme().colors.warning,
                            _ => cx.theme().foreground,
                        };
                        h_flex()
                            .w_full()
                            .gap_2()
                            .child(
                                div().w(px(24.)).child(
                                    Label::new(n.to_string())
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground),
                                ),
                            )
                            .child(Label::new(text).text_xs().text_color(color))
                    })),
            )
    }
}
