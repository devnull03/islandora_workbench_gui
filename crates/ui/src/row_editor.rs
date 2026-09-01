//! The bordered table that list, map, and pair editors all live in.
//!
//! Component Spec §07. Shell, header band, zebra rows, a hover-revealed remove column, and a
//! footer strip for the add affordance.
//!
//! ponytail: this is the chrome only — the cells stay with the caller. §07 asks for a column-spec
//! refactor ("build RowEditor once; the others are constructor calls"), and the *visual* half of
//! that is here: header, zebra, row height, the action column. The other half is not, because in
//! this codebase a cell is not a value — it is an `InputState` created and looked up by field id
//! in `ConfigBuilder`'s widget cache, and wired to that type's own row-mutation methods. Lifting
//! it would mean handing this type an input factory plus three callbacks to build one `h_flex`,
//! which is exactly the callback machinery a generic row abstraction is supposed to avoid. The
//! duplication that actually hurt was the box, and that is what this owns. Revisit if a second
//! crate ever grows a row editor.

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{ActiveTheme, h_flex, v_flex};

use crate::tokens::{CONTROL_H, CONTROL_H_SM, GAP_MD, GAP_SM, ROW_ACTION_W};

/// Hover-group name shared by every row, so [`RowActions`] can reveal against the row it sits
/// in without either side knowing the other's index.
const ROW_GROUP: &str = "row-editor-row";

#[derive(IntoElement)]
pub struct RowEditor {
    /// Column titles band. Omitted for a plain list, where a header would name nothing — §07
    /// shows it "only when columns are named".
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

    /// The add-a-row affordance, drawn as a strip below the last row.
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
        let colors = cx.theme().colors;
        let last = self.rows.len().saturating_sub(1);

        v_flex()
            .w_full()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(colors.border)
            // Rows paint to the frame's inner edge, so the frame has to clip them or their square
            // corners show through its curve.
            .overflow_hidden()
            .when_some(self.header, |el, header| {
                el.child(
                    h_flex()
                        .w_full()
                        .h(CONTROL_H)
                        .px(GAP_MD)
                        .items_center()
                        .bg(colors.table_head)
                        .border_b_1()
                        .border_color(colors.table_row_border)
                        .child(header),
                )
            })
            .children(self.rows.into_iter().enumerate().map(|(i, row)| {
                h_flex()
                    // Named so `RowActions` inside can reveal on hover; the name is shared by
                    // every row, which is what GPUI's group hover wants — the nearest
                    // ancestor with the name is the one that matters.
                    .group(ROW_GROUP)
                    .w_full()
                    .min_h(CONTROL_H)
                    .px(GAP_MD)
                    .py(px(2.))
                    .items_center()
                    // §00 confines zebra to row-editor tables, which is what this is. It is what
                    // lets the eye follow one row of a five-column map across the whole width.
                    .when(i % 2 == 1, |el| el.bg(colors.table_even))
                    // No rule under the final row: the box's own border is already there, and two
                    // lines a pixel apart read as a rendering bug.
                    .when(i != last, |el| {
                        el.border_b_1().border_color(colors.table_row_border)
                    })
                    .child(row)
            }))
            .when_some(self.add, |el, add| {
                el.child(
                    // §07: a plain strip, no top border. The dashed affordance inside it is what
                    // says "not a row"; a rule as well would make it look like one more.
                    h_flex()
                        .w_full()
                        .h(CONTROL_H_SM)
                        .px(GAP_SM)
                        .items_center()
                        .child(add),
                )
            })
    }
}

/// The trailing action column of a row — remove, and anything beside it.
///
/// §07 hides these until the row is hovered or focus is inside it, so a table of eight rows is
/// not also a column of eight crosses. The width is reserved either way, or every row would
/// reflow on hover.
#[derive(IntoElement)]
pub struct RowActions {
    children: Vec<AnyElement>,
}

impl RowActions {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
        }
    }
}

impl Default for RowActions {
    fn default() -> Self {
        Self::new()
    }
}

impl ParentElement for RowActions {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for RowActions {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        h_flex()
            // The width is reserved whether or not the actions are showing. Revealing them
            // by adding width would reflow every cell in the row on hover.
            .w(ROW_ACTION_W * self.children.len().max(1) as f32)
            .flex_none()
            .justify_end()
            .items_center()
            .invisible()
            .group_hover(ROW_GROUP, |el| el.visible())
            .children(self.children)
    }
}
