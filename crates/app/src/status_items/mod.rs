//! All items that are displayed in the status bar
//! registered in this file, through [build_status_bar_registry] function

pub mod ping;

use std::path::PathBuf;

use gpui::*;
use gpui_component::{IconName, Sizable, button::Button};
use settings::AppSettings;
use window_wrapper::status_bar::StatusBarRegistry;

use crate::{helpers, status_items::ping::ServerPingIndicator};

pub fn build_status_bar_registry(cx: &mut App) -> StatusBarRegistry {
    let mut registry = StatusBarRegistry::new();

    registry.add_left(cx.new(|cx| ServerPingIndicator::new(cx)));
    
    // registry.add_right(cx.new(|_| WindowBoundsDebug));
    registry.add_right(cx.new(|_| OpenTerminal));
    
    registry
}

pub struct OpenTerminal;

impl OpenTerminal {
    fn spawn_terminal(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let path = AppSettings::get(cx)
            .values
            .get("workbench_path")
            .map(|v| PathBuf::from(v.text().as_ref()))
            .unwrap_or_default();

        if path.as_os_str().is_empty() {
            return;
        }

        println!("spawning terminal at: {:?}", path);
        
        cx.spawn(async move |_this, _cx| {
            let _ = helpers::spawn_terminal_at(&path, "");
        })
        .detach();
    }
}

impl Render for OpenTerminal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().child(
            Button::new("Open Terminal")
                .icon(IconName::SquareTerminal)
                .label("Open Terminal")
                .small()
                .cursor_pointer()
                .on_click(cx.listener(|this, _, window, cx| {
                    println!("terminal button clicked");
                    this.spawn_terminal(window, cx);
                })),
        )
    }
}

// pub struct WindowBoundsDebug;
// impl Render for WindowBoundsDebug {
//     fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
//         let width = window.bounds().size.width;
//         let height = window.bounds().size.height;
//         div().child(format!("{width} x {height}"))
//     }
// }
