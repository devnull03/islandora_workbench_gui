//! Read-only YAML preview (mockup `1b`): the file exactly as it will be written, line
//! numbered, with the lines carrying a problem marked.
//!
//! It is a preview, not a second editor. One editable representation of the config is enough;
//! two that can disagree is a bug generator.

use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::YAML_PANEL_WIDTH;
use gpui_component::{ActiveTheme, Disableable, h_flex, input::Editor, label::Label, v_flex};
use workbench_integration::config::validate::Severity;

use super::ConfigBuilder;

impl ConfigBuilder {
    /// The YAML text and its problem markers, recomputed only when the draft changed — never
    /// once per frame. `revalidate` (every draft edit) bumps `draft_revision`.
    fn refresh_yaml(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.yaml_revision == Some(self.draft_revision) {
            return;
        }
        self.yaml_revision = Some(self.draft_revision);
        let yaml = self.draft.to_yaml();
        self.yaml_markers = line_markers(&yaml, |key| {
            self.problems_for(key).map(|p| p.severity).max()
        });
        // `set_value` clears the selection, so it runs only when the text really changed —
        // otherwise copying from the preview would be impossible.
        if self.yaml_text != yaml {
            self.yaml_editor.update(cx, |editor, cx| {
                editor.set_value(yaml.clone(), window, cx);
            });
            self.yaml_text = yaml;
        }
    }

    pub(super) fn render_yaml_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.refresh_yaml(window, cx);
        let colors = cx.theme().colors;
        let problems: Vec<AnyElement> = self
            .yaml_markers
            .iter()
            .map(|(line, severity)| {
                let (label, color) = match severity {
                    Severity::Error => ("error", colors.danger),
                    _ => ("warning", colors.warning),
                };
                Label::new(format!("line {line}: {label}"))
                    .text_xs()
                    .text_color(color)
                    .into_any_element()
            })
            .collect();
        let yaml_for_copy = self.yaml_text.clone();
        // Beside the form in its own window; under it when docked, where the pane is too
        // narrow for two columns.
        let docked = self.host == crate::BuilderHost::Docked;

        v_flex()
            .map(|panel| {
                if docked {
                    panel.w_full().h(relative(0.4)).border_t_1()
                } else {
                    panel.w(YAML_PANEL_WIDTH).h_full().border_l_1()
                }
            })
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
                        ui::ghost_button("copy-yaml")
                            .label("Copy YAML")
                            .disabled(yaml_for_copy.is_empty())
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    yaml_for_copy.clone(),
                                ));
                            }),
                    )
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
                    .gap_2()
                    .when(!problems.is_empty(), |this| {
                        this.child(h_flex().px_2().gap_2().flex_wrap().children(problems))
                    })
                    .child(
                        Editor::new(&self.yaml_editor)
                            .readonly(true)
                            .h(relative(1.))
                            .w_full()
                            .rounded_none(),
                    ),
            )
    }
}

/// `(line number, severity)` for each YAML line whose setting has a warning or error. A line
/// belongs to the setting whose key starts it; continuation lines inherit it, so a problem on a
/// list setting marks the whole block.
pub(crate) fn line_markers(
    yaml: &str,
    severity_of: impl Fn(&str) -> Option<Severity>,
) -> Vec<(usize, Severity)> {
    let mut current_key = "";
    yaml.lines()
        .enumerate()
        .filter_map(|(i, text)| {
            if !text.starts_with([' ', '-', '\t'])
                && let Some(key) = text.split(':').next()
                && !key.is_empty()
            {
                current_key = key;
            }
            // Only the two levels worth acting on; "line 4: ok" tells you what you assumed.
            severity_of(current_key)
                .filter(|s| *s != Severity::Ok)
                .map(|s| (i + 1, s))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Severity, line_markers};

    #[test]
    fn continuation_lines_carry_their_settings_marker() {
        let yaml = "task: create\nmedia_types:\n- image: [png]\n- file: [pdf]\nid_field: id\n";
        let markers = line_markers(yaml, |key| match key {
            "media_types" => Some(Severity::Error),
            "id_field" => Some(Severity::Ok),
            _ => None,
        });
        assert_eq!(
            markers,
            vec![
                (2, Severity::Error),
                (3, Severity::Error),
                (4, Severity::Error)
            ]
        );
    }
}
