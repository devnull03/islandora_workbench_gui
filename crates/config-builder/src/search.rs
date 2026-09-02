//! The add-a-setting palette of mockup `1a`, built to Component Spec §08: type to filter the
//! catalogue, see the shape and default before adding, or start from one of the four task
//! templates.
//!
//! Three properties this owes to §08, all of which the previous flow-laid-out version got wrong:
//!
//! * The panel is **absolutely positioned** and `w_full` of the trigger, so its width is the
//!   trigger's width and nothing below it moves when results appear. Every row's text column is
//!   `flex_1 min_w(0) truncate`, so a long key cannot push the panel wider either. A results list
//!   that resizes the window is the defect this layout exists to prevent.
//! * The **whole row** adds the setting. §08 makes the row the target; a trailing `+` button is a
//!   smaller hit area for the same action, and two ways to do one thing.
//! * Results are **grouped**, under the schema's own section names, with a count.
//!
//! ponytail: hand-rolled rather than driven by `list::ListState`, which §08 asks for. The list
//! widget would bring virtualization and ↑↓/Enter/Esc, but its search field is fixed (magnifier
//! prefix, no counter slot) and the trigger §08 specifies is not expressible in it, so the rows,
//! the header bands and the trigger would all be custom anyway. 142 rows in a 360px scroll area
//! do not need virtualizing. What is genuinely lost is keyboard navigation; the hint strip says
//! so honestly rather than advertising keys that do nothing — wire it when focus routing between
//! a trigger input and a list is needed somewhere else too.

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme, Sizable, StyledExt, h_flex, input::Input, label::Label, scroll::ScrollableElement,
    v_flex,
};
use serde_yaml::Value;
use ui::tokens::{
    CHIP_H, GAP_MD, GAP_SM, GAP_XS, PICKER_MAX_H, PICKER_TRIGGER_H, RADIUS_SM, TYPE_COL_W,
};
use ui::{APP_CONTROL_SIZE, app_button};
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

/// How many results to show before it stops being a list and starts being the catalogue. The
/// panel scrolls at [`PICKER_MAX_H`] well before this, so the cap is about the tail of a vague
/// query, not about height.
const MAX_RESULTS: usize = 40;

/// Every setting in the catalogue, for the trigger's "7 of 142" counter.
fn catalogue_size() -> usize {
    catalog::addable().count()
}

impl ConfigBuilder {
    pub(super) fn render_search(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let query = self.search.read(cx).value().to_string();
        let matches = self.matches(&query);
        let show_results = self.search_open && !query.trim().is_empty();
        let colors = &cx.theme().colors;
        let muted = colors.muted_foreground;
        let border = if show_results {
            colors.primary
        } else {
            colors.border
        };
        let background = colors.background;
        let primary = colors.primary;
        let total = catalogue_size();

        v_flex()
            .w_full()
            .gap(GAP_MD)
            .child(
                // `relative` is what anchors the panel below. Without it the panel would position
                // itself against the scroll container instead of against this row.
                div()
                    .relative()
                    .w_full()
                    .child(
                        h_flex()
                            .w_full()
                            .h(PICKER_TRIGGER_H)
                            .px(GAP_MD)
                            .gap(GAP_MD)
                            .items_center()
                            .rounded(cx.theme().radius)
                            .border_1()
                            .border_color(border)
                            .bg(background)
                            // The chevron is the picker's mark: it says this row is a command
                            // surface, not another setting's value field.
                            .child(Label::new("›").text_sm().text_color(primary))
                            .child(
                                div().flex_1().min_w(px(0.)).child(
                                    Input::new(&self.search)
                                        .with_size(APP_CONTROL_SIZE)
                                        .appearance(false)
                                        .w_full(),
                                ),
                            )
                            .when(show_results, |this| {
                                this.child(
                                    Label::new(format!("{} of {total}", matches.len()))
                                        .text_xs()
                                        .text_color(muted),
                                )
                            }),
                    )
                    .when(show_results, |this| {
                        this.child(self.render_results(&query, &matches, cx))
                    }),
            )
            .when(self.draft.values.is_empty(), |this| {
                this.child(self.render_templates(cx))
            })
    }

    /// Settings ranked so a prefix match beats a mention anywhere.
    ///
    /// Settings already in the draft stay in the results rather than vanishing from them (§08):
    /// a key that disappears the instant you add it leaves you wondering whether the search is
    /// broken or the setting was never there. They come back dimmed and unclickable instead.
    fn matches(&self, query: &str) -> Vec<&'static SettingDef> {
        let q = query.trim().to_lowercase();
        let mut found: Vec<(u8, &'static SettingDef)> = catalog::addable()
            .filter_map(|def| rank(def, &q).map(|rank| (rank, def)))
            .collect();
        found.sort_by_key(|(rank, def)| (*rank, def.key.len()));
        found.into_iter().map(|(_, def)| def).collect()
    }

    /// The results panel: groups in the order their best match ranked, each under a header band.
    ///
    /// The height chain is load-bearing and was wrong twice. `overflow_y_scrollbar` moves the
    /// caller's size refinements onto a `size_full` wrapper, so a `max_h` written directly on the
    /// scrolling element resolves against an auto-height ancestor and clamps nothing. The panel
    /// therefore caps its own height, and the scroll area takes a definite one from flex —
    /// `flex_1` plus `min_h(0)`, the vertical twin of the zero minimum every row here needs.
    fn render_results(
        &self,
        query: &str,
        matches: &[&'static SettingDef],
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = cx.theme().colors;
        let muted = colors.muted_foreground;

        let groups = group_by_section(&matches[..matches.len().min(MAX_RESULTS)]);
        // Rows are rendered up front rather than inside `children`: that closure cannot borrow
        // `cx`, and the rows need it.
        let sections: Vec<(String, usize, Vec<AnyElement>)> = groups
            .into_iter()
            .map(|(group, items)| {
                let count = items.len();
                let rows = items
                    .into_iter()
                    .map(|def| self.render_result(def, cx).into_any_element())
                    .collect();
                (group, count, rows)
            })
            .collect();

        // `deferred` is what makes this a popover instead of a rectangle the rows draw over.
        // GPUI paints in tree order and absolute positioning does not change that, so the
        // settings list below — later in the tree — was painting straight through the panel.
        // Deferring moves only the paint; the layout stays exactly where it is.
        deferred(
            div()
                .absolute()
                .top_full()
                .left_0()
                // Exactly as wide as the trigger. This is the fix for a results list that used to
                // widen its own column: nothing inside the panel gets a vote on the width.
                .w_full()
                .mt(GAP_SM)
                .child(
                    v_flex()
                        .w_full()
                        .max_h(PICKER_MAX_H)
                        .rounded(cx.theme().radius_lg)
                        .border_1()
                        .border_color(colors.border)
                        .bg(colors.popover)
                        .overflow_hidden()
                        .child(
                            v_flex()
                                .w_full()
                                .flex_1()
                                // The vertical zero minimum. Without it this flex item's basis is
                                // its content and it simply grows past the cap above.
                                .min_h(px(0.))
                                .overflow_y_scrollbar()
                                .when(sections.is_empty(), |this| {
                                    this.child(
                                        h_flex()
                                            .w_full()
                                            .h(PICKER_TRIGGER_H)
                                            .px(GAP_MD)
                                            .items_center()
                                            .child(
                                                Label::new(format!("No setting matches {query}"))
                                                    .text_xs()
                                                    .text_color(muted),
                                            ),
                                    )
                                })
                                .children(sections.into_iter().map(|(group, count, rows)| {
                                    v_flex()
                                        .w_full()
                                        .child(
                                            h_flex()
                                                .w_full()
                                                .px(GAP_MD)
                                                .py(GAP_XS)
                                                .gap(GAP_SM)
                                                .items_center()
                                                .bg(colors.table_head)
                                                .border_b_1()
                                                .border_color(colors.table_row_border)
                                                .child(
                                                    Label::new(group.to_uppercase())
                                                        .text_xs()
                                                        .font_semibold()
                                                        .text_color(colors.table_head_foreground),
                                                )
                                                .child(
                                                    Label::new(count.to_string())
                                                        .text_xs()
                                                        .text_color(muted),
                                                ),
                                        )
                                        .children(rows)
                                })),
                        )
                        .child(
                            h_flex()
                                .w_full()
                                .flex_none()
                                .px(GAP_MD)
                                .py(GAP_XS)
                                .gap(GAP_MD)
                                .items_center()
                                .bg(colors.table_head)
                                .border_t_1()
                                .border_color(colors.table_row_border)
                                .child(
                                    Label::new("Click a setting to add it")
                                        .text_xs()
                                        .text_color(muted),
                                )
                                .child(
                                    Label::new("clear the box to close")
                                        .text_xs()
                                        .text_color(muted),
                                ),
                        ),
                ),
        )
    }

    fn render_result(&self, def: &'static SettingDef, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors;
        let muted = colors.muted_foreground;
        let added = self.draft.values.contains_key(&def.key);
        let key = def.key.clone();

        h_flex()
            .id(SharedString::from(format!("pick-{}", def.key)))
            .w_full()
            .px(GAP_MD)
            .py(GAP_SM)
            .gap(GAP_MD)
            .items_center()
            .border_b_1()
            .border_color(colors.table_row_border)
            .when(added, |this| this.opacity(0.5))
            .when(!added, |this| {
                this.cursor_pointer()
                    .hover(move |this| this.bg(colors.list_hover))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let Some(def) = catalog::find(&key) else {
                            return;
                        };
                        this.add_setting(def, cx);
                        // §08: adding keeps the panel open with the query intact, so a run of
                        // related settings is one search and several clicks. `add_setting` closes
                        // it, which is right for the keyboard flow and wrong here.
                        this.search_open = true;
                        cx.notify();
                    }))
            })
            .child(
                // The text column is the only thing allowed to flex, and it truncates. Everything
                // to its right is fixed width, so a row can never be wider than the panel.
                v_flex()
                    .flex_1()
                    .min_w(px(0.))
                    .gap(GAP_XS)
                    .child(Label::new(def.key.clone()).text_sm().truncate())
                    .when(!def.description.is_empty(), |this| {
                        this.child(
                            Label::new(def.description.clone())
                                .text_xs()
                                .text_color(muted)
                                .truncate(),
                        )
                    }),
            )
            .child(
                h_flex()
                    .h(CHIP_H)
                    .px(GAP_SM)
                    .items_center()
                    .flex_none()
                    .rounded(RADIUS_SM)
                    .border_1()
                    .border_color(colors.border)
                    .child(Label::new(def.shape.label()).text_xs().text_color(muted)),
            )
            .child(
                div().w(TYPE_COL_W).flex_none().child(
                    Label::new(if added {
                        "added".to_string()
                    } else {
                        default_preview(&def.default)
                    })
                    .text_xs()
                    .text_color(muted)
                    .truncate(),
                ),
            )
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

/// How well `def` answers `query`, or `None` if it does not. Lower is better: an exact prefix
/// of the key beats a mention inside it, which beats a mention in the prose. An empty query
/// matches everything at the lowest rank, so the ordering falls back to key length.
///
/// Ranking is deliberately substring and case-insensitive, with no fuzzy scoring (§08). Fuzzy
/// matching on 142 short snake_case keys mostly produces confident wrong answers.
fn rank(def: &SettingDef, query: &str) -> Option<u8> {
    if query.is_empty() {
        return Some(2);
    }
    let key = def.key.to_lowercase();
    if key.starts_with(query) {
        Some(0)
    } else if key.contains(query) {
        Some(1)
    } else if def.description.to_lowercase().contains(query)
        || def.group.to_lowercase().contains(query)
    {
        Some(2)
    } else {
        None
    }
}

/// Bucket ranked results under the schema's own section names, keeping both the order the
/// groups first appear in and the order within a group. The group holding the best match
/// therefore leads.
///
/// A `Vec` rather than a map because insertion order is the whole point, and there are a dozen
/// groups — the linear scan is cheaper than the hashing that would replace it.
fn group_by_section<'a>(matches: &[&'a SettingDef]) -> Vec<(String, Vec<&'a SettingDef>)> {
    let mut groups: Vec<(String, Vec<&'a SettingDef>)> = Vec::new();
    for def in matches {
        match groups.iter_mut().find(|(name, _)| name == &def.group) {
            Some((_, items)) => items.push(def),
            None => groups.push((def.group.clone(), vec![*def])),
        }
    }
    groups
}

#[cfg(test)]
mod tests {
    // Not `use super::*`: this module glob-imports `gpui`, whose own `test` macro would
    // shadow the one this needs.
    use super::{group_by_section, rank};
    use serde_yaml::Value;
    use workbench_integration::config::catalog::{Browse, SettingDef, Shape};

    /// Only the three fields the ranking reads are interesting; the rest is whatever a
    /// setting with no default and no choices looks like.
    fn def(key: &str, group: &str, description: &str) -> SettingDef {
        SettingDef {
            key: key.to_string(),
            group: group.to_string(),
            description: description.to_string(),
            shape: Shape::String,
            default: Value::Null,
            required: false,
            locked: false,
            choices: Vec::new(),
            unit: None,
            tokens: Vec::new(),
            browse: Browse::default(),
        }
    }

    /// The ranking is the whole reason the palette is usable: typing `roll` has to put
    /// `rollback_dir` above `timestamp_rollback`, and both above a setting that merely mentions
    /// rollback in its description.
    #[test]
    fn a_prefix_of_the_key_outranks_a_mention_anywhere_else() {
        let prefix = def("rollback_dir", "Rollback", "");
        let inside = def("timestamp_rollback", "Rollback", "");
        let prose = def(
            "log_file_path",
            "Logging",
            "Written before the rollback runs.",
        );
        let unrelated = def("user_agent", "HTTP", "Sent with every request.");

        assert_eq!(rank(&prefix, "roll"), Some(0));
        assert_eq!(rank(&inside, "roll"), Some(1));
        assert_eq!(rank(&prose, "roll"), Some(2));
        assert_eq!(rank(&unrelated, "roll"), None);
    }

    #[test]
    fn an_empty_query_matches_everything_at_one_rank() {
        assert_eq!(rank(&def("user_agent", "HTTP", ""), ""), Some(2));
        assert_eq!(rank(&def("rollback_dir", "Rollback", ""), ""), Some(2));
    }

    /// Group order follows the best match, not the schema's declaration order — the section
    /// holding what you searched for has to be the one you see first.
    #[test]
    fn groups_appear_in_the_order_their_best_match_did() {
        let a = def("rollback_dir", "Rollback", "");
        let b = def("log_file_path", "Logging", "");
        let c = def("timestamp_rollback", "Rollback", "");
        let groups = group_by_section(&[&a, &b, &c]);

        let names: Vec<&str> = groups.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["Rollback", "Logging"]);
        assert_eq!(
            groups[0].1.len(),
            2,
            "both Rollback settings land in one section"
        );
        assert_eq!(groups[1].1.len(), 1);
    }

    #[test]
    fn no_matches_means_no_sections_rather_than_one_empty_one() {
        assert!(group_by_section(&[]).is_empty());
    }
}
