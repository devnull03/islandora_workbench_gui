use gpui::*;

pub use gpui_component::TitleBar;
crate::title_bar;

pub fn standard_title_bar(
    title: impl Into<SharedString>,
    right_item: impl Into<SharedString>,
) -> impl IntoElement {
    let title = title.into();
    let right_item = right_item.into();

    TitleBar::new().child(
        gpui_component::h_flex()
            .w_full()
            .pr_2()
            .justify_between()
            .child(title)
            .child(right_item),
    )
}

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
