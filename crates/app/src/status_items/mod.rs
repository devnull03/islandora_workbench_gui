//! All items that are displayed in the status bar
//! registered in this file, through [build_status_bar_registry] function

pub mod ping;

use std::path::PathBuf;

use gpui::*;
use gpui_component::{IconName, Sizable, button::Button};
use settings::{AppSettings, load_app_settings};
use window_wrapper::status_bar::StatusBarRegistry;

use self::ping::ServerPingIndicator;
use crate::helpers;

pub use ping::PingLogEvent;

pub fn build_status_bar_registry(
    ping: Entity<ServerPingIndicator>,
    cx: &mut App,
) -> StatusBarRegistry {
    let mut registry = StatusBarRegistry::new();

    registry.add_left(ping);

    // registry.add_right(cx.new(|_| WindowBoundsDebug));
    registry.add_right(cx.new(|_| ReloadConfigs));
    registry.add_right(cx.new(|_| OpenTerminal));

    registry
}

pub struct OpenTerminal;

impl OpenTerminal {
    fn spawn_terminal(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = AppSettings::get(cx)
            .values
            .get("workbench_path")
            .map(|v| v.text())
            .filter(|s| !s.trim().is_empty())
            .map(|s| PathBuf::from(s.trim()))
        else {
            return;
        };

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
                .px_0()
                .cursor_pointer()
                .on_click(cx.listener(|this, _, window, cx| {
                    println!("terminal button clicked");
                    this.spawn_terminal(window, cx);
                })),
        )
    }
}

pub struct ReloadConfigs;

impl Render for ReloadConfigs {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().child(
            Button::new("reload-configs")
                .icon(IconName::Redo2)
                .label("Reload")
                .small()
                .px_0()
                .cursor_pointer()
                .tooltip("Reload settings and task configs from disk")
                .on_click(cx.listener(|_, _, _, cx| {
                    let current_bounds = AppSettings::get(cx).main_window_bounds.clone();
                    cx.spawn(async move |_this, cx| {
                        let loaded = cx
                            .background_executor()
                            .spawn(async move { load_app_settings().unwrap_or_default() })
                            .await;
                        cx.update(|cx| {
                            let mut new_settings = loaded;
                            new_settings.main_window_bounds = current_bounds;
                            cx.set_global(new_settings);
                        })
                        .ok();
                    })
                    .detach();
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
