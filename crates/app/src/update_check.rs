//! Checking whether a newer release exists, and the status-bar item that says so.
//!
//! Notify-only: this finds an update and points the user at it. Nothing is downloaded and nothing
//! is applied — the user installs the new version themselves, the same way they installed this one.
//!
//! The feed is a JSON manifest attached to each release, so `releases/latest/download/stable.json`
//! is a URL that always resolves to the newest published (non-prerelease) release. The payload
//! shape is the one a *signed* feed would carry — schema number, per-artifact size and sha256 —
//! so growing into self-applying updates means wrapping this payload in a signature envelope and
//! serving it from somewhere the release pipeline cannot forge, not redesigning it. See
//! `scripts/build-update-manifest.sh`, which writes it.

use std::time::Duration;

use gpui::*;
use gpui_component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex};
use serde::Deserialize;
use settings::AppSettings;

use crate::app_menus::REPO_URL;

/// Setting key for whether to check for updates at startup. On by default — a user who never
/// hears about a fix does not get it.
pub const AUTO_UPDATE_KEY: &str = "automatic_updates";

/// The manifest attached to the newest published release. `latest` skips pre-releases, so a beta
/// tag cannot offer itself to someone on stable.
const FEED_URL: &str =
    "https://github.com/devnull03/islandora_workbench_gui/releases/latest/download/stable.json";

/// Ignored fields are the point: `artifacts`, `schema` and the rest exist in the feed for the
/// updater this will grow into, and are deliberately not read yet.
#[derive(Debug, Deserialize)]
struct Manifest {
    version: String,
    release_notes_url: String,
}

/// A release newer than the running build, once one has been found. Absent until then, which is
/// also what the status bar reads to decide whether it has anything to say.
#[derive(Clone, Debug)]
pub struct AvailableUpdate {
    pub version: SharedString,
    pub release_notes_url: SharedString,
}

impl Global for AvailableUpdate {}

pub fn automatic_updates(cx: &App) -> bool {
    AppSettings::get(cx)
        .values
        .get(AUTO_UPDATE_KEY)
        .map(|value| value.bool())
        .unwrap_or(true)
}

/// Ask the feed once, in the background, and publish the answer if it is newer than us.
///
/// Once per launch, not on a timer: the app cannot install the update anyway, so a second ask an
/// hour later tells the user something they were already told. Failure is silent — an update check
/// that interrupts someone because their wifi is down is worse than no update check.
pub fn check_on_startup(cx: &mut App) {
    if !automatic_updates(cx) {
        return;
    }
    cx.spawn(async move |cx| {
        let Some(manifest) = cx
            .background_executor()
            .spawn(async move { fetch(FEED_URL) })
            .await
        else {
            return;
        };
        if !is_newer(&manifest.version, env!("CARGO_PKG_VERSION")) {
            log::info!("update check: {} is current", env!("CARGO_PKG_VERSION"));
            return;
        }
        log::info!("update check: {} is available", manifest.version);
        _ = cx.update(|cx| {
            cx.set_global(AvailableUpdate {
                version: manifest.version.into(),
                release_notes_url: manifest.release_notes_url.into(),
            });
        });
    })
    .detach();
}

fn fetch(url: &str) -> Option<Manifest> {
    let response = ureq::get(url)
        .timeout(Duration::from_secs(10))
        .call()
        .inspect_err(|err| log::debug!("update check failed: {err}"))
        .ok()?;
    response
        .into_json::<Manifest>()
        .inspect_err(|err| log::warn!("update feed is not a manifest we understand: {err}"))
        .ok()
}

/// Whether `candidate` is a later release than `current`.
///
/// Hand-rolled rather than pulling in semver: the only comparison needed is "is the feed ahead of
/// us", and both sides are versions this project itself produces. A pre-release suffix loses to
/// the same numbers without one (`0.2.0-alpha.1` < `0.2.0`) and is otherwise compared as text,
/// which orders `alpha.1 < alpha.2 < beta.1` correctly and is all the ordering we ship.
///
/// ponytail: reach for the `semver` crate if channels or build metadata ever enter the feed.
fn is_newer(candidate: &str, current: &str) -> bool {
    fn parts(version: &str) -> ([u32; 3], String) {
        let (core, pre) = version.split_once('-').unwrap_or((version, ""));
        let core = core.split_once('+').map(|(c, _)| c).unwrap_or(core);
        let mut numbers = [0; 3];
        for (slot, text) in numbers.iter_mut().zip(core.split('.')) {
            *slot = text.parse().unwrap_or(0);
        }
        (numbers, pre.to_string())
    }

    let (candidate_core, candidate_pre) = parts(candidate);
    let (current_core, current_pre) = parts(current);
    if candidate_core != current_core {
        return candidate_core > current_core;
    }
    match (candidate_pre.is_empty(), current_pre.is_empty()) {
        // Same numbers, and only one of them is a release: the release is the later one.
        (true, false) => true,
        (false, true) => false,
        _ => candidate_pre > current_pre,
    }
}

/// Status-bar item: silent until there is an update, then a link to the release notes.
pub struct UpdateIndicator {
    /// Redraws when the check finishes, which is the only time this has anything to show.
    _sub: Subscription,
}

impl UpdateIndicator {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            _sub: cx.observe_global::<AvailableUpdate>(|_this, cx| cx.notify()),
        }
    }

    /// Whether the item has anything to draw — the status bar asks before allotting it a divider.
    pub fn occupied(cx: &App) -> bool {
        cx.has_global::<AvailableUpdate>()
    }
}

impl Render for UpdateIndicator {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(update) = cx.try_global::<AvailableUpdate>().cloned() else {
            return div();
        };
        let hover_bg = cx.theme().secondary_hover;

        div().child(
            h_flex()
                .id("update-available")
                .gap_1()
                .px(px(4.))
                .py(px(2.))
                .rounded_md()
                .cursor_pointer()
                .text_color(cx.theme().primary)
                .hover(move |this| this.bg(hover_bg))
                .child(Icon::new(IconName::ArrowDown).small())
                .child(format!("Update to {}", update.version))
                .tooltip(|window, cx| {
                    gpui_component::tooltip::Tooltip::new("Open the release notes to download")
                        .build(window, cx)
                })
                .on_click(move |_, _, cx| cx.open_url(&update.release_notes_url)),
        )
    }
}

/// Where Help ▸ Check for Updates sends someone who has the check switched off, or who asked
/// before the answer came back.
pub fn releases_url() -> String {
    format!("{REPO_URL}/releases/latest")
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    /// The whole risk in a notify-only updater is nagging: telling someone to "upgrade" to what
    /// they are already running, or to something older. Both are ordering bugs.
    #[test]
    fn only_a_later_release_counts_as_newer() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(is_newer("1.0.0", "0.9.9"));

        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(!is_newer("0.9.9", "1.0.0"));
    }

    #[test]
    fn a_prerelease_loses_to_the_release_of_the_same_version() {
        // The case that ships today: alpha builds must be offered the real 0.1.0.
        assert!(is_newer("0.1.0", "0.1.0-alpha.1"));
        assert!(!is_newer("0.1.0-alpha.1", "0.1.0"));
        assert!(is_newer("0.1.0-alpha.2", "0.1.0-alpha.1"));
        assert!(!is_newer("0.1.0-alpha.1", "0.1.0-alpha.2"));
    }

    /// A feed that has gone strange must not be read as "you are out of date" — the malformed
    /// half parses as zeroes, which is older than anything real.
    #[test]
    fn a_malformed_version_never_prompts_an_upgrade() {
        assert!(!is_newer("not-a-version", "0.1.0"));
        assert!(!is_newer("", "0.1.0"));
    }
}
