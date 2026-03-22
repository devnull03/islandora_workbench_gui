mod app_menus;

use gpui::*;
use gpui_component::{
    Root, TitleBar,
    button::{Button, ButtonVariants},
    v_flex,
};
use window_wrapper::{
    status_bar::{StatusBar, StatusBarRegistry},
    title_bar::AppTitleBar,
};

use crate::app_menus::app_menus;

pub struct Example {}

impl Example {
    pub fn new() -> Self {
        Self {}
    }
}

impl Render for Example {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .child(AppTitleBar::new(cx))
            .child(
                div()
                    .id("window-body")
                    .p_5()
                    .w_full()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .child(
                        Button::new("uhh")
                            .primary()
                            .label("test")
                            .on_click(|_, _, _| {
                                // if let Some(registry) = cx.try_global::<StatusBarRegistry>() {
                                //     registry.add_left(cx.new_view(|_| DynamicItem {
                                //         text: "Added Later!".into(),
                                //     }));
                                // }
                                println!("helo");
                            }),
                    ),
            )
            .child(cx.new(|_| StatusBar::new()))
    }
}

struct WindowBoundsDebug;
impl Render for WindowBoundsDebug {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let width = window.bounds().size.width;
        let height = window.bounds().size.height;
        div().child(format!("{width} x {height}"))
    }
}

fn main() {
    let app = Application::new().with_assets(gpui_component_assets::Assets);

    app.run(move |cx| {
        gpui_component::init(cx);

        let mut registry = StatusBarRegistry::new();
        registry.add_right(cx.new(|_| WindowBoundsDebug));
        cx.set_global(registry);

        cx.set_menus(app_menus());
        const WINDOW_SIZE_MULTIPLIER: f32 = 2.0;
        let bounds = Bounds::centered(
            None,
            size(
                px(300.0 * WINDOW_SIZE_MULTIPLIER),
                px(400.0 * WINDOW_SIZE_MULTIPLIER),
            ),
            cx,
        );

        let min_size = Size::new(px(520.0), px(300.0));

        let window_options = WindowOptions {
            // Setup GPUI to use custom title bar
            titlebar: Some(TitleBar::title_bar_options()),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(min_size),
            ..Default::default()
        };

        cx.spawn(async move |cx| {
            cx.open_window(window_options, |window, cx| {
                let view = cx.new(|_| Example::new());
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("Failed to open window");
        })
        .detach();
    });
}
