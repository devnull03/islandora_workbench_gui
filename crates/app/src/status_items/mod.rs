//! All items that are displayed in the status bar
//! registered in this file, through [build_status_bar_registry] function

pub mod ping;

use gpui::*;
use gpui_component::{IconName, button::Button};
use window_wrapper::status_bar::StatusBarRegistry;

use crate::{status_items::ping::ServerPingIndicator};

pub fn build_status_bar_registry(cx: &mut App) -> StatusBarRegistry {
    let mut registry = StatusBarRegistry::new();

    registry.add_left(cx.new(|cx| ServerPingIndicator::new(cx)));
    
    // registry.add_right(cx.new(|_| WindowBoundsDebug));
    registry.add_right(cx.new(|_| OpenTerminal));
    
    registry
}

pub struct OpenTerminal;
impl Render for OpenTerminal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().child(
            Button::new("Open Terminal")
                .icon(IconName::SquareTerminal)
                .label("Open Terminal")
                .cursor_pointer()
                .on_click(cx.listener(|_, _, _window, cx| {
                    todo!()
                })),
        )
    }
}

pub struct WindowBoundsDebug;
impl Render for WindowBoundsDebug {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let width = window.bounds().size.width;
        let height = window.bounds().size.height;
        div().child(format!("{width} x {height}"))
    }
}
