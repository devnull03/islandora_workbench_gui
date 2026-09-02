//! A set of short values as removable chips, with an inline slot to add another.
//!
//! Component Spec §05. The rule for when to reach for this is in the spec and worth repeating,
//! because it is the whole distinction: **chips imply order does not matter.** `ignore_csv_columns`
//! is a set of column names and gets chips; `csv_field_templates` is an ordered list of rules and
//! gets a [`crate::RowEditor`]. Rendering an ordered list as chips tells the user a lie about the
//! value they are editing.
//!
//! Stateless about the set itself: the chips are handed in and removal is a callback, so the
//! draft stays the only place the answer lives. The one piece of state is
//! the add slot's own input, which belongs to the caller for the same reason every other input in
//! the builder does — it is cached by field id and survives a re-render.

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, Sizable as _, h_flex,
    input::{Input, InputState},
    label::Label,
};

use crate::tokens::{CHIP_H, GAP_SM, RADIUS_SM};
use crate::{APP_CONTROL_SIZE_SM, app_tag};

type RemoveFn = Box<dyn Fn(usize, &mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct ChipList {
    id: SharedString,
    chips: Vec<SharedString>,
    /// The trailing "+ Add" slot. `None` renders the set read-only, which is what a chip row
    /// inside a disabled section wants.
    add: Option<Entity<InputState>>,
    on_remove: Option<RemoveFn>,
}

impl ChipList {
    pub fn new(id: impl Into<SharedString>, chips: impl IntoIterator<Item = SharedString>) -> Self {
        Self {
            id: id.into(),
            chips: chips.into_iter().collect(),
            add: None,
            on_remove: None,
        }
    }

    /// The input that commits a new chip. The caller owns it, seeds it empty, and clears it when
    /// its change handler has taken the value — this type never writes to it.
    pub fn add_slot(mut self, input: &Entity<InputState>) -> Self {
        self.add = Some(input.clone());
        self
    }

    pub fn on_remove(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_remove = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for ChipList {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors;
        let mono = cx.theme().mono_font_family.clone();
        let on_remove = self.on_remove.map(std::rc::Rc::new);
        let id = self.id;

        h_flex()
            .w_full()
            .gap(GAP_SM)
            .flex_wrap()
            .items_center()
            .children(self.chips.into_iter().enumerate().map({
                let mono = mono.clone();
                move |(i, chip)| {
                    let remove = on_remove.clone();
                    app_tag()
                        .gap(GAP_SM)
                        .flex_none()
                        .bg(colors.table_head)
                        .child(Label::new(chip.clone()).text_xs().font_family(mono.clone()))
                        .when_some(remove, |el, remove| {
                            el.child(
                                div()
                                    .id(SharedString::from(format!("{id}-x-{i}")))
                                    .cursor_pointer()
                                    .text_color(colors.muted_foreground)
                                    // Danger on hover only. A row of red crosses reads as a row
                                    // of errors; the colour belongs to the moment before a
                                    // deletion, not to the resting state.
                                    .hover(|el| el.text_color(colors.danger))
                                    .child(Label::new("✕").text_xs())
                                    .on_click(move |_, window, cx| remove(i, window, cx)),
                            )
                        })
                }
            }))
            .when_some(self.add, |row, input| {
                row.child(
                    // Dashed, so an empty set does not look like it already holds one blank chip.
                    h_flex()
                        .h(CHIP_H)
                        .w(px(96.))
                        .px(GAP_SM)
                        .items_center()
                        .flex_none()
                        .rounded(RADIUS_SM)
                        .border_1()
                        .border_dashed()
                        .border_color(colors.border)
                        .child(
                            Input::new(&input)
                                .with_size(APP_CONTROL_SIZE_SM)
                                .appearance(false)
                                .w_full(),
                        ),
                )
            })
    }
}
