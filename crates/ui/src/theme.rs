//! The app's theme, ported from the Claude Design canvas `4c5759a2` — the `Config Builder
//! Mockups` palette and the `Design Language` spec's §02 token map and §03 type rules.
//!
//! The mockups are drawn dark — near-black grounds, amber `#f99d2a` accent, 4px corners, no
//! shadows. The design system's own tokens are the light half (paper `#f7f4ee`, ink `#17140f`,
//! rust `#a8410f`), so the two modes use different accents from the same hue family: amber has
//! no contrast on paper and rust has none on near-black.
//!
//! `theme.json` is a gpui-component `ThemeSet`, embedded at compile time and installed over the
//! library defaults. Colours belong there, not in element code — a `bg(rgb(0x1a1a1a))` anywhere
//! in the app is a bug, because it will not follow the mode. Dimensions that do not vary by mode
//! belong in [`crate::tokens`].

use gpui::App;
use gpui_component::{Theme, ThemeSet};
use std::borrow::Cow;
use std::rc::Rc;

/// Archivo for prose and labels, IBM Plex Mono for anything the tool will parse. Bundled rather
/// than requested from the OS so the app looks the same on a machine that has neither.
const FONTS: [&[u8]; 5] = [
    include_bytes!("../../../assets/fonts/Archivo-Regular.ttf"),
    include_bytes!("../../../assets/fonts/Archivo-Medium.ttf"),
    include_bytes!("../../../assets/fonts/Archivo-SemiBold.ttf"),
    include_bytes!("../../../assets/fonts/IBMPlexMono-Regular.ttf"),
    include_bytes!("../../../assets/fonts/IBMPlexMono-Medium.ttf"),
];

/// A light/dark pair the user can pick in Settings → Appearance (plan 7.1). Qrate is the
/// design's own; the others are generated from it by `scripts/gen-themes.py`, so every palette
/// keeps the same surface hierarchy and only its hues change.
pub struct Palette {
    pub key: &'static str,
    pub label: &'static str,
    dark: &'static str,
    light: &'static str,
}

pub const PALETTES: &[Palette] = &[
    Palette {
        key: "qrate",
        label: "Qrate",
        dark: include_str!("../../../assets/themes/qrate-dark.json"),
        light: include_str!("../../../assets/themes/qrate-light.json"),
    },
    Palette {
        key: "gruvbox",
        label: "Gruvbox",
        dark: include_str!("../../../assets/themes/gruvbox-dark.json"),
        light: include_str!("../../../assets/themes/gruvbox-light.json"),
    },
    Palette {
        key: "everforest",
        label: "Everforest",
        dark: include_str!("../../../assets/themes/everforest-dark.json"),
        light: include_str!("../../../assets/themes/everforest-light.json"),
    },
];

/// The palette for a stored key; Qrate for anything unknown.
pub fn palette(key: &str) -> &'static Palette {
    PALETTES
        .iter()
        .find(|p| p.key == key)
        .unwrap_or(&PALETTES[0])
}

/// Load the bundled fonts and the Qrate pair. Call once, after `gpui_component::init`, which is
/// what puts a `Theme` global in place.
pub fn init(cx: &mut App) {
    // Before the theme, so the families it names are already resolvable. A failure here is not
    // fatal — the platform's default face is ugly, not broken — but it is worth a log line.
    if let Err(err) = cx
        .text_system()
        .add_fonts(FONTS.iter().map(|f| Cow::Borrowed(*f)).collect())
    {
        log::warn!("bundled fonts failed to load, falling back to system faces: {err}");
    }
    install(&PALETTES[0], cx);
}

/// Make `palette` the light and dark themes, keeping whichever mode is showing.
pub fn install(palette: &Palette, cx: &mut App) {
    for json in [palette.dark, palette.light] {
        let set: ThemeSet = serde_json::from_str(json).expect("bundled themes parse");
        for config in set.themes {
            let config = Rc::new(config);
            let theme = Theme::global_mut(cx);
            if config.mode.is_dark() {
                theme.dark_theme = config;
            } else {
                theme.light_theme = config;
            }
        }
    }
    // `Theme::change` is what copies the config into the live colours.
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
        let parse = |json: &str| {
            let set: ThemeSet = serde_json::from_str(json).unwrap();
            set.themes.into_iter().next().unwrap()
        };
        let (dark, light) = (&parse(PALETTES[0].dark), &parse(PALETTES[0].light));
        assert!(dark.mode.is_dark() && !light.mode.is_dark());

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
        // Bundled families, named so gpui resolves the faces `theme::init` just registered.
        // A typo here is silent: the config keeps `None` and the platform default renders.
        for t in [dark, light] {
            assert_eq!(t.font_family.as_deref(), Some("Archivo"));
            assert_eq!(t.mono_font_family.as_deref(), Some("IBM Plex Mono"));
        }

        // Every generated palette parses, has both modes, and keeps the fonts.
        for palette in PALETTES {
            let (d, l) = (parse(palette.dark), parse(palette.light));
            assert!(d.mode.is_dark() && !l.mode.is_dark(), "{}", palette.key);
            assert_eq!(d.font_family.as_deref(), Some("Archivo"));
            assert!(d.colors.primary.is_some() && l.colors.background.is_some());
        }

        // Font sizes are deliberately absent: the gpui-component defaults scale with the
        // platform, and pinning them made the whole app read a size too small.
        assert_eq!(dark.font_size, None);
    }
}
