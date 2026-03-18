mod app_menus;

use gpui::*;
use gpui_component::{
    Root, TitleBar,
    button::{Button, ButtonVariants},
    v_flex,
};
use window_wrapper::{status_bar::status_bar, title_bar::AppTitleBar};

pub struct Example {
    menus: Vec<OwnedMenu>,
}

impl Example {
    pub fn new() -> Self {
        Self {
            menus: vec![app_menus::app_menus().owned()],
        }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .child(AppTitleBar::with_owned("App".to_string(), self.menus.clone()).build())
            .child(
                div()
                    .id("window-body")
                    .p_5()
                    .w_full()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .child("Hello, World!")
                    .child(
                        Button::new("ok")
                            .primary()
                            .label("Let's Go!")
                            .on_click(|_, _, _| println!("Clicked!")),
                    ),
            )
            .child(status_bar("Ready", "Ln 1, Col 1"))
    }
}

fn main() {
    let app = Application::new().with_assets(gpui_component_assets::Assets);

    app.run(move |cx| {
        gpui_component::init(cx);

        cx.spawn(async move |cx| {
            let window_options = WindowOptions {
                // Setup GPUI to use custom title bar
                titlebar: Some(TitleBar::title_bar_options()),
                ..Default::default()
            };

            cx.open_window(window_options, |window, cx| {
                let view = cx.new(|_| Example::new());
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("Failed to open window");
        })
        .detach();
    });
}
