// On Windows, suppress the console window for release builds (the GUI is the only
// surface users want). Debug builds keep the console so stdout/logs stay visible.
// The attribute is a no-op on non-Windows targets.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod app_menus;
mod app_settings;
mod dock;
mod helpers;
mod logging;
mod status_items;
mod theming;
mod update_check;
mod workspace;

use gpui::*;
use gpui_component::{Root, TitleBar, menu::AppMenuBar, v_flex};
use settings::{
    AppSettings, MainWindowBounds, SettingsPersistence, SettingsWindow, SettingsWindowHandle,
    SettingsWriter, load_app_settings,
};
use ui::AppFont as _;
use window_wrapper::{
    BarRegistry as _, OpenBrowser, WindowLock,
    status_bar::StatusBar,
    title_bar::{AppTitleBar, TitleBarRegistry},
};

use crate::{
    app_menus::{
        CloseWindow, CopyDebugInfo, OpenLogsFolder, OpenProfiles, OpenSettings, Quit, REPO_URL,
        ReportIssue, ToggleLog, app_menus,
    },
    app_settings::build_pages,
    dock::{LogDockButton, MainDock},
    status_items::{PingLogEvent, build_status_bar_registry, ping::ServerPingIndicator},
    workspace::{LogViewer, Workspace},
};
use config_builder::{
    BuilderDock, ConfigBuilderWindows, OpenConfigBuilder, ToggleBuilderDock, close_builder_window,
    install_builder_dock, open_config_builder,
};

pub struct App {
    dock: Entity<MainDock>,
    status_bar: Entity<StatusBar>,
    focus_handle: FocusHandle,
    _main_window_bounds_sub: Subscription,
    _ping_log_sub: Subscription,
    _window_lock_sub: Subscription,
    _release: Subscription,
}

impl App {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // The log outlives the views that write to it: the workspace appends run output, the
        // bottom dock panel shows it, and the ping indicator files server status into it. Built
        // here so all three hold the same one.
        let log_viewer = cx.new(LogViewer::new);
        let workspace = cx.new(|cx| Workspace::new(log_viewer.clone(), window, cx));
        let dock = cx.new(|cx| MainDock::new(workspace, log_viewer.clone(), window, cx));
        let builder_pane = dock.read(cx).builder_pane().clone();
        install_builder_dock(window.window_handle(), &builder_pane, cx);
        let status_bar = cx.new(|_| StatusBar);

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

        let mut title_registry = TitleBarRegistry::default();
        title_registry
            .items_mut()
            .add_left(AppMenuBar::new(&mut *cx));
        cx.set_global(title_registry);

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

        // Block the OS close button while an ingest run is in progress, and ask before a docked
        // builder's unsaved edits go with the window (plan 5, accept).
        window.on_window_should_close(cx, |window, cx| {
            may_close(window, cx, |window, _| window.remove_window())
        });
        // Closing however it happens: the docked builder and the dock's hook go with the window.
        let _release = cx.on_release(|_, cx| BuilderDock::uninstall(cx));

        let focus_handle = cx.focus_handle();
        if window.focused(cx).is_none() {
            focus_handle.focus(window, cx);
        }

        Self {
            dock,
            status_bar,
            focus_handle,
            _main_window_bounds_sub,
            _ping_log_sub,
            _window_lock_sub,
            _release,
        }
    }
}

impl Render for App {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dialog_layer = Root::render_dialog_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);

        div()
            .id("workbench-root")
            .app_font(cx)
            .key_context("IslandoraWorkbench")
            .track_focus(&self.focus_handle)
            .role(Role::Group)
            .size_full()
            .on_action(cx.listener(|this, _: &ToggleLog, window, cx| {
                this.dock.update(cx, |dock, cx| dock.toggle_log(window, cx));
            }))
            // Reached only when focus is outside the docked builder, which handles it itself.
            .on_action(cx.listener(|this, _: &ToggleBuilderDock, _, cx| {
                let pane = this.dock.read(cx).builder_pane().clone();
                pane.update(cx, |pane, cx| pane.toggle(cx));
            }))
            .child(
                v_flex()
                    .size_full()
                    .child(
                        AppTitleBar::new("Islandora Workbench").on_close(|window, cx| {
                            if may_close(window, cx, |window, _| window.remove_window()) {
                                window.remove_window();
                            }
                        }),
                    )
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
            .children(notification_layer)
    }
}

/// Whether the main window may close (or the app quit) now. While a run holds the window
/// lock, never; with unsaved edits in the docked builder, not until the user agrees — then
/// `proceed` runs. `true` means nothing stands in the way and the caller goes ahead.
fn may_close(
    window: &mut Window,
    cx: &mut gpui::App,
    proceed: impl Fn(&mut Window, &mut gpui::App) + 'static,
) -> bool {
    if WindowLock::blocks_close(cx) {
        use gpui_component::WindowExt as _;
        window.push_notification(
            gpui_component::notification::Notification::warning(
                "A run is in progress. Cancel it before closing.",
            ),
            cx,
        );
        return false;
    }
    let Some((_, pane)) = BuilderDock::main(cx) else {
        return true;
    };
    if !pane.read(cx).has_unsaved_changes(cx) {
        return true;
    }
    ui::confirm_dialog(
        window,
        cx,
        "Close with unsaved builder edits?",
        "The docked Config Builder has edits that are not saved. Closing discards them.",
        "Discard and close",
        move |window, cx| {
            pane.update(cx, |pane, cx| pane.discard_docked(cx));
            proceed(window, cx);
        },
    );
    false
}

/// Open Settings on `page`, or bring the open Settings window forward (it keeps its page).
fn open_settings(page: usize, cx: &mut gpui::App) {
    if let Some(handle) = cx.global::<SettingsWindowHandle>().handle
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }
    cx.global_mut::<SettingsWindowHandle>().handle = None;
    let (bounds, min_size) = SettingsWindow::startup_placement(cx);
    let window_options = WindowOptions {
        titlebar: Some(TitleBar::title_bar_options()),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: min_size,
        ..Default::default()
    };
    cx.spawn(async move |cx| {
        let result = cx.open_window(window_options, |window, cx| {
            let view = cx.new(|cx| {
                let mut settings = SettingsWindow::new(window, cx, build_pages);
                settings.initial_page = page;
                settings
            });
            cx.new(|cx| Root::new(view, window, cx))
        });
        if let Ok(window_handle) = result {
            cx.update(|cx| {
                cx.global_mut::<SettingsWindowHandle>().handle = Some(window_handle.into());
            });
        }
    })
    .detach();
}

/// Quit, unless a run is going (say so, and stay) or a builder — docked or in its own window —
/// has unsaved edits (ask once, in a window the user can see).
fn request_quit(cx: &mut gpui::App) {
    use gpui_component::WindowExt as _;
    let main = BuilderDock::main(cx).map(|(window, _)| window);
    if WindowLock::blocks_close(cx) {
        if let Some(main) = main {
            let _ = main.update(cx, |_, window, cx| {
                window.activate_window();
                window.push_notification(
                    gpui_component::notification::Notification::warning(
                        "A run is in progress. Cancel it before quitting.",
                    ),
                    cx,
                );
            });
        }
        return;
    }
    let (unsaved, builder_window) = cx.global::<ConfigBuilderWindows>().unsaved(cx);
    if unsaved == 0 {
        cx.quit();
        return;
    }
    let Some(ask_in) = main.or(builder_window) else {
        cx.quit();
        return;
    };
    let asked = ask_in.update(cx, |_, window, cx| {
        window.activate_window();
        ui::confirm_dialog(
            window,
            cx,
            "Quit with unsaved builder edits?",
            if unsaved == 1 {
                "A Config Builder has edits that are not saved. Quitting discards them.".to_string()
            } else {
                format!("{unsaved} Config Builders have edits that are not saved. Quitting discards them.")
            },
            "Discard and quit",
            |_, cx| {
                ConfigBuilderWindows::forget_unsaved(cx);
                cx.quit();
            },
        );
    });
    if asked.is_err() {
        log::warn!("couldn't ask about unsaved builder edits; not quitting");
    }
}

fn main() {
    // First, so a failure during startup — the exact case where no window appears to report it —
    // still reaches the file.
    logging::init();

    let app = gpui_platform::application().with_assets(gpui_component_assets::Assets);
    // macOS keeps an app running with its windows closed; clicking it in the Dock brings the
    // main window back.
    app.on_reopen(|cx| {
        if BuilderDock::main(cx).is_none() {
            open_main_window(cx);
        }
    });

    app.run(move |cx| {
        gpui_component::init(cx);
        ui::theme::init(cx);

        cx.set_global(WindowLock::default());

        // Settings ------------------------------------
        let settings = load_app_settings().unwrap_or_default();
        cx.set_global(settings);
        cx.set_global(SettingsPersistence {
            writer: Some(SettingsWriter::start()),
        });
        cx.set_global(SettingsWindowHandle::default());
        cx.set_global(ConfigBuilderWindows::default());

        theming::init(cx);

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

        cx.on_action(|_: &OpenSettings, cx| open_settings(0, cx));
        // "Manage profiles": the same window, opened on its last page (Profiles).
        cx.on_action(|_: &OpenProfiles, cx| {
            let last = build_pages(cx).len().saturating_sub(1);
            open_settings(last, cx);
        });
        // ----------------------------------------------

        let menus = app_menus();
        cx.set_menus(menus);
        let owned = app_menus().into_iter().map(|menu| menu.owned()).collect();
        gpui_component::GlobalState::global_mut(cx).set_app_menus(owned);

        cx.bind_keys(actions::key_bindings());

        cx.on_action(|action: &OpenBrowser, cx| {
            cx.open_url(&action.url);
        });

        // Ctrl+\ when no focused view handled it (nothing focused after a pop out, say).
        cx.on_action(|_: &ToggleBuilderDock, cx| {
            if let Some((_, pane)) = BuilderDock::main(cx) {
                pane.update(cx, |pane, cx| pane.toggle(cx));
            }
        });
        // Quit asks the same questions as closing — deferred, because a menu dispatch runs while
        // the window it came from is being updated, and that window cannot be updated again.
        cx.on_action(|_: &Quit, cx| cx.defer(request_quit));
        // Cmd+W: whichever window is in front, with the same questions as its close button.
        cx.on_action(|_: &CloseWindow, cx| cx.defer(close_active_window));

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
        cx.on_action(|_: &OpenLogsFolder, _| match logging::reveal_target() {
            Some(target) => {
                if let Err(err) = helpers::reveal_in_folder(&target) {
                    log::error!("failed to reveal the log folder: {err}");
                }
            }
            None => log::error!("no local data dir, so there is no log folder to open"),
        });
        open_main_window(cx);
    });
}

/// Open the main window where it was last left.
fn open_main_window(cx: &mut gpui::App) {
    let (main_bounds, main_display) = AppSettings::get(cx).main_window_startup_placement(cx);
    let min_size = Size::new(ui::tokens::MIN_WINDOW_W, px(300.0));
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
}

/// Close the window in front as its close button would: the main window and builders ask
/// about unsaved edits (and the main window refuses during a run); others just close.
fn close_active_window(cx: &mut gpui::App) {
    let main = BuilderDock::main(cx).map(|(window, _)| window);
    // No active window when no window has focus (a menu that took it, a bare X server).
    let Some(active) = cx.active_window().or(main) else {
        return;
    };
    if close_builder_window(active, cx) {
        return;
    }
    let _ = active.update(cx, |_, window, cx| {
        if main != Some(active) || may_close(window, cx, |window, _| window.remove_window()) {
            window.remove_window();
        }
    });
}
