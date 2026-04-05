mod app_menus;
mod settings;
mod workspace;

use gpui::*;
use gpui_component::{Root, TitleBar, v_flex};
use window_wrapper::{
    status_bar::{StatusBar, StatusBarRegistry},
    title_bar::AppTitleBar,
};

use crate::app_menus::{app_menus, OpenSettings};
use crate::settings::{AppSettings, SettingsWindow};
use crate::workspace::Workspace;

pub struct App {
    workspace: Entity<Workspace>,
}

impl App {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let workspace = cx.new(|cx| Workspace::new(window, cx));
        Self { workspace }
    }
}

impl Render for App {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .child(AppTitleBar::new(cx))
            .child(
                div()
                    .id("window-body")
                    .w_full()
                    .flex_1()
                    .child(self.workspace.clone()),
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

        cx.set_global(AppSettings::default());

        let mut registry = StatusBarRegistry::new();
        registry.add_right(cx.new(|_| WindowBoundsDebug));
        cx.set_global(registry);

        cx.set_menus(app_menus());

        // Handle OpenSettings action globally
        cx.on_action(|_: &OpenSettings, cx| {
            let bounds = Bounds::centered(
                None,
                size(px(1000.0), px(800.0)),
                cx,
            );

            let window_options = WindowOptions {
                titlebar: Some(TitleBar::title_bar_options()),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(Size::new(px(600.0), px(400.0))),
                ..Default::default()
            };

            cx.spawn(async move |cx| {
                cx.open_window(window_options, |window, cx| {
                    let view = cx.new(|cx| SettingsWindow::new(window, cx));
                    cx.new(|cx| Root::new(view, window, cx))
                })
                .ok();
            })
            .detach();
        });
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
            titlebar: Some(TitleBar::title_bar_options()),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(min_size),
            ..Default::default()
        };

        cx.spawn(async move |cx| {
            cx.open_window(window_options, |window, cx| {
                let view = cx.new(|cx| App::new(window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("Failed to open window");
        })
        .detach();
    });
}
