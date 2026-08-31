//! An icon-and-label affordance in the bottom status strip.
//!
//! Small enough to look like a label until hovered, which is the point: the status bar reports
//! state, and these are the few entries that also do something. Written out twice by hand before
//! this existed, and both copies had drifted onto `rounded_md()` instead of the theme radius.

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{ActiveTheme, Icon, IconName, Sizable, h_flex, tooltip::Tooltip};

type ClickFn = Box<dyn Fn(&mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct StatusBarButton {
    id: ElementId,
    icon: IconName,
    label: SharedString,
    tooltip: SharedString,
    /// Shown as a keybinding hint beside the tooltip text, when the affordance has an action.
    action: Option<Box<dyn Action>>,
    on_click: Option<ClickFn>,
}

impl StatusBarButton {
    pub fn new(id: impl Into<ElementId>, icon: IconName, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            icon,
            label: label.into(),
            tooltip: SharedString::default(),
            action: None,
            on_click: None,
        }
    }

    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = tooltip.into();
        self
    }

    pub fn action(mut self, action: impl Action) -> Self {
        self.action = Some(Box::new(action));
        self
    }

    pub fn on_click(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for StatusBarButton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let hover_bg = cx.theme().secondary_hover;
        let hover_fg = cx.theme().primary;
        let tooltip = self.tooltip.clone();
        let action = self.action;
        let on_click = self.on_click;

        h_flex()
            .id(self.id)
            .gap_1()
            .px(px(4.))
            .py(px(2.))
            .rounded(cx.theme().radius)
            .cursor_pointer()
            .hover(move |this| this.bg(hover_bg).text_color(hover_fg))
            .child(Icon::new(self.icon).small())
            .child(self.label)
            .tooltip(move |window, cx| {
                let tip = Tooltip::new(tooltip.clone());
                match &action {
                    Some(a) => tip.action(&**a, None),
                    None => tip,
                }
                .build(window, cx)
            })
            .when_some(on_click, |el, handler| {
                el.on_click(move |_, window, cx| handler(window, cx))
            })
    }
}
