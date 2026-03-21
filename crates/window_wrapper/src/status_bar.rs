use gpui::*;
use gpui_component::ActiveTheme as _;


pub struct StatusBar<V: 'static> {
    left: SharedString,
    right: SharedString,
}

pub fn status_bar<V: 'static>(
    left: impl Into<SharedString>,
    right: impl Into<SharedString>,
    _: &mut Window,
    cx: &mut Context<V>,
) -> impl IntoElement {
    let left = left.into();
    let right = right.into();

    gpui_component::h_flex()
        .id("status-bar")
        .bg(cx.theme().title_bar)
        .w_full()
        .px_3()
        .py_1()
        .justify_between()
        .border_t_1()
        .border_color(cx.theme().title_bar_border)
        .child(left)
        .child(right)
}
