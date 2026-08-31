//! The bordered box that list, map, and pair editors all live in.
//!
//! ponytail: this is the chrome only — the box, the row separators, the dashed add strip. The
//! cells stay with the caller. The per-shape cell construction in the config builder is fused to
//! its widget cache (inputs are created and looked up by field id) and to its own row-mutation
//! methods, so lifting it here would mean handing this type an input factory and three callbacks
//! to build one `h_flex`. If a second crate ever grows a row editor, revisit; until then the
//! duplication that mattered was the box, and that is what this owns.
//!
//! Geometry is the design language's §09: rows separated by a 1px `table_row_border`, a header
//! band at `table_head` when there are named columns, and the add affordance as a dashed strip
//! that is visibly not a row.

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{ActiveTheme, h_flex, v_flex};

use crate::tokens::{CONTROL_H_SM, GAP_SM};

#[derive(IntoElement)]
pub struct RowEditor {
    /// Column titles band; omitted for a plain list, where a header would name nothing.
    header: Option<AnyElement>,
    rows: Vec<AnyElement>,
    add: Option<AnyElement>,
}

impl RowEditor {
    pub fn new() -> Self {
        Self {
            header: None,
            rows: Vec::new(),
            add: None,
        }
    }

    pub fn header(mut self, header: impl IntoElement) -> Self {
        self.header = Some(header.into_any_element());
        self
    }

    /// The add-a-row affordance, drawn as the last child inside a dashed strip.
    pub fn add_row(mut self, add: impl IntoElement) -> Self {
        self.add = Some(add.into_any_element());
        self
    }
}

impl Default for RowEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl ParentElement for RowEditor {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.rows.extend(elements);
    }
}

impl RenderOnce for RowEditor {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = &cx.theme().colors;
        let separator = colors.table_row_border;
        let head = colors.table_head;
        let last = self.rows.len().saturating_sub(1);

        v_flex()
            .w_full()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(colors.border)
            .when_some(self.header, |el, header| {
                el.child(
                    h_flex()
                        .w_full()
                        .px(GAP_SM)
                        .py(px(4.))
                        .bg(head)
                        .border_b_1()
                        .border_color(separator)
                        .child(header),
                )
            })
            .children(self.rows.into_iter().enumerate().map(|(i, row)| {
                h_flex()
                    .w_full()
                    .px(GAP_SM)
                    .py(px(4.))
                    // No rule under the final row: the box's own border is already there, and
                    // two lines a pixel apart read as a rendering bug.
                    .when(i != last, |el| el.border_b_1().border_color(separator))
                    .child(row)
            }))
            .when_some(self.add, |el, add| {
                el.child(
                    h_flex()
                        .w_full()
                        .h(CONTROL_H_SM)
                        .px(GAP_SM)
                        .items_center()
                        .border_t_1()
                        .border_dashed()
                        .border_color(separator)
                        .child(add),
                )
            })
    }
}
