//! The add-a-setting palette of mockup `1a`: type to filter the catalogue, see the type and
//! default before adding, or start from one of the four task templates.

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme, IconName, Sizable, StyledExt, h_flex, input::Input, label::Label, v_flex,
};
use serde_yaml::Value;
use ui::tokens::GAP_XS;
use ui::{APP_CONTROL_SIZE, Card, app_button};
use workbench_integration::config::catalog::{self, SettingDef};

use super::ConfigBuilder;

/// The one-click starting points. Each is a task plus the settings that task almost always
/// needs — enough to have something real on screen rather than an empty form.
const TEMPLATES: &[(&str, &str, &[&str])] = &[
    (
        "Create nodes",
        "create",
        &[
            "id_field",
            "allow_missing_files",
            "log_file_path",
            "timestamp_rollback",
            "rollback_dir",
            "allow_adding_terms",
        ],
    ),
    (
        "Add media",
        "add_media",
        &[
            "id_field",
            "media_use_tid",
            "log_file_path",
            "allow_missing_files",
        ],
    ),
    (
        "Update metadata",
        "update",
        &[
            "id_field",
            "update_mode",
            "log_file_path",
            "csv_value_templates",
        ],
    ),
    (
        "Export CSV",
        "export_csv",
        &[
            "export_csv_file_path",
            "export_csv_field_list",
            "export_csv_term_mode",
            "log_file_path",
        ],
    ),
];

/// How many results to show before it stops being a list and starts being the catalogue.
const MAX_RESULTS: usize = 12;

impl ConfigBuilder {
    pub(super) fn render_search(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let query = self.search.read(cx).value().to_string();
        let matches = self.matches(&query);
        let show_results = self.search_open && !query.trim().is_empty();

        v_flex()
            .w_full()
            .gap_2()
            .child(
                h_flex().w_full().gap_2().items_center().child(
                    div().flex_1().child(
                        Input::new(&self.search)
                            .with_size(APP_CONTROL_SIZE)
                            .w_full(),
                    ),
                ),
            )
            .when(show_results, |this| {
                this.child(self.render_results(&matches, cx))
            })
            .when(self.draft.values.is_empty(), |this| {
                this.child(self.render_templates(cx))
            })
    }

    /// Settings not already in the draft, ranked so a prefix match beats a mention anywhere.
    fn matches(&self, query: &str) -> Vec<&'static SettingDef> {
        let q = query.trim().to_lowercase();
        let mut found: Vec<(u8, &'static SettingDef)> = catalog::addable()
            .filter(|def| !self.draft.values.contains_key(&def.key))
            .filter_map(|def| {
                if q.is_empty() {
                    return Some((2, def));
                }
                let key = def.key.to_lowercase();
                if key.starts_with(&q) {
                    Some((0, def))
                } else if key.contains(&q) {
                    Some((1, def))
                } else if def.description.to_lowercase().contains(&q)
                    || def.group.to_lowercase().contains(&q)
                {
                    Some((2, def))
                } else {
                    None
                }
            })
            .collect();
        found.sort_by_key(|(rank, def)| (*rank, def.key.len()));
        found.into_iter().map(|(_, def)| def).collect()
    }

    fn render_results(
        &self,
        matches: &[&'static SettingDef],
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let hidden = matches.len().saturating_sub(MAX_RESULTS);

        Card::new()
            .gap(GAP_XS)
            .child(
                Label::new(format!(
                    "{} matching setting{}",
                    matches.len(),
                    if matches.len() == 1 { "" } else { "s" }
                ))
                .text_xs()
                .text_color(cx.theme().muted_foreground),
            )
            .children(matches.iter().take(MAX_RESULTS).map(|def| {
                let key = def.key.clone();
                h_flex()
                    .w_full()
                    .gap_2()
                    .items_center()
                    .child(
                        v_flex()
                            .flex_1()
                            .child(Label::new(def.key.clone()).text_sm().font_semibold())
                            .when(!def.description.is_empty(), |this| {
                                this.child(
                                    Label::new(def.description.clone())
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground),
                                )
                            }),
                    )
                    .child(
                        Label::new(def.group.clone())
                            .text_xs()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        Label::new(def.shape.label())
                            .text_xs()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        Label::new(default_preview(&def.default))
                            .text_xs()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        app_button(SharedString::from(format!("add-{key}")))
                            .outline()
                            .xsmall()
                            .icon(IconName::Plus)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(def) = catalog::find(&key) {
                                    this.add_setting(def, cx);
                                    this.search.update(cx, |search, cx| {
                                        search.set_value("", window, cx);
                                    });
                                }
                            })),
                    )
            }))
            .when(hidden > 0, |this| {
                this.child(
                    Label::new(format!("{hidden} more — keep typing to narrow it down"))
                        .text_xs()
                        .text_color(cx.theme().muted_foreground),
                )
            })
    }

    fn render_templates(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .w_full()
            .gap_2()
            .child(
                Label::new("Or start from a template")
                    .text_sm()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(h_flex().gap_2().flex_wrap().children(TEMPLATES.iter().map(
                |(name, task, extras)| {
                    let task = *task;
                    let extras = *extras;
                    app_button(SharedString::from(format!("template-{task}")))
                        .outline()
                        .small()
                        .label(format!("{name} — task: {task}, plus {}", extras.len()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.apply_template(task, extras, cx);
                        }))
                },
            )))
    }

    fn apply_template(&mut self, task: &str, extras: &[&str], cx: &mut Context<Self>) {
        if let Some(def) = catalog::find("task") {
            self.add_setting(def, cx);
        }
        self.draft
            .values
            .insert("task".into(), Value::String(task.to_string()));
        self.forget_widgets("task");
        for key in extras {
            if let Some(def) = catalog::find(key) {
                self.add_setting(def, cx);
            }
        }
        self.revalidate(cx);
    }
}

/// The default, short enough to sit at the end of a search row.
fn default_preview(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) if s.len() <= 24 => s.clone(),
        Value::String(s) => format!("{}…", &s[..24]),
        Value::Sequence(items) => format!("{} items", items.len()),
        Value::Mapping(m) => format!("{} entries", m.len()),
        _ => String::new(),
    }
}
