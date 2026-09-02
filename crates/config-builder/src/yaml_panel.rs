//! Read-only YAML preview (mockup `1b`): the file exactly as it will be written, line
//! numbered, with the lines carrying a problem marked.
//!
//! It is a preview, not a second editor. One editable representation of the config is enough;
//! two that can disagree is a bug generator.

use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::YAML_PANEL_WIDTH;
use gpui_component::{ActiveTheme, h_flex, input::Editor, label::Label, v_flex};
use workbench_integration::config::validate::Severity;

use super::ConfigBuilder;

impl ConfigBuilder {
    pub(super) fn render_yaml_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
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

        // Keep the editor's state stable while the draft is unchanged. `set_value` clears the
        // selection, so calling it every frame would make copying a generated file impossible.
        if self.yaml_text != yaml {
            self.yaml_editor.update(cx, |editor, cx| {
                editor.set_value(yaml.clone(), window, cx);
            });
            self.yaml_text = yaml;
        }

        let problems: Vec<SharedString> = lines
            .iter()
            .filter_map(|(line, _, severity)| {
                // Only the two levels worth acting on. "line 4: ok" is a chip that costs a glance
                // and tells you the thing you already assumed.
                let label = match (*severity)? {
                    Severity::Error => "error",
                    Severity::Warn => "warning",
                    Severity::Ok => return None,
                };
                Some(format!("line {line}: {label}").into())
            })
            .collect();

        v_flex()
            .w(YAML_PANEL_WIDTH)
            .h_full()
            .border_l_1()
            .border_color(cx.theme().colors.border)
            .child(
                h_flex()
                    .w_full()
                    .p_2()
                    .gap_2()
                    .items_center()
                    .bg(cx.theme().colors.table_head)
                    .border_b_1()
                    .border_color(cx.theme().colors.border)
                    .child(
                        Label::new("YAML PREVIEW")
                            .text_xs()
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_color(cx.theme().colors.table_head_foreground),
                    )
                    .child(div().flex_1())
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
                    .min_h_0()
                    .p_2()
                    .gap_2()
                    .when(lines.is_empty(), |this| {
                        this.child(
                            Label::new("Nothing added yet.")
                                .text_xs()
                                .text_color(cx.theme().muted_foreground),
                        )
                    })
                    .when(!problems.is_empty(), |this| {
                        this.child(h_flex().gap_2().flex_wrap().children(problems.iter().map(
                            |problem| {
                                Label::new(problem.clone())
                                    .text_xs()
                                    .text_color(cx.theme().colors.warning)
                            },
                        )))
                    })
                    .child(
                        Editor::new(&self.yaml_editor)
                            .readonly(true)
                            .h(relative(1.))
                            .w_full(),
                    ),
            )
    }
}
