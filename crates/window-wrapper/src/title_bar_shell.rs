//! The title bar chrome — drag region and window controls.
//!
//! A fork of `gpui_component::TitleBar`, kept for one reason: **this app does not maximize.**
//! Upstream always renders a Maximize/Restore control and offers no way to turn it off, so the
//! only way to drop it is to own the control row.
//!
//! What that buys, and what it does not: the button is gone, and with it the Windows snap-layouts
//! flyout, because both come from the `WindowControlArea::Max` region. Double-clicking the drag
//! region and Win+Up still zoom — those belong to the OS and follow from the window being
//! resizable at all, which it must be. Making them stop needs `is_resizable: false`, which would
//! also stop the user dragging the window edges.
//!
//! Everything else is upstream's, so keep it recognisably close to the original when updating.

use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, ClickEvent, Context, Decorations, Hsla, InteractiveElement, IntoElement,
    MouseButton, ParentElement, Pixels, Render, RenderOnce, StatefulInteractiveElement as _,
    StyleRefinement, Styled, TitlebarOptions, Window, WindowControlArea, div, px,
};
use gpui_component::{ActiveTheme, Icon, IconName, Sizable as _, StyledExt, h_flex};
use smallvec::SmallVec;

pub const TITLE_BAR_HEIGHT: Pixels = px(34.);
#[cfg(target_os = "macos")]
const TITLE_BAR_LEFT_PADDING: Pixels = px(80.);
#[cfg(not(target_os = "macos"))]
const TITLE_BAR_LEFT_PADDING: Pixels = px(12.);

type CloseHandler = Rc<Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>>;

#[derive(IntoElement)]
pub struct TitleBarShell {
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 1]>,
    on_close_window: Option<CloseHandler>,
}

impl Default for TitleBarShell {
    fn default() -> Self {
        Self::new()
    }
}

impl TitleBarShell {
    pub fn new() -> Self {
        Self {
            style: StyleRefinement::default(),
            children: SmallVec::new(),
            on_close_window: None,
        }
    }

    /// Window options matching this chrome: the OS bar is transparent because we draw it.
    pub fn title_bar_options() -> TitlebarOptions {
        TitlebarOptions {
            title: None,
            appears_transparent: true,
            traffic_light_position: Some(gpui::point(px(9.0), px(9.0))),
        }
    }

    /// Linux only; elsewhere the close button is handled by the OS.
    pub fn on_close_window(
        mut self,
        f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        if cfg!(target_os = "linux") {
            self.on_close_window = Some(Rc::new(Box::new(f)));
        }
        self
    }
}

/// Minimize and Close. There is deliberately no Maximize — see the module docs.
#[derive(IntoElement, Clone)]
enum ControlIcon {
    Minimize,
    Close {
        on_close_window: Option<CloseHandler>,
    },
}

impl ControlIcon {
    fn id(&self) -> &'static str {
        match self {
            Self::Minimize => "minimize",
            Self::Close { .. } => "close",
        }
    }

    fn icon(&self) -> IconName {
        match self {
            Self::Minimize => IconName::WindowMinimize,
            Self::Close { .. } => IconName::WindowClose,
        }
    }

    fn window_control_area(&self) -> WindowControlArea {
        match self {
            Self::Minimize => WindowControlArea::Min,
            Self::Close { .. } => WindowControlArea::Close,
        }
    }

    fn is_close(&self) -> bool {
        matches!(self, Self::Close { .. })
    }

    fn hover_fg(&self, cx: &App) -> Hsla {
        if self.is_close() {
            cx.theme().danger_foreground
        } else {
            cx.theme().secondary_foreground
        }
    }

    fn hover_bg(&self, cx: &App) -> Hsla {
        if self.is_close() {
            cx.theme().danger
        } else {
            cx.theme().secondary_hover
        }
    }

    fn active_bg(&self, cx: &App) -> Hsla {
        if self.is_close() {
            cx.theme().danger_active
        } else {
            cx.theme().secondary_active
        }
    }
}

impl RenderOnce for ControlIcon {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let is_linux = cfg!(target_os = "linux");
        let is_windows = cfg!(target_os = "windows");
        let hover_fg = self.hover_fg(cx);
        let hover_bg = self.hover_bg(cx);
        let active_bg = self.active_bg(cx);
        let icon = self.clone();
        let on_close_window = match &self {
            ControlIcon::Close { on_close_window } => on_close_window.clone(),
            _ => None,
        };

        div()
            .id(self.id())
            .flex()
            .w(TITLE_BAR_HEIGHT)
            .h_full()
            .flex_shrink_0()
            .justify_center()
            .content_center()
            .items_center()
            .text_color(cx.theme().foreground)
            .hover(|style| style.bg(hover_bg).text_color(hover_fg))
            .active(|style| style.bg(active_bg).text_color(hover_fg))
            .when(is_windows, |this| {
                this.window_control_area(self.window_control_area())
            })
            .when(is_linux, |this| {
                this.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    match icon {
                        Self::Minimize => window.minimize_window(),
                        Self::Close { .. } => {
                            if let Some(f) = on_close_window.clone() {
                                f(&ClickEvent::default(), window, cx);
                            } else {
                                window.remove_window();
                            }
                        }
                    }
                })
            })
            .child(Icon::new(self.icon()).small())
    }
}

#[derive(IntoElement)]
struct WindowControls {
    on_close_window: Option<CloseHandler>,
}

impl RenderOnce for WindowControls {
    fn render(self, _window: &mut Window, _: &mut App) -> impl IntoElement {
        if cfg!(target_os = "macos") {
            return div().id("window-controls");
        }

        h_flex()
            .id("window-controls")
            .items_center()
            .flex_shrink_0()
            .h_full()
            .child(ControlIcon::Minimize)
            .child(ControlIcon::Close {
                on_close_window: self.on_close_window,
            })
    }
}

impl Styled for TitleBarShell {
    fn style(&mut self) -> &mut gpui::StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for TitleBarShell {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

/// Upstream tracks the press so a drag starts a window move rather than a text selection.
struct TitleBarState {
    should_move: bool,
}

impl Render for TitleBarState {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl RenderOnce for TitleBarShell {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let is_client_decorated = matches!(window.window_decorations(), Decorations::Client { .. });
        let is_linux = cfg!(target_os = "linux");

        let state = window.use_state(cx, |_, _| TitleBarState { should_move: false });

        div().flex_shrink_0().child(
            div()
                .id("title-bar")
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .h(TITLE_BAR_HEIGHT)
                .pl(TITLE_BAR_LEFT_PADDING)
                .border_b_1()
                .border_color(cx.theme().title_bar_border)
                .bg(cx.theme().title_bar)
                .refine_style(&self.style)
                // Upstream wires double-click to zoom here on Linux and macOS. Dropped: this
                // window does not maximize.
                .on_mouse_down_out(window.listener_for(&state, |state, _, _, _| {
                    state.should_move = false;
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    window.listener_for(&state, |state, _, _, _| {
                        state.should_move = true;
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    window.listener_for(&state, |state, _, _, _| {
                        state.should_move = false;
                    }),
                )
                .on_mouse_move(window.listener_for(&state, |state, _, window, _| {
                    if state.should_move {
                        state.should_move = false;
                        window.start_window_move();
                    }
                }))
                .child(
                    h_flex()
                        .id("bar")
                        .window_control_area(WindowControlArea::Drag)
                        .when(window.is_fullscreen(), |this| this.pl_3())
                        .h_full()
                        .justify_between()
                        .flex_shrink_0()
                        .flex_1()
                        .when(is_linux && is_client_decorated, |this| {
                            this.child(
                                div()
                                    .top_0()
                                    .left_0()
                                    .absolute()
                                    .size_full()
                                    .h_full()
                                    .on_mouse_down(MouseButton::Right, move |ev, window, _| {
                                        window.show_window_menu(ev.position)
                                    }),
                            )
                        })
                        .children(self.children),
                )
                .child(WindowControls {
                    on_close_window: self.on_close_window,
                }),
        )
    }
}
