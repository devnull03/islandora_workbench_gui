//! The app's theme, ported from the Claude Design canvas
//! (`4c5759a2-16e4-4f27-b9fb-126fc19f0b30`, `Config Builder Mockups`) and its `qrate-site`
//! design system.
//!
//! The mockups are drawn dark — near-black grounds, amber `#f99d2a` accent, 4px corners, no
//! shadows. The design system's own tokens are the light half (paper `#f7f4ee`, ink `#17140f`,
//! rust `#a8410f`), so the two modes use different accents from the same hue family: amber has
//! no contrast on paper and rust has none on near-black.
//!
//! `theme.json` is a gpui-component `ThemeSet`, embedded at compile time and installed over the
//! library defaults. Colours belong there, not in element code — a `bg(rgb(0x1a1a1a))` anywhere
//! in the app is a bug, because it will not follow the mode.

use gpui::App;
use gpui_component::{Theme, ThemeSet};
use std::rc::Rc;

/// Install the Qrate light/dark pair and re-apply the mode already chosen from the OS.
///
/// Call once, after `gpui_component::init`, which is what puts a `Theme` global in place.
pub fn init(cx: &mut App) {
    let set: ThemeSet =
        serde_json::from_str(include_str!("../theme.json")).expect("theme.json is embedded");

    for config in set.themes {
        let config = Rc::new(config);
        let theme = Theme::global_mut(cx);
        if config.mode.is_dark() {
            theme.dark_theme = config;
        } else {
            theme.light_theme = config;
        }
    }

    // `Theme::change` is what actually copies the config into the live colours. Re-run it with
    // the mode gpui already picked from the OS so this is a restyle, not a mode switch.
    let mode = Theme::global(cx).mode;
    Theme::change(mode, None, cx);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ThemeConfigColors` ignores keys it does not recognise, so a typo in `theme.json` costs a
    /// colour with no error anywhere. Assert a few landed, in both modes.
    #[test]
    fn theme_json_parses_into_two_modes() {
        let set: ThemeSet = serde_json::from_str(include_str!("../theme.json")).unwrap();
        assert_eq!(set.themes.len(), 2);

        let dark = set.themes.iter().find(|t| t.mode.is_dark()).unwrap();
        let light = set.themes.iter().find(|t| !t.mode.is_dark()).unwrap();

        assert_eq!(
            dark.colors.primary.as_ref().map(|c| c.as_ref()),
            Some("#f99d2a")
        );
        assert_eq!(
            light.colors.primary.as_ref().map(|c| c.as_ref()),
            Some("#a8410f")
        );
        assert_eq!(
            dark.colors.title_bar.as_ref().map(|c| c.as_ref()),
            Some("#141414")
        );
        assert_eq!(
            dark.colors.ring.as_ref().map(|c| c.as_ref()),
            Some("#f99d2a")
        );
        assert_eq!(
            light.colors.ring.as_ref().map(|c| c.as_ref()),
            Some("#a8410f")
        );
        assert_eq!(
            dark.colors.link.as_ref().map(|c| c.as_ref()),
            Some("#f99d2a")
        );
        assert_eq!(
            light.colors.link.as_ref().map(|c| c.as_ref()),
            Some("#a8410f")
        );
        assert_eq!(dark.radius, Some(4));
        // Font sizes are deliberately absent: the gpui-component defaults scale with the
        // platform, and pinning them made the whole app read a size too small.
        assert_eq!(dark.font_size, None);
    }
}
