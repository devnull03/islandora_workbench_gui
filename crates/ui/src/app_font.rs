//! Applying the theme's UI font family.
//!
//! gpui gives every window the platform UI font and never consults the theme for it, so a bundled
//! family is inert until a root element asks. One call per window root is enough — font family
//! inherits down the element tree, and the few places that want mono set
//! `cx.theme().mono_font_family` themselves.

use gpui::{App, Styled};
use gpui_component::ActiveTheme;

pub trait AppFont: Styled + Sized {
    fn app_font(self, cx: &App) -> Self {
        self.font_family(cx.theme().font_family.clone())
    }
}

impl<T: Styled> AppFont for T {}
