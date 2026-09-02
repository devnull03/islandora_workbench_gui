//! What a dropdown row looks like in this app, and the geometry every dropdown gets.
//!
//! Component Spec §05. The library's `Select` is reused rather than replaced — it owns the
//! popover, the keyboard handling and the menu placement, and none of that is what was wrong.
//! What this adds is the row: a value line over a muted description line, and a mono value line
//! when the value is a literal that lands in YAML.
//!
//! The constructors exist because every one of the app's eight dropdowns used to build this with
//! a struct literal, which is how the config builder ended up putting `create - Create new nodes`
//! on one line — the value spelled twice, in a menu exactly as wide as its trigger, with nothing
//! to truncate it. [`DetailSelectItem::from_choice`] splits that back apart.

use gpui::prelude::FluentBuilder as _;
use gpui::{App, IntoElement, ParentElement, SharedString, Styled, Window, div, px};
use gpui_component::{
    ActiveTheme, Sizable as _,
    select::{Select, SelectItem, SelectState},
    v_flex,
};

use crate::APP_CONTROL_SIZE;

/// §05: past this the menu scrolls rather than growing.
const MENU_MAX_H: f32 = 280.;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetailSelectItem {
    pub label: SharedString,
    pub subtitle: SharedString,
    pub value: SharedString,
    pub divider_above: bool,
    /// The label is a literal — a YAML value, a filename — rather than prose, so it is set in the
    /// mono family (§05: "Plex Mono when it is a bare YAML value").
    pub mono: bool,
}

impl DetailSelectItem {
    /// A row whose label is prose: a server name, a config's title.
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            subtitle: SharedString::default(),
            value: value.into(),
            divider_above: false,
            mono: false,
        }
    }

    /// A row whose label is a literal value.
    pub fn code(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            mono: true,
            ..Self::new(value, label)
        }
    }

    /// One enum choice from the setting catalogue.
    ///
    /// The generator emits the label as `"{value} - {prose}"`, so rendering it whole prints the
    /// value twice and leaves a line too long for a menu the width of its trigger. Splitting on
    /// the separator puts the value on the mono line and the prose underneath, where there is
    /// room for it. A label that is not in that form is used as-is.
    pub fn from_choice(value: impl Into<SharedString>, label: &str) -> Self {
        let value = value.into();
        let prose = label
            .strip_prefix(value.as_ref())
            .and_then(|rest| rest.strip_prefix(" - "))
            .unwrap_or("");
        Self::code(value.clone(), value).subtitle(prose)
    }

    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = subtitle.into();
        self
    }

    pub fn divider_above(mut self, divider: bool) -> Self {
        self.divider_above = divider;
        self
    }
}

/// A dropdown at the app's control size, with §05's menu geometry.
///
/// The menu's width is the library's `Length::Auto`, which is already the trigger's width — the
/// one thing §05 asks for that the library does not do by default is the height cap.
pub fn app_select<D>(state: &gpui::Entity<SelectState<D>>) -> Select<D>
where
    D: gpui_component::select::SelectDelegate + 'static,
{
    Select::new(state)
        .with_size(APP_CONTROL_SIZE)
        .menu_max_h(px(MENU_MAX_H))
}

impl SelectItem for DetailSelectItem {
    type Value = SharedString;

    fn title(&self) -> SharedString {
        self.label.clone()
    }
    fn value(&self) -> &Self::Value {
        &self.value
    }

    fn render(&self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let mono = cx.theme().mono_font_family.clone();
        let body = v_flex()
            .w_full()
            .min_w(px(0.))
            .gap_0p5()
            .py_0p5()
            .child(
                div()
                    .w_full()
                    .min_w(px(0.))
                    .truncate()
                    .text_color(cx.theme().foreground)
                    .when(self.mono, |el| el.font_family(mono))
                    .child(self.label.clone()),
            )
            .when(!self.subtitle.is_empty(), |el| {
                el.child(
                    div()
                        .w_full()
                        .min_w(px(0.))
                        // The description is the line that overruns, and a menu is exactly as
                        // wide as its trigger. Truncating is what keeps it inside.
                        .truncate()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(self.subtitle.clone()),
                )
            });

        if self.divider_above {
            v_flex()
                .w_full()
                .child(div().h(px(1.)).bg(cx.theme().colors.border))
                .child(body)
        } else {
            body
        }
    }

    fn matches(&self, query: &str) -> bool {
        let q = query.to_lowercase();
        self.label.to_lowercase().contains(&q) || self.subtitle.to_lowercase().contains(&q)
    }
}

#[cfg(test)]
mod tests {
    use super::DetailSelectItem;

    /// The catalogue emits `"{value} - {prose}"` as one label. Rendering that whole is how the
    /// config builder ended up printing `create - Create new nodes` in a menu the width of its
    /// trigger, with the value spelled twice and no room for either half.
    #[test]
    fn a_catalogue_choice_splits_into_a_value_and_a_description() {
        let item = DetailSelectItem::from_choice("create", "create - Create new nodes");
        assert_eq!(item.label, "create");
        assert_eq!(item.subtitle, "Create new nodes");
        assert!(item.mono, "a YAML value is set in the mono family");
    }

    /// A label that is not in that form is a label, not a mis-parse waiting to happen.
    #[test]
    fn a_label_that_is_not_prefixed_by_its_value_is_left_alone() {
        let item = DetailSelectItem::from_choice("md5", "md5");
        assert_eq!(item.label, "md5");
        assert_eq!(item.subtitle, "");

        // The separator has to be exactly " - "; a value that merely starts the label is not it.
        let item = DetailSelectItem::from_choice("update", "update_media - Update media");
        assert_eq!(item.subtitle, "", "no false split on a prefix collision");
    }
}
