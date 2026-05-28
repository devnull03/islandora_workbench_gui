use gpui::*;
use gpui_component::{
    ActiveTheme,
    label::Label,
    select::{Select, SelectState},
    v_flex,
};

use super::select_items::DetailSelectItem;

#[derive(IntoElement)]
pub struct LabeledSelect {
    label: SharedString,
    description: SharedString,
    select: Entity<SelectState<Vec<DetailSelectItem>>>,
    placeholder: SharedString,
    disabled: bool,
}

impl LabeledSelect {
    pub fn new(
        label: impl Into<SharedString>,
        select: &Entity<SelectState<Vec<DetailSelectItem>>>,
    ) -> Self {
        Self {
            label: label.into(),
            description: SharedString::default(),
            select: select.clone(),
            placeholder: SharedString::default(),
            disabled: false,
        }
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = description.into();
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for LabeledSelect {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        v_flex()
            .flex_1()
            .min_w(px(0.))
            .gap_1()
            .child(Label::new(self.label).text_sm())
            .child(
                Select::new(&self.select)
                    .placeholder(self.placeholder)
                    .disabled(self.disabled)
                    .w_full(),
            )
            .child(
                Label::new(self.description)
                    .text_sm()
                    .text_color(cx.theme().muted_foreground),
            )
    }
}
