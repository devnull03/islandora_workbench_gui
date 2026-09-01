//! All items that are displayed in the status bar
//! registered in this file, through [build_status_bar_registry] function

pub mod ping;

use std::path::PathBuf;

use gpui::*;
use gpui_component::{Disableable, IconName};
use settings::{AppSettings, load_app_settings};
use window_wrapper::{BarRegistry as _, status_bar::StatusBarRegistry};

use self::ping::ServerPingIndicator;
use crate::dock::LogDockButton;
use crate::helpers;
use crate::update_check::UpdateIndicator;

pub use ping::PingLogEvent;

pub fn build_status_bar_registry(
    ping: Entity<ServerPingIndicator>,
    log_button: Entity<LogDockButton>,
    cx: &mut App,
) -> StatusBarRegistry {
    let mut registry = StatusBarRegistry::new();

    registry.add_left(ping);
    // Centred, under the dock it opens — the same rule qrate's bar buttons follow.
    registry.items_mut().add_centre_if(log_button, |_| true);

    // Silent until the startup check finds something, so it declares its own occupancy rather
    // than stranding a divider beside an empty slot.
    registry
        .items_mut()
        .add_right_if(cx.new(UpdateIndicator::new), |cx: &App| {
            UpdateIndicator::occupied(cx)
        });
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

        log::debug!("spawning terminal at {path:?}");

        cx.spawn(async move |_this, _cx| {
            let _ = helpers::spawn_terminal_at(&path, "");
        })
        .detach();
    }
}

impl Render for OpenTerminal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let configured = AppSettings::get(cx)
            .values
            .get("workbench_path")
            .is_some_and(|value| !value.text().trim().is_empty());
        div().child(
            ui::ghost_button("Open Terminal")
                .icon(IconName::SquareTerminal)
                .label("Open Terminal")
                .px_0()
                .cursor_pointer()
                .disabled(!configured)
                .tooltip(if configured {
                    "Open a terminal in the Workbench folder"
                } else {
                    "Set the Workbench path in Settings first"
                })
                .on_click(cx.listener(|this, _, window, cx| {
                    log::debug!("terminal button clicked");
                    this.spawn_terminal(window, cx);
                })),
        )
    }
}

pub struct ReloadConfigs;

impl Render for ReloadConfigs {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().child(
            ui::ghost_button("reload-configs")
                .icon(IconName::Redo2)
                .label("Reload")
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
                        });
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
