use gpui::*;

pub fn status_bar(
    left: impl Into<SharedString>,
    right: impl Into<SharedString>,
) -> impl IntoElement {
    let left = left.into();
    let right = right.into();

    gpui_component::h_flex()
        .id("status-bar")
        .w_full()
        .px_3()
        .py_1()
        .justify_between()
        .border_t_1()
        .child(left)
        .child(right)
}