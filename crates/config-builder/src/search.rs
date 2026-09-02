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
//! Hand-rolled rather than driven by `list::ListState`, whose fixed magnifier trigger cannot
//! express this design's chevron, inline result counter, grouped metadata rows, or persistent
//! footer. The picker owns Up/Down, Enter, Escape and scroll-to-selection directly; 142 catalogue
//! rows behind a 40-result cap do not need virtualization.

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme, Sizable, StyledExt, button::Button, h_flex, input::Input, label::Label,
    scroll::ScrollableElement, v_flex,
};
use serde_yaml::Value;
use ui::tokens::{
    GAP_2XL, GAP_MD, GAP_SM, GAP_XS, PICKER_GLYPH_W, PICKER_MAX_H, PICKER_TRIGGER_H, TYPE_COL_W,
};
use ui::{APP_CONTROL_SIZE, app_tag};
use workbench_integration::config::catalog::{self, SettingDef, Shape};

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
        self.ensure_search_selection(&matches);
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
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                        let handled = match event.keystroke.key.as_str() {
                            "up" => {
                                this.move_search_selection(-1, cx);
                                true
                            }
                            "down" => {
                                this.move_search_selection(1, cx);
                                true
                            }
                            "enter" => {
                                this.activate_search_selection(cx);
                                true
                            }
                            "escape" => {
                                this.search_open = false;
                                cx.notify();
                                true
                            }
                            _ => false,
                        };
                        if handled {
                            cx.stop_propagation();
                        }
                    }))
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
                            .when(!query.trim().is_empty(), |this| {
                                this.child(
                                    Label::new(format!("{} of {total} settings", matches.len()))
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

    fn ensure_search_selection(&mut self, matches: &[&SettingDef]) {
        let keys = visible_result_keys(matches);
        let valid = self.search_selected.as_ref().is_some_and(|selected| {
            keys.iter().any(|key| key == selected)
                && !self.draft.values.contains_key(selected.as_ref())
        });
        if !valid {
            self.search_selected = keys
                .into_iter()
                .find(|key| !self.draft.values.contains_key(key.as_ref()));
        }
    }

    fn move_search_selection(&mut self, direction: isize, cx: &mut Context<Self>) {
        let query = self.search.read(cx).value().to_string();
        if query.trim().is_empty() {
            return;
        }
        self.search_open = true;
        let matches = self.matches(&query);
        let keys = visible_result_keys(&matches);
        let next = next_selectable_key(&keys, self.search_selected.as_ref(), direction, |key| {
            self.draft.values.contains_key(key.as_ref())
        });
        let Some(next) = next else {
            self.search_selected = None;
            cx.notify();
            return;
        };
        self.search_selected = Some(next.clone());
        if let Some(child_index) = result_child_index(&matches, &next) {
            self.search_scroll.scroll_to_item(child_index);
        }
        cx.notify();
    }

    fn activate_search_selection(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.search_selected.clone() else {
            return;
        };
        if self.draft.values.contains_key(key.as_ref()) {
            return;
        }
        let Some(def) = catalog::find(key.as_ref()) else {
            return;
        };
        self.add_setting(def, cx);
        self.search_open = true;

        let query = self.search.read(cx).value().to_string();
        let matches = self.matches(&query);
        let visual_keys = visible_result_keys(&matches);
        self.search_selected = next_selectable_key(&visual_keys, Some(&key), 1, |candidate| {
            self.draft.values.contains_key(candidate.as_ref())
        });
        if let Some(selected) = &self.search_selected
            && let Some(child_index) = result_child_index(&matches, selected)
        {
            self.search_scroll.scroll_to_item(child_index);
        }
        cx.notify();
    }

    /// The results panel: groups in the order their best match ranked, each under a header band.
    ///
    /// The result body is content-sized until it reaches [`PICKER_MAX_H`], then scrolls. Do not
    /// use `overflow_y_scrollbar` here: that convenience wrapper renders a `size_full` container,
    /// which collapses inside this auto-height absolute popover. Keeping the native GPUI scroll
    /// element also means keyboard `scroll_to_item` and the visible scrollbar share one handle.
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
        // Headers and rows are direct scroll children. That lets `ScrollHandle::scroll_to_item`
        // reveal a keyboard-selected row even though the visual list is grouped.
        let mut children: Vec<AnyElement> = Vec::new();
        for (group, items) in groups {
            children.push(
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
                        Label::new(format!("{} matches", items.len()))
                            .text_xs()
                            .text_color(muted),
                    )
                    .into_any_element(),
            );
            children.extend(
                items
                    .into_iter()
                    .map(|def| self.render_result(def, cx).into_any_element()),
            );
        }

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
                        .rounded(cx.theme().radius_lg)
                        .border_1()
                        .border_color(colors.border)
                        .bg(colors.popover)
                        .overflow_hidden()
                        .child(
                            v_flex()
                                .id("setting-search-results")
                                .w_full()
                                .max_h(PICKER_MAX_H)
                                .track_scroll(&self.search_scroll)
                                .overflow_y_scroll()
                                .vertical_scrollbar(&self.search_scroll)
                                .when(children.is_empty(), |this| {
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
                                .children(children),
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
                                .child(Label::new("↑↓ to move").text_xs().text_color(muted))
                                .child(Label::new("Enter to add").text_xs().text_color(muted))
                                .child(Label::new("Esc to close").text_xs().text_color(muted)),
                        ),
                ),
        )
    }

    fn render_result(&self, def: &'static SettingDef, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors;
        let muted = colors.muted_foreground;
        let added = self.draft.values.contains_key(&def.key);
        let key: SharedString = def.key.clone().into();
        let selected = self.search_selected.as_ref() == Some(&key);
        let hover_key = key.clone();

        h_flex()
            .id(SharedString::from(format!("pick-{}", def.key)))
            .w_full()
            .px(GAP_MD)
            .py(GAP_SM)
            .gap(GAP_MD)
            .items_center()
            .border_b_1()
            .border_color(colors.table_row_border)
            .when(selected && !added, |this| this.bg(colors.list_hover))
            .when(added, |this| this.opacity(0.5))
            .when(!added, |this| {
                this.cursor_pointer()
                    .hover(move |this| this.bg(colors.list_hover))
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if *hovered {
                            this.search_selected = Some(hover_key.clone());
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.search_selected = Some(key.clone());
                        this.activate_search_selection(cx);
                    }))
            })
            .child(
                div().w(PICKER_GLYPH_W).flex_none().child(
                    Label::new(shape_glyph(def.shape))
                        .text_xs()
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_color(colors.primary),
                ),
            )
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
                app_tag()
                    .flex_none()
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
                    .text_right()
                    .truncate(),
                ),
            )
    }

    fn render_templates(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .w_full()
            .mt(GAP_2XL)
            .pt(GAP_2XL)
            .gap(GAP_MD)
            .border_t_1()
            .border_color(cx.theme().colors.border)
            .child(
                Label::new("OR START FROM A TEMPLATE")
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(
                h_flex()
                    .w_full()
                    .gap(GAP_MD)
                    .items_stretch()
                    .flex_wrap()
                    .children(TEMPLATES.iter().map(|(name, task, extras)| {
                        let task = *task;
                        let extras = *extras;
                        div().flex_1().min_w(px(140.)).child(
                            Button::new(SharedString::from(format!("template-{task}")))
                                .outline()
                                .w_full()
                                .h_auto()
                                .p(GAP_MD)
                                .items_start()
                                .justify_start()
                                .child(
                                    v_flex()
                                        .w_full()
                                        .gap(GAP_XS)
                                        .items_start()
                                        .child(Label::new(*name).text_sm().font_semibold())
                                        .child(
                                            Label::new(format!(
                                                "task: {task}, plus {} settings",
                                                extras.len()
                                            ))
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground),
                                        ),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.apply_template(task, extras, cx);
                                })),
                        )
                    })),
            )
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
        Value::Null => "—".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) if s.len() <= 24 => s.clone(),
        Value::String(s) => format!("{}…", &s[..24]),
        Value::Sequence(items) => format!("{} items", items.len()),
        Value::Mapping(m) => format!("{} entries", m.len()),
        _ => String::new(),
    }
}

fn shape_glyph(shape: Shape) -> &'static str {
    match shape {
        Shape::Boolean => "□",
        Shape::Integer => "12",
        Shape::FilePath | Shape::ConfigRef => "/",
        Shape::String | Shape::Delimiter | Shape::Url | Shape::TemplateString => "ab",
        Shape::Enum | Shape::NullableEnum => "◇",
        Shape::ListOfStrings | Shape::ListOfNumbers | Shape::ListOfOneKeyMaps => "[]",
        Shape::Map | Shape::MapOfLists => "{}",
        Shape::CommandList => ">",
    }
}

/// Visible result order after the group projection, rather than catalogue rank order. Keyboard
/// movement must follow what the eye sees when two groups interleave in the ranked input.
fn visible_result_keys(matches: &[&SettingDef]) -> Vec<SharedString> {
    group_by_section(&matches[..matches.len().min(MAX_RESULTS)])
        .into_iter()
        .flat_map(|(_, items)| items.into_iter().map(|def| def.key.clone().into()))
        .collect()
}

/// Direct-child index in the scroll body. Each group header consumes one slot before its rows.
fn result_child_index(matches: &[&SettingDef], selected: &SharedString) -> Option<usize> {
    let mut child_index = 0;
    for (_, items) in group_by_section(&matches[..matches.len().min(MAX_RESULTS)]) {
        child_index += 1;
        for def in items {
            if def.key.as_str() == selected.as_ref() {
                return Some(child_index);
            }
            child_index += 1;
        }
    }
    None
}

fn next_selectable_key(
    keys: &[SharedString],
    current: Option<&SharedString>,
    direction: isize,
    is_unavailable: impl Fn(&SharedString) -> bool,
) -> Option<SharedString> {
    if keys.is_empty() {
        return None;
    }
    let step = if direction < 0 { -1 } else { 1 };
    let start = current
        .and_then(|selected| keys.iter().position(|key| key == selected))
        .map(|index| index as isize)
        .unwrap_or(if step < 0 { 0 } else { -1 });
    (1..=keys.len()).find_map(|offset| {
        let index = (start + step * offset as isize).rem_euclid(keys.len() as isize) as usize;
        (!is_unavailable(&keys[index])).then(|| keys[index].clone())
    })
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
    use super::{
        group_by_section, next_selectable_key, rank, result_child_index, shape_glyph,
        visible_result_keys,
    };
    use gpui::SharedString;
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

    #[test]
    fn visible_keys_and_scroll_indices_follow_grouped_order() {
        let a = def("a", "First", "");
        let b = def("b", "Second", "");
        let c = def("c", "First", "");
        let matches = [&a, &b, &c];

        assert_eq!(
            visible_result_keys(&matches),
            vec![SharedString::from("a"), "c".into(), "b".into()]
        );
        // Header, a, c, header, b.
        assert_eq!(result_child_index(&matches, &"c".into()), Some(2));
        assert_eq!(result_child_index(&matches, &"b".into()), Some(4));
    }

    #[test]
    fn keyboard_selection_wraps_and_skips_unavailable_rows() {
        let keys: Vec<SharedString> = vec!["a".into(), "b".into(), "c".into()];
        let unavailable = |key: &SharedString| key.as_ref() == "b";

        assert_eq!(
            next_selectable_key(&keys, Some(&"a".into()), 1, unavailable),
            Some("c".into())
        );
        assert_eq!(
            next_selectable_key(&keys, Some(&"c".into()), 1, unavailable),
            Some("a".into())
        );
        assert_eq!(
            next_selectable_key(&keys, None, -1, unavailable),
            Some("c".into())
        );
        assert_eq!(next_selectable_key(&keys, None, 1, |_| true), None);
    }

    #[test]
    fn every_shape_family_has_a_compact_palette_glyph() {
        assert_eq!(shape_glyph(Shape::Boolean), "□");
        assert_eq!(shape_glyph(Shape::Integer), "12");
        assert_eq!(shape_glyph(Shape::FilePath), "/");
        assert_eq!(shape_glyph(Shape::TemplateString), "ab");
        assert_eq!(shape_glyph(Shape::ListOfStrings), "[]");
        assert_eq!(shape_glyph(Shape::Map), "{}");
    }
}
