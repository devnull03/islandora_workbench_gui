//! A short enum as a row of buttons rather than a dropdown.
//!
//! Up to four options; past that the row is wider than the dropdown it replaced and the design
//! language says to use a select instead. The win is one click instead of two, and every legal
//! value visible without opening anything.
//!
//! Stateless on purpose: the selected value is passed in and the click is a callback, so this
//! adds nothing to whatever already owns the value. A control that had its own state entity would
//! be a second place for the answer to live.

use gpui::*;
use gpui_component::{Selectable, Sizable as _, button::Button, h_flex};

use crate::APP_CONTROL_SIZE;
use crate::tokens::GAP_SM;

/// Above this many options, use a dropdown — see the module docs.
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
}

impl RenderOnce for Segmented {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let on_select = self.on_select.map(std::rc::Rc::new);
        let selected = self.selected;
        let id = self.id;

        h_flex()
            .gap(GAP_SM)
            .children(self.options.into_iter().map(move |(value, label)| {
                let is_selected = selected.as_ref() == Some(&value);
                let handler = on_select.clone();
                Button::new(SharedString::from(format!("{id}-{value}")))
                    .label(label)
                    .with_size(APP_CONTROL_SIZE)
                    .outline()
                    .selected(is_selected)
                    .on_click(move |_, window, cx| {
                        if let Some(handler) = &handler {
                            handler(&value, window, cx);
                        }
                    })
            }))
    }
}
