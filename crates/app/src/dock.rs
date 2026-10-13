//! The window body as a `gpui_component::dock::DockArea`: the workbench steps in the centre, the
//! run log in a bottom dock, and the status-bar button that opens and closes it.
//!
//! One dock, deliberately. qrate's version carries a panel registry because its panels *move*
//! between three docks and its bar buttons have to follow them; with a single panel in a single
//! dock there is nothing to track — the button toggles the bottom dock and that is the whole model.

use config_builder::{BuilderPane, DOCK_DEFAULT_W, clamp_dock_width};
use gpui::*;
use gpui_component::{
    IconName, Selectable as _,
    dock::{
        DockArea, DockAreaState, DockEvent, DockLayout, DockPlacement, DockSkin, panel_handle,
        register_panel,
    },
};
use settings::AppSettings;
use ui::tokens::DOCK_TITLE_H;

use crate::{
    app_menus::ToggleLog,
    workspace::{LogViewer, Workspace},
};

/// Settings key under which the serialized [`DockAreaState`] is persisted, so the log pane reopens
/// at the height it was left and stays shut if that is how the last session ended.
const LAYOUT_KEY: &str = "main_dock_layout";
/// Layout schema version. Bump when a panel is renamed or the arrangement changes shape — a saved
/// layout from an older version is discarded rather than restored into a shape it no longer fits.
const LAYOUT_VERSION: usize = 2;

pub struct MainDock {
    dock_area: Entity<DockArea>,
    /// The right dock's one panel: the docked Config Builder, or its placeholder.
    builder_pane: Entity<BuilderPane>,
    /// Persists the layout whenever the dock emits `LayoutChanged`.
    _layout_sub: Subscription,
    /// Re-clamps the builder's dock when the window shrinks under it.
    _bounds_sub: Subscription,
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
        let builder_pane = cx.new(BuilderPane::new);
        register_panel(cx, "BuilderPane", {
            let pane = builder_pane.clone();
            move |_, _, _| panel_handle(pane.clone())
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
            area.set_dock(
                DockPlacement::Right,
                DockLayout::tabs().panel_view(panel_handle(builder_pane.clone()), cx),
                window,
                cx,
            );
            area.set_dock_size(DockPlacement::Right, DOCK_DEFAULT_W, window, cx);
        });
        builder_pane.update(cx, |pane, _| pane.set_area(dock_area.downgrade()));

        Self::restore_layout(&dock_area, window, cx);
        // Nothing is docked at startup: a restored layout keeps the width, not an empty pane.
        dock_area.update(cx, |area, cx| {
            if area.is_dock_open(DockPlacement::Right) {
                area.toggle_dock(DockPlacement::Right, window, cx);
            }
        });

        let _layout_sub = cx.subscribe_in(
            &dock_area,
            window,
            |this, area, event: &DockEvent, window, cx| {
                if matches!(event, DockEvent::LayoutChanged) {
                    Self::clamp_builder_width(area, window, cx);
                    // The snap preview opens and closes the dock while a builder is dragged;
                    // that is not an arrangement the user chose.
                    if !this.builder_pane.read(cx).is_previewing() {
                        Self::persist_layout(area, cx);
                    }
                }
            },
        );

        // The 60% ceiling is a share of the window, so a smaller window lowers it too (`2b`).
        let _bounds_sub = cx.observe_window_bounds(window, |this, window, cx| {
            Self::clamp_builder_width(&this.dock_area, window, cx);
        });
        // A layout restored from a wider window may already be over it.
        Self::clamp_builder_width(&dock_area, window, cx);

        Self {
            _bounds_sub,
            dock_area,
            builder_pane,
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

    pub fn builder_pane(&self) -> &Entity<BuilderPane> {
        &self.builder_pane
    }

    /// Component Spec `2b`: the builder's dock stays between 420 px and 60% of the window.
    fn clamp_builder_width(area: &Entity<DockArea>, window: &mut Window, cx: &mut App) {
        let window_width = window.bounds().size.width;
        area.update(cx, |area, cx| {
            if let Some(width) = area.dock_size(DockPlacement::Right) {
                let clamped = clamp_dock_width(width, window_width);
                if clamped != width {
                    area.set_dock_size(DockPlacement::Right, clamped, window, cx);
                }
            }
        });
    }

    fn persist_layout(dock_area: &Entity<DockArea>, cx: &mut App) {
        let state = dock_area.read(cx).dump(cx);
        let Ok(json) = serde_json::to_string(&state) else {
            return;
        };
        // The event fires on every edit, most of which leave the saved layout as it was.
        let saved = AppSettings::get(cx)
            .values
            .get(LAYOUT_KEY)
            .map(|v| v.text());
        if saved.as_deref() != Some(json.as_str()) {
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
        // The centre and right docks each hold one panel, which the library titles with a
        // plain row; the workspace needs no title and the docked builder draws its own bar. The
        // area is lifted by exactly that row and clipped, so neither shows (the bottom log's
        // title sits at its own top and stays).
        div().size_full().relative().overflow_hidden().child(
            div()
                .absolute()
                .top(-DOCK_TITLE_H)
                .bottom_0()
                .left_0()
                .right_0()
                .child(self.dock_area.clone()),
        )
    }
}

/// The status-bar button that opens and closes the log dock.
///
/// Uses the same library-backed status button as every other action in the bottom strip.
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
        let dock = self.dock.clone();

        // The panel-bottom glyph fills in when the dock is open — the button says its state in
        // shape without forcing an accent colour while the control is idle.
        ui::status_bar_button("log-dock-toggle")
            .icon(if open {
                IconName::PanelBottomOpen
            } else {
                IconName::PanelBottom
            })
            .label("Log")
            .selected(open)
            .toggled(open)
            .tooltip_with_action("Toggle Log", &ToggleLog, None)
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
