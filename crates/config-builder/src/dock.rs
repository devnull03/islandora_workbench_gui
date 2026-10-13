//! Where a builder lives (Stage 4, mockup `2b`): its own window, or the main window's right dock.
//!
//! A gpui view belongs to the window that made it — its inputs hold that window's focus handles
//! — so a builder is never moved between windows. Docking and floating *rebuild* it in the other
//! host from a [`BuilderSnapshot`]: the draft is the source of truth, and every widget is made
//! from it anyway. The dock itself is gpui-component's `DockArea`; [`BuilderPane`] is the one
//! panel in its right dock, holding the docked builder, or the dashed placeholder while that
//! builder floats, or the snap target while a floating one is dragged over the window's edge.
//!
//! The library has no drag of a panel *out* to an OS window or back in, so the snap is driven
//! by the floating window's own moves: a window held over the right edge of the main window for
//! [`SNAP_SETTLE`] docks (Stage 4's "drop-driven" fallback; window managers do not report the
//! end of a move drag).

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, Sizable as _,
    dock::{BasePanel, DockArea, DockPlacement, Panel, PanelControl, PanelEvent},
    h_flex,
    label::Label,
    v_flex,
};
use settings::AppSettings;
use ui::tokens::{GAP_MD, GAP_SM, PAD_PAGE};
use workbench_integration::config::ConfigDraft;

use crate::{
    BuilderChromeState, ConfigBuilder, ConfigBuilderWindows, open_builder_window, ui_title,
};

actions!(config_builder, [ToggleBuilderDock]);

/// Settings → Config builder → "Open the builder docked". Off by default (plan Q4) until
/// docking has been lived with.
pub const BUILDER_OPEN_DOCKED_KEY: &str = "builder_open_docked";

/// How long a floating builder must stay over the main window's right edge to dock.
const SNAP_SETTLE: Duration = Duration::from_millis(600);
/// How close the floating window's left edge must come to the main window's right edge.
const SNAP_DISTANCE: Pixels = px(24.);
/// Component Spec `2b`: the docked builder opens 480 px wide, no narrower than 420 px and no
/// wider than 60% of the window.
pub const DOCK_DEFAULT_W: Pixels = px(480.);
pub const DOCK_MIN_W: Pixels = px(420.);
pub const DOCK_MAX_FRACTION: f32 = 0.6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BuilderHost {
    Window,
    Docked,
}

/// Everything a builder holds that is not a widget.
pub(crate) struct BuilderSnapshot {
    /// What widgets refused, and their text, so a move does not lose a half-typed value.
    pub(crate) widget_errors: std::collections::HashMap<String, String>,
    pub(crate) carried_text: std::collections::HashMap<SharedString, String>,
    pub(crate) profile: String,
    pub(crate) draft: ConfigDraft,
    pub(crate) description: String,
    pub(crate) ancestors: Vec<PathBuf>,
    pub(crate) loaded_numbers: HashSet<String>,
    pub(crate) collapsed_chain: HashSet<PathBuf>,
    pub(crate) chrome: BuilderChromeState,
}

/// The main window's side of docking, installed by the app once its dock exists.
#[derive(Default)]
pub struct BuilderDock {
    window: Option<AnyWindowHandle>,
    pane: Option<WeakEntity<BuilderPane>>,
}

impl Global for BuilderDock {}

impl BuilderDock {
    /// The main window and its builder pane, while the main window is open.
    pub fn main(cx: &App) -> Option<(AnyWindowHandle, Entity<BuilderPane>)> {
        Self::get(cx)
    }

    /// The main window closed: forget it, and drop the docked builder with it, so nothing
    /// tries to dock into a window that is gone or edits a builder nobody can see.
    pub fn uninstall(cx: &mut App) {
        if let Some(pane) = cx
            .try_global::<BuilderDock>()
            .and_then(|dock| dock.pane.as_ref()?.upgrade())
        {
            pane.update(cx, |pane, cx| {
                pane.docked = None;
                pane.floated = None;
                pane._floated_release = None;
                pane.snap_preview = None;
                pane._preview_release = None;
                cx.notify();
            });
        }
        cx.set_global(BuilderDock::default());
    }

    fn get(cx: &App) -> Option<(AnyWindowHandle, Entity<BuilderPane>)> {
        let dock = cx.try_global::<BuilderDock>()?;
        Some((dock.window?, dock.pane.as_ref()?.upgrade()?))
    }
}

/// Tell the builder where the main window's dock is.
pub fn install_builder_dock(window: AnyWindowHandle, pane: &Entity<BuilderPane>, cx: &mut App) {
    cx.set_global(BuilderDock {
        window: Some(window),
        pane: Some(pane.downgrade()),
    });
}

/// The right dock's one panel.
pub struct BuilderPane {
    area: WeakEntity<DockArea>,
    docked: Option<Entity<ConfigBuilder>>,
    /// The builder last floated out of the dock, while its window is open: the placeholder
    /// names it and its Dock button brings it back.
    floated: Option<WeakEntity<ConfigBuilder>>,
    /// The dock's width as a share of the window, while a floating builder is over the edge.
    snap_preview: Option<f32>,
    /// Clears the preview if the builder that set it goes away before it docks or moves on.
    _preview_release: Option<Subscription>,
    focus_handle: FocusHandle,
    _floated_release: Option<Subscription>,
}

impl BuilderPane {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            area: WeakEntity::new_invalid(),
            docked: None,
            floated: None,
            snap_preview: None,
            _preview_release: None,
            focus_handle: cx.focus_handle(),
            _floated_release: None,
        }
    }

    /// The dock this pane sits in, so it can open and close it.
    pub fn set_area(&mut self, area: WeakEntity<DockArea>) {
        self.area = area;
    }

    /// Whether the pane has anything to show; the app closes an empty right dock on startup.
    pub fn is_empty(&self) -> bool {
        self.docked.is_none() && self.floated_view().is_none() && self.snap_preview.is_none()
    }

    /// Whether the dock is open only to show where a dragged builder would land.
    pub fn is_previewing(&self) -> bool {
        self.snap_preview.is_some()
    }

    /// The dock's width while a builder is docked in it — the main window lays its steps out
    /// in what is left.
    pub fn docked_width(&self, cx: &App) -> Option<Pixels> {
        self.docked.as_ref()?;
        self.area
            .upgrade()?
            .read(cx)
            .dock_size(DockPlacement::Right)
    }

    /// The docked builder holds edits that are not saved.
    pub fn has_unsaved_changes(&self, cx: &App) -> bool {
        self.docked
            .as_ref()
            .is_some_and(|view| view.read(cx).chrome.is_dirty())
    }

    /// Drop the docked builder without saving — the main window is closing.
    pub fn discard_docked(&mut self, cx: &mut Context<Self>) {
        self.docked = None;
        cx.notify();
    }

    /// `Ctrl+\` from anywhere in the main window: float the docked builder, or bring the
    /// floated one home.
    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        if let Some(view) = self.docked.clone() {
            cx.defer(move |cx| float(view, cx));
        } else if let Some(view) = self.floated_view() {
            cx.defer(move |cx| dock(view, cx));
        }
    }

    fn floated_view(&self) -> Option<Entity<ConfigBuilder>> {
        self.floated.as_ref().and_then(WeakEntity::upgrade)
    }

    /// Open or close the right dock to match what the pane has to show.
    fn sync_dock(&self, window: &mut Window, cx: &mut App) {
        let show = !self.is_empty();
        if let Some(area) = self.area.upgrade() {
            area.update(cx, |area, cx| {
                if area.is_dock_open(DockPlacement::Right) != show {
                    area.toggle_dock(DockPlacement::Right, window, cx);
                }
            });
        }
    }

    fn set_docked(&mut self, view: Entity<ConfigBuilder>, cx: &mut Context<Self>) {
        self.docked = Some(view);
        self.floated = None;
        self._floated_release = None;
        self.snap_preview = None;
        self._preview_release = None;
        cx.notify();
    }

    fn set_floated(&mut self, view: &Entity<ConfigBuilder>, cx: &mut Context<Self>) {
        self.docked = None;
        self.floated = Some(view.downgrade());
        // Closing the floated window takes the placeholder with it.
        self._floated_release = Some(cx.observe_release(view, |pane, _, cx| {
            pane.floated = None;
            pane._floated_release = None;
            sync_dock_later(cx);
            cx.notify();
        }));
        cx.notify();
    }

    fn set_snap_preview(
        &mut self,
        preview: Option<f32>,
        source: Option<&Entity<ConfigBuilder>>,
        cx: &mut Context<Self>,
    ) {
        self._preview_release = match (preview, source) {
            (Some(_), Some(view)) => Some(cx.observe_release(view, |pane, _, cx| {
                pane.set_snap_preview(None, None, cx);
            })),
            _ => None,
        };
        if self.snap_preview != preview {
            self.snap_preview = preview;
            sync_dock_later(cx);
            cx.notify();
        }
    }
}

/// Open or close the dock once the current update is over (the dock needs the main window).
fn sync_dock_later(cx: &mut Context<BuilderPane>) {
    let pane = cx.entity().downgrade();
    cx.defer(move |cx| {
        let (Some((window, _)), Some(pane)) = (BuilderDock::get(cx), pane.upgrade()) else {
            return;
        };
        let _ = window.update(cx, |_, window, cx| {
            let pane = pane.read(cx);
            let (area, show) = (pane.area.clone(), !pane.is_empty());
            if let Some(area) = area.upgrade() {
                area.update(cx, |area, cx| {
                    if area.is_dock_open(DockPlacement::Right) != show {
                        area.toggle_dock(DockPlacement::Right, window, cx);
                    }
                });
            }
        });
    });
}

impl Render for BuilderPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors;
        let content: AnyElement = if let Some(preview) = self.snap_preview {
            // `2b`: the dashed target the builder will fill, and what letting go does.
            drop_target(
                format!("right half · {}%", (preview * 100.).round()),
                "Release to dock",
                None,
                cx,
            )
        } else if let Some(view) = self.docked.clone() {
            view.into_any_element()
        } else if let Some(view) = self.floated_view() {
            let title = ui_title(&view.read(cx).draft);
            drop_target(
                format!("{title} is in its own window"),
                "Drag it back over this edge, or",
                Some(view),
                cx,
            )
        } else {
            div().into_any_element()
        };
        div()
            .size_full()
            .track_focus(&self.focus_handle)
            .bg(colors.background)
            .child(content)
    }
}

/// The dashed outline the dock shows where a builder will go or has gone.
fn drop_target(
    title: String,
    hint: &'static str,
    floated: Option<Entity<ConfigBuilder>>,
    cx: &App,
) -> AnyElement {
    let colors = cx.theme().colors;
    div()
        .size_full()
        .p(PAD_PAGE)
        .child(
            v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap(GAP_MD)
                .border_2()
                .border_dashed()
                .border_color(colors.drag_border)
                .rounded(cx.theme().radius_lg)
                .bg(colors.drop_target)
                .child(Label::new(title).text_sm())
                .child(
                    h_flex()
                        .gap(GAP_SM)
                        .items_center()
                        .child(
                            Label::new(hint)
                                .text_xs()
                                .text_color(colors.muted_foreground),
                        )
                        .when_some(floated, |row, view| {
                            row.child(
                                ui::ghost_button("builder-dock-back")
                                    .label("Dock")
                                    .with_size(ui::APP_CONTROL_SIZE)
                                    .on_click(move |_, _, cx| {
                                        let view = view.clone();
                                        cx.defer(move |cx| dock(view, cx));
                                    }),
                            )
                        }),
                ),
        )
        .into_any_element()
}

impl Focusable for BuilderPane {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for BuilderPane {}

impl BasePanel for BuilderPane {
    fn panel_name(&self) -> &'static str {
        "BuilderPane"
    }

    /// Closed with the builder's own ✕, which asks about unsaved edits; the tab's close would
    /// not.
    fn closable(&self, _cx: &App) -> bool {
        false
    }

    fn zoomable(&self, _cx: &App) -> bool {
        false
    }
}

impl Panel for BuilderPane {
    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        SharedString::from("Config Builder")
    }

    fn zoom_control(&self, _cx: &App) -> Option<PanelControl> {
        None
    }
}

/// Open `path` in the dock if Settings asks for that and the dock is free. `false` when the
/// caller should open a window instead.
pub(crate) fn open_docked(path: Option<PathBuf>, cx: &mut App) -> bool {
    if !AppSettings::bool_or(BUILDER_OPEN_DOCKED_KEY, false, cx) {
        return false;
    }
    let Some((window, pane)) = BuilderDock::get(cx) else {
        return false;
    };
    if !pane.read(cx).is_empty() {
        return false;
    }
    let key = path.clone();
    window
        .update(cx, |_, window, cx| {
            let view = cx.new(|cx| {
                let mut builder = ConfigBuilder::new(path, Vec::new(), window, cx);
                builder.host = BuilderHost::Docked;
                builder
            });
            cx.global_mut::<ConfigBuilderWindows>().register(
                key,
                window.window_handle(),
                view.downgrade(),
            );
            pane.update(cx, |pane, cx| {
                pane.set_docked(view, cx);
                pane.sync_dock(window, cx);
            });
        })
        .is_ok()
}

/// Move a floating builder into the dock. A builder already docked floats out to make room.
pub(crate) fn dock(view: Entity<ConfigBuilder>, cx: &mut App) {
    let Some((main, pane)) = BuilderDock::get(cx) else {
        return;
    };
    // Chain children always keep their own window (plan 5.7).
    if view.read(cx).host == BuilderHost::Docked || !view.read(cx).ancestors.is_empty() {
        return;
    }
    if let Some(current) = pane.read(cx).docked.clone() {
        float(current, cx);
    }
    let floating = cx
        .global::<ConfigBuilderWindows>()
        .window_of(&view.downgrade());
    let snapshot = view.read(cx).snapshot(cx);
    let key = snapshot.draft.path.clone();
    let docked_ok = main.update(cx, |_, window, cx| {
        let docked = cx.new(|cx| ConfigBuilder::restore(snapshot, BuilderHost::Docked, window, cx));
        cx.global_mut::<ConfigBuilderWindows>().register(
            key,
            window.window_handle(),
            docked.downgrade(),
        );
        pane.update(cx, |pane, cx| {
            pane.set_docked(docked, cx);
            pane.sync_dock(window, cx);
        });
        window.activate_window();
    });
    // The floating window goes only once its draft is safely in the dock.
    if docked_ok.is_err() {
        return;
    }
    if let Some(floating) = floating {
        let _ = floating.update(cx, |_, window, _| window.remove_window());
    }
}

/// Move the docked builder out to its own window, leaving the placeholder behind.
pub(crate) fn float(view: Entity<ConfigBuilder>, cx: &mut App) {
    let Some((_, pane)) = BuilderDock::get(cx) else {
        return;
    };
    let snapshot = view.read(cx).snapshot(cx);
    let key = snapshot.draft.path.clone();
    let depth = snapshot.ancestors.len();
    let with_yaml = snapshot.chrome.yaml_visible();
    let opening = open_builder_window(
        key,
        depth,
        with_yaml,
        move |window, cx| ConfigBuilder::restore(snapshot, BuilderHost::Window, window, cx),
        cx,
    );
    cx.spawn(async move |cx| {
        let Some(floated) = opening.await else {
            return;
        };
        cx.update(|cx| {
            pane.update(cx, |pane, cx| {
                // Only if the docked builder is still the one being floated.
                if pane.docked.as_ref() == Some(&view) {
                    pane.set_floated(&floated, cx);
                }
            });
        });
    })
    .detach();
}

impl ConfigBuilder {
    /// `Ctrl+\` and the Pop out button: float when docked, dock when floating.
    pub(crate) fn toggle_dock(&mut self, cx: &mut Context<Self>) {
        let view = cx.entity();
        match self.host {
            BuilderHost::Docked => cx.defer(move |cx| float(view, cx)),
            BuilderHost::Window if self.ancestors.is_empty() => cx.defer(move |cx| dock(view, cx)),
            BuilderHost::Window => {}
        }
    }

    /// The docked builder's ✕: close it, asking first when there are unsaved edits.
    pub(crate) fn close_docked(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let close = |cx: &mut App| {
            if let Some((main, pane)) = BuilderDock::get(cx) {
                let _ = main.update(cx, |_, window, cx| {
                    pane.update(cx, |pane, cx| {
                        pane.discard_docked(cx);
                        pane.sync_dock(window, cx);
                    });
                });
            }
        };
        if !self.chrome.is_dirty() {
            cx.defer(close);
            return;
        }
        let title = ui_title(&self.draft);
        ui::confirm_dialog(
            window,
            cx,
            format!("Close {title} without saving?"),
            "Your edits since the last save are lost. The file on disk is not changed.",
            "Close without saving",
            move |_, cx| cx.defer(close),
        );
    }
}

/// The floating builder's window moved or resized. Over the main window's right edge, on the
/// same display, it shows the dock's target and docks once it has been still for
/// [`SNAP_SETTLE`]; anywhere else it clears the target.
pub(crate) fn window_moved(
    this: &mut ConfigBuilder,
    window: &mut Window,
    cx: &mut Context<ConfigBuilder>,
) {
    let Some((main, pane)) = BuilderDock::get(cx) else {
        return;
    };
    let bounds = window.bounds();
    // Only a move arms the snap: a resize (or tiling a window beside the main one) changes
    // the size too, and a maximized or chained window never docks.
    let moved_only = this
        .last_bounds
        .replace(bounds)
        .is_some_and(|last| last.size == bounds.size && last.origin != bounds.origin);
    if !moved_only || window.is_maximized() || window.is_fullscreen() || !this.ancestors.is_empty()
    {
        if this.snap.take().is_some() {
            pane.update(cx, |pane, cx| pane.set_snap_preview(None, None, cx));
        }
        return;
    }
    let display = window.display(cx).map(|d| d.id());
    let main_state = main
        .update(cx, |_, main_window, cx| {
            (
                main_window.bounds(),
                main_window.display(cx).map(|d| d.id()),
            )
        })
        .ok();
    let over = main_state.is_some_and(|(main_bounds, main_display)| {
        main_display == display && over_right_edge(bounds, main_bounds)
    });
    let preview = main_state
        .filter(|_| over)
        .map(|(main_bounds, _)| dock_fraction(main_bounds.size.width, cx));
    let source = cx.entity();
    pane.update(cx, |pane, cx| {
        pane.set_snap_preview(preview, Some(&source), cx)
    });
    if !over {
        this.snap = None;
        return;
    }
    let moved_at = Instant::now();
    this.snap = Some(moved_at);
    let view = cx.entity().downgrade();
    this.snap_task = Some(cx.spawn(async move |_, cx| {
        cx.background_executor().timer(SNAP_SETTLE).await;
        cx.update(|cx| {
            let Some(view) = view.upgrade() else {
                return;
            };
            // Moved again since: that move scheduled its own check.
            if view.read(cx).snap != Some(moved_at) {
                return;
            }
            dock(view, cx);
        });
    }));
}

/// The floating window's left edge has been dragged over the main window's right edge — inside
/// it, by up to [`SNAP_DISTANCE`] — and the two overlap vertically. Merely touching the edge
/// (two windows tiled side by side) is not enough.
fn over_right_edge(floating: Bounds<Pixels>, main: Bounds<Pixels>) -> bool {
    let edge = main.origin.x + main.size.width;
    let left = floating.origin.x;
    let vertical = floating.origin.y < main.origin.y + main.size.height
        && main.origin.y < floating.origin.y + floating.size.height;
    vertical && left < edge && left >= edge - SNAP_DISTANCE
}

/// The share of the main window the dock will take, from its saved width, clamped as the
/// spec clamps it.
fn dock_fraction(window_width: Pixels, cx: &App) -> f32 {
    let width = clamp_dock_width(saved_dock_width(cx), window_width);
    f32::from(width) / f32::from(window_width).max(1.)
}

/// The right dock's width from the persisted layout, else the spec default. Read only while a
/// floating builder is over the edge, never per frame.
fn saved_dock_width(cx: &App) -> Pixels {
    cx.try_global::<BuilderDock>()
        .and_then(|dock| dock.pane.as_ref()?.upgrade())
        .and_then(|pane| pane.read(cx).area.upgrade())
        .and_then(|area| area.read(cx).dock_size(DockPlacement::Right))
        .unwrap_or(DOCK_DEFAULT_W)
}

/// `width` held between the spec's minimum and 60% of the window.
pub fn clamp_dock_width(width: Pixels, window_width: Pixels) -> Pixels {
    let max = (window_width * DOCK_MAX_FRACTION).max(DOCK_MIN_W);
    width.clamp(DOCK_MIN_W, max)
}

#[cfg(test)]
mod tests {
    use super::{DOCK_MIN_W, clamp_dock_width, over_right_edge};
    use gpui::{Bounds, point, px, size};

    #[test]
    fn snapping_needs_the_left_edge_at_the_right_edge() {
        let main = Bounds::new(point(px(0.), px(0.)), size(px(1000.), px(700.)));
        let at = |x: f32, y: f32| Bounds::new(point(px(x), px(y)), size(px(800.), px(600.)));
        assert!(over_right_edge(at(990., 50.), main));
        assert!(over_right_edge(at(980., 50.), main));
        // Tiled exactly beside it, or just outside: not a drop.
        assert!(!over_right_edge(at(1000., 50.), main));
        assert!(!over_right_edge(at(1010., 50.), main));
        assert!(!over_right_edge(at(1100., 50.), main));
        assert!(!over_right_edge(at(500., 50.), main));
        // Over the edge's line but entirely below the window.
        assert!(!over_right_edge(at(990., 800.), main));
    }

    #[test]
    fn dock_width_is_clamped_to_the_spec() {
        assert_eq!(clamp_dock_width(px(100.), px(1200.)), DOCK_MIN_W);
        assert_eq!(clamp_dock_width(px(900.), px(1200.)), px(720.));
        assert_eq!(clamp_dock_width(px(500.), px(1200.)), px(500.));
        // A window too narrow for 60% to reach the minimum still gets the minimum.
        assert_eq!(clamp_dock_width(px(500.), px(600.)), DOCK_MIN_W);
    }
}
