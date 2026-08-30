// On Windows, suppress the console window for release builds (the GUI is the only
// surface users want). Debug builds keep the console so stdout/logs stay visible.
// The attribute is a no-op on non-Windows targets.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_menus;
mod app_settings;
mod dock;
mod helpers;
mod logging;
mod status_items;
mod update_check;
mod workspace;

use gpui::*;
use gpui_component::{Root, TitleBar, v_flex};
use settings::{
    AppSettings, MainWindowBounds, SettingsPersistence, SettingsWindow, SettingsWindowHandle,
    SettingsWriter, load_app_settings,
};
use window_wrapper::{OpenBrowser, WindowLock, status_bar::StatusBar, title_bar::AppTitleBar};

use crate::{
    app_menus::{
        CheckForUpdates, CopyDebugInfo, OpenLogsFolder, OpenSettings, Quit, REPO_URL, ReportIssue,
        app_menus,
    },
    app_settings::build_pages,
    dock::{LogDockButton, MainDock},
    status_items::{PingLogEvent, build_status_bar_registry, ping::ServerPingIndicator},
    workspace::{LogViewer, Workspace},
};
use config_builder::{ConfigBuilderWindows, OpenConfigBuilder, open_config_builder};

pub struct App {
    dock: Entity<MainDock>,
    status_bar: Entity<StatusBar>,
    _main_window_bounds_sub: Subscription,
    _ping_log_sub: Subscription,
    _window_lock_sub: Subscription,
}

impl App {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // The log outlives the views that write to it: the workspace appends run output, the
        // bottom dock panel shows it, and the ping indicator files server status into it. Built
        // here so all three hold the same one.
        let log_viewer = cx.new(LogViewer::new);
        let workspace = cx.new(|cx| Workspace::new(log_viewer.clone(), window, cx));
        let dock = cx.new(|cx| MainDock::new(workspace, log_viewer.clone(), window, cx));
        let status_bar = cx.new(|_| StatusBar::new());

        let ping = cx.new(ServerPingIndicator::new);
        let _ping_log_sub = cx.subscribe(&ping, {
            let log_viewer = log_viewer.clone();
            move |_, _, event: &PingLogEvent, cx| {
                log_viewer.update(cx, |log, cx| log.append(&event.0, cx));
            }
        });
        let log_button = cx.new({
            let dock_area = dock.read(cx).dock_area();
            |cx| LogDockButton::new(dock_area, cx)
        });
        let registry = build_status_bar_registry(ping, log_button, &mut *cx);
        cx.set_global(registry);

        let _main_window_bounds_sub = cx.observe_window_bounds(window, |_, window, cx| {
            let b = MainWindowBounds::capture_from_window(window, cx);
            AppSettings::update(cx, |s| {
                s.main_window_bounds = Some(b);
            });
        });

        // Re-render this view (and therefore AppTitleBar) whenever WindowLock changes.
        let _window_lock_sub = cx.observe_global::<WindowLock>(|_, cx| {
            cx.notify();
        });

        // Block the OS close button while an ingest run is in progress.
        window.on_window_should_close(cx, |_, cx| !WindowLock::is_locked(cx));

        Self {
            dock,
            status_bar,
            _main_window_bounds_sub,
            _ping_log_sub,
            _window_lock_sub,
        }
    }
}

impl Render for App {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dialog_layer = Root::render_dialog_layer(window, cx);

        div()
            .size_full()
            .child(
                v_flex()
                    .size_full()
                    .child(AppTitleBar::new(cx).title("Islandora Workbench"))
                    .child(
                        // `min_h(0)` is load-bearing: a flex item's min height defaults to its
                        // content, so without it a tall workspace refuses to shrink and shoves
                        // the status bar past the bottom of the viewport. With it the body is
                        // clipped to the space left over and the workspace scrolls internally.
                        div()
                            .id("window-body")
                            .w_full()
                            .flex_1()
                            .min_h(px(0.))
                            .overflow_hidden()
                            .child(self.dock.clone()),
                    )
                    .child(self.status_bar.clone()),
            )
            .children(dialog_layer)
    }
}

fn main() {
    // First, so a failure during startup — the exact case where no window appears to report it —
    // still reaches the file.
    logging::init();

    let app = Application::new().with_assets(gpui_component_assets::Assets);

    app.run(move |cx| {
        gpui_component::init(cx);
        ui::theme::init(cx);

        cx.set_global(WindowLock::default());

        // Settings ------------------------------------
        let settings = load_app_settings().unwrap_or_default();
        cx.set_global(settings);
        let (main_bounds, main_display) = AppSettings::get(cx).main_window_startup_placement(cx);
        cx.set_global(SettingsPersistence {
            writer: Some(SettingsWriter::start()),
        });
        cx.set_global(SettingsWindowHandle::default());
        cx.set_global(ConfigBuilderWindows::default());

        // Written on first launch rather than defaulted silently: the Updates switch reads the
        // stored value, so without this it would sit in the off position while checks were on.
        if !AppSettings::get(cx)
            .values
            .contains_key(update_check::AUTO_UPDATE_KEY)
        {
            AppSettings::set_bool(update_check::AUTO_UPDATE_KEY, true, cx);
        }
        update_check::check_on_startup(cx);

        cx.on_action(|_: &OpenConfigBuilder, cx| open_config_builder(None, cx));

        cx.on_action(|_: &OpenSettings, cx| {
            let state = cx.global::<SettingsWindowHandle>();

            if let Some(handle) = &state.handle {
                if handle.update(cx, |_, _, _| {}).is_ok() {
                    return;
                } else {
                    cx.global_mut::<SettingsWindowHandle>().handle = None;
                }
            }
            let (bounds, min_size) = SettingsWindow::startup_placement(cx);
            let window_options = WindowOptions {
                titlebar: Some(TitleBar::title_bar_options()),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: min_size,
                ..Default::default()
            };

            cx.spawn(async move |cx| {
                let result = cx.open_window(window_options, |window, cx| {
                    let view = cx.new(|cx| SettingsWindow::new(window, cx, build_pages));
                    cx.new(|cx| Root::new(view, window, cx))
                });

                if let Ok(window_handle) = result {
                    cx.update(|cx| {
                        cx.global_mut::<SettingsWindowHandle>().handle = Some(window_handle.into());
                    })
                    .ok();
                }
            })
            .detach();
        });
        // ----------------------------------------------

        cx.set_menus(app_menus());

        cx.on_action(|action: &OpenBrowser, cx| {
            cx.open_url(&action.url);
        });

        cx.on_action(|_: &Quit, cx| {
            cx.quit();
        });

        // Help ▸ … — the report path. Nothing is uploaded: both routes put the text in front of
        // the user (clipboard, or a prefilled issue form) before it leaves the machine.
        cx.on_action(|_: &CopyDebugInfo, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(logging::debug_info(cx, 200)));
        });
        cx.on_action(|_: &ReportIssue, cx| {
            // Far fewer log lines than the clipboard dump: GitHub truncates a prefilled body
            // somewhere past 8 KB, and a truncated report loses its tail — the part that matters.
            let body = logging::urlencode(&logging::debug_info(cx, 30));
            cx.open_url(&format!("{REPO_URL}/issues/new?body={body}"));
        });
        cx.on_action(|_: &CheckForUpdates, cx| cx.open_url(&update_check::releases_url()));
        cx.on_action(|_: &OpenLogsFolder, _| match logging::reveal_target() {
            Some(target) => {
                if let Err(err) = helpers::reveal_in_folder(&target) {
                    log::error!("failed to reveal the log folder: {err}");
                }
            }
            None => log::error!("no local data dir, so there is no log folder to open"),
        });
        let min_size = Size::new(px(520.0), px(300.0));

        let window_options = WindowOptions {
            titlebar: Some(TitleBar::title_bar_options()),
            window_bounds: Some(WindowBounds::Windowed(main_bounds)),
            display_id: main_display,
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
