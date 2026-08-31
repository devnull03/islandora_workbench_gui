//! The window body as a `gpui_component::dock::DockArea`: the workbench steps in the centre, the
//! run log in a bottom dock, and the status-bar button that opens and closes it.
//!
//! One dock, deliberately. qrate's version carries a panel registry because its panels *move*
//! between three docks and its bar buttons have to follow them; with a single panel in a single
//! dock there is nothing to track — the button toggles the bottom dock and that is the whole model.

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _,
    dock::{
        DockArea, DockAreaState, DockEvent, DockLayout, DockPlacement, DockSkin, panel_handle,
        register_panel,
    },
    h_flex,
};
use settings::AppSettings;

use crate::{
    app_menus::ToggleLog,
    workspace::{LogViewer, Workspace},
};

/// Settings key under which the serialized [`DockAreaState`] is persisted, so the log pane reopens
/// at the height it was left and stays shut if that is how the last session ended.
const LAYOUT_KEY: &str = "main_dock_layout";
/// Layout schema version. Bump when a panel is renamed or the arrangement changes shape — a saved
/// layout from an older version is discarded rather than restored into a shape it no longer fits.
const LAYOUT_VERSION: usize = 1;

pub struct MainDock {
    dock_area: Entity<DockArea>,
    /// Persists the layout whenever the dock emits `LayoutChanged`.
    _layout_sub: Subscription,
}

impl MainDock {
    pub fn new(
        workspace: Entity<Workspace>,
        log: Entity<LogViewer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // Restoring a saved layout rebuilds panels *by name*, so both need a constructor
        // registered under theirs. Both hand back the entities built above rather than making new
        // ones: the workspace holds every input the user has typed into, and the log holds the
        // run's output — rebuilding either would silently throw that away.
        register_panel(cx, "Workspace", {
            let workspace = workspace.clone();
            move |_, _, _| panel_handle(workspace.clone())
        });
        register_panel(cx, "LogPanel", {
            let log = log.clone();
            move |_, _, _| panel_handle(log.clone())
        });

        let (dock_area, dock_skin) = DockSkin::dock_area("main", Some(LAYOUT_VERSION), window, cx);
        dock_skin.set_toggle_button_visible(false, cx);

        dock_area.update(cx, |area, cx| {
            area.set_center(
                DockLayout::tabs().panel_view(panel_handle(workspace), cx),
                window,
                cx,
            );
            area.set_dock(
                DockPlacement::Bottom,
                DockLayout::tabs().panel_view(panel_handle(log), cx),
                window,
                cx,
            );
            area.set_dock_size(DockPlacement::Bottom, px(220.), window, cx);
            // Start closed unless a persisted layout below says otherwise.
            if area.is_dock_open(DockPlacement::Bottom) {
                area.toggle_dock(DockPlacement::Bottom, window, cx);
            }
        });

        Self::restore_layout(&dock_area, window, cx);

        let _layout_sub = cx.subscribe(&dock_area, |_this, area, event: &DockEvent, cx| {
            if matches!(event, DockEvent::LayoutChanged) {
                Self::persist_layout(&area, cx);
            }
        });

        Self {
            dock_area,
            _layout_sub,
        }
    }

    /// Weak handle to the dock area, so the status-bar button can toggle the log open and closed.
    pub fn dock_area(&self) -> WeakEntity<DockArea> {
        self.dock_area.downgrade()
    }

    /// Toggle the log from the View menu or its window-wide shortcut.
    pub fn toggle_log(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.dock_area.update(cx, |area, cx| {
            toggle_bottom_dock(area, window, cx);
        });
    }

    fn persist_layout(dock_area: &Entity<DockArea>, cx: &mut App) {
        let state = dock_area.read(cx).dump(cx);
        if let Ok(json) = serde_json::to_string(&state) {
            AppSettings::set_text(LAYOUT_KEY, json.into(), cx);
        }
    }

    /// Applies a previously saved arrangement over the default one built above. Any failure —
    /// nothing saved, corrupt JSON, a stale schema version — leaves that default in place, which
    /// is why this returns nothing for a caller to handle.
    fn restore_layout(dock_area: &Entity<DockArea>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(raw) = AppSettings::get(cx)
            .values
            .get(LAYOUT_KEY)
            .map(|v| v.text().to_string())
            .filter(|raw| !raw.is_empty())
        else {
            return;
        };
        let state: DockAreaState = match serde_json::from_str(&raw) {
            Ok(state) => state,
            Err(err) => {
                log::warn!("ignoring corrupt dock layout: {err}");
                return;
            }
        };
        if state.version != Some(LAYOUT_VERSION) {
            return;
        }
        dock_area.update(cx, |area, cx| {
            if let Err(err) = area.load(state, window, cx) {
                log::error!("failed to restore dock layout: {err}");
            }
        });
    }
}

impl Render for MainDock {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.dock_area.clone())
    }
}

/// The status-bar button that opens and closes the log dock.
///
/// A plain `div` rather than `gpui_component::Button` on purpose: the library Button hardcodes
/// `cursor_default` for every non-link variant with no override, so a div is the only way to get
/// a pointer cursor on a toggle. It also lets the hover highlight be set explicitly instead of
/// relying on the ghost variant's very subtle default.
pub struct LogDockButton {
    dock: WeakEntity<DockArea>,
    /// Redraws the button when the dock opens or closes, including from the library's own
    /// drag-resize and close affordances rather than only from a click on this button.
    _sub: Option<Subscription>,
}

impl LogDockButton {
    pub fn new(dock: WeakEntity<DockArea>, cx: &mut Context<Self>) -> Self {
        let _sub = dock
            .upgrade()
            .map(|area| cx.subscribe(&area, |_this, _, _: &DockEvent, cx| cx.notify()));
        Self { dock, _sub }
    }
}

impl Render for LogDockButton {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let open = self
            .dock
            .upgrade()
            .is_some_and(|area| area.read(cx).is_dock_open(DockPlacement::Bottom));
        let hover_bg = cx.theme().secondary_hover;
        let dock = self.dock.clone();

        h_flex()
            .id("log-dock-toggle")
            .gap_1()
            .px(px(4.))
            .py(px(2.))
            .rounded_md()
            .cursor_pointer()
            .hover(move |this| this.bg(hover_bg))
            .when(open, |this| this.text_color(cx.theme().primary))
            // The panel-bottom glyph fills in when the dock is open — the button says its state
            // twice, in colour and in shape, which is what makes it readable at a glance.
            .child(
                Icon::new(if open {
                    IconName::PanelBottomOpen
                } else {
                    IconName::PanelBottom
                })
                .small(),
            )
            .child("Log")
            .tooltip(|window, cx| {
                gpui_component::tooltip::Tooltip::new("Toggle Log")
                    .action(&ToggleLog, None)
                    .build(window, cx)
            })
            .on_click(move |_, window, cx| {
                let Some(area) = dock.upgrade() else { return };
                area.update(cx, |area, cx| {
                    toggle_bottom_dock(area, window, cx);
                });
            })
    }
}

/// Toggle the bottom dock and publish the resulting layout in the same UI turn, so the open state
/// reaches the settings file rather than only the screen.
///
/// The event is ours to emit: the library's `toggle_dock` notifies its own view and stops there,
/// and `LayoutChanged` is what `MainDock` persists on.
fn toggle_bottom_dock(area: &mut DockArea, window: &mut Window, cx: &mut Context<DockArea>) {
    area.toggle_dock(DockPlacement::Bottom, window, cx);
}
