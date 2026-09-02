//! A short enum as one strip of joined cells rather than a dropdown.
//!
//! Component Spec §05: one border around the group, cells divided by a 1px rule, the selected
//! cell filled with `primary`. The win over a dropdown is one click instead of two, and every
//! legal value visible without opening anything.
//!
//! Up to [`MAX_SEGMENTS`] options only. §05 also describes a `+N` overflow cell for five to
//! seven options, which is **not** built: the cell has to be a dropdown trigger showing `+N`,
//! `Select` has no custom-trigger hook, and a `Select` dropped into the strip renders its own
//! full-width trigger showing the selected label — which is how `task` ended up as a strip
//! wider than its column with the selected value in it twice. §05's primary rule covers the
//! case anyway: past four options, use a plain dropdown.
//!
//! Stateless on purpose: the selected value is passed in and the click is a callback, so this
//! adds nothing to whatever already owns the value. A control with its own state entity would be
//! a second place for the answer to live, and the answer already lives in the draft.

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{ActiveTheme, h_flex, label::Label};

use crate::tokens::{CONTROL_H, GAP_MD};

/// Options that fit inline. Past this the strip is wider than the dropdown it replaced, so the
/// caller should render a dropdown instead — see [`Segmented::fits`].
pub const MAX_SEGMENTS: usize = 4;

type SelectFn = Box<dyn Fn(&SharedString, &mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct Segmented {
    id: SharedString,
    options: Vec<(SharedString, SharedString)>,
    selected: Option<SharedString>,
    on_select: Option<SelectFn>,
}

impl Segmented {
    /// `options` are `(value, label)` pairs, in the order they should read.
    pub fn new(
        id: impl Into<SharedString>,
        options: impl IntoIterator<Item = (SharedString, SharedString)>,
    ) -> Self {
        Self {
            id: id.into(),
            options: options.into_iter().collect(),
            selected: None,
            on_select: None,
        }
    }

    pub fn selected(mut self, selected: Option<SharedString>) -> Self {
        self.selected = selected;
        self
    }

    pub fn on_select(
        mut self,
        handler: impl Fn(&SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Box::new(handler));
        self
    }

    /// Whether this many options belong in a strip at all. The caller renders a dropdown when
    /// they do not, so the decision lives here rather than being spelled out per call site.
    pub fn fits(count: usize) -> bool {
        count <= MAX_SEGMENTS
    }
}

impl RenderOnce for Segmented {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors;
        let on_select = self.on_select.map(std::rc::Rc::new);
        let selected = self.selected;
        let id = self.id;
        let cells: Vec<(SharedString, SharedString)> = self.options;

        h_flex()
            .h(CONTROL_H)
            // `fit-content`: a three-option enum must not stretch to the width of the control
            // column, or it reads as a text field that happens to have words in it.
            .flex_none()
            .w_auto()
            .items_center()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(colors.border)
            // The cells paint to the frame's inner edge, so the frame has to clip them or their
            // square corners show through its curve.
            .overflow_hidden()
            .children(
                cells
                    .into_iter()
                    .enumerate()
                    .map(move |(i, (value, label))| {
                        let is_selected = selected.as_ref() == Some(&value);
                        let handler = on_select.clone();
                        h_flex()
                            .id(SharedString::from(format!("{id}-{value}")))
                            .h_full()
                            .px(GAP_MD)
                            .items_center()
                            .cursor_pointer()
                            // A divider before every cell but the first, so the group reads as one
                            // control rather than as a row of buttons that happen to touch.
                            .when(i != 0, |el| el.border_l_1().border_color(colors.border))
                            .when(is_selected, |el| {
                                el.bg(colors.primary).text_color(colors.primary_foreground)
                            })
                            .when(!is_selected, |el| {
                                el.bg(colors.table_head)
                                    .hover(|el| el.bg(colors.list_hover))
                            })
                            .child(Label::new(label).text_sm())
                            .on_click(move |_, window, cx| {
                                if let Some(handler) = &handler {
                                    handler(&value, window, cx);
                                }
                            })
                    }),
            )
    }
}
