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
use gpui_component::IconName;
use semver::Version;
use serde::Deserialize;
use settings::AppSettings;

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
        cx.update(|cx| {
            cx.set_global(AvailableUpdate {
                version: manifest.version.into(),
                release_notes_url: manifest.release_notes_url.into(),
            });
            // This view is absent from the element tree until the global exists. Refresh every
            // window so the status bar gets a chance to add it immediately.
            cx.refresh_windows();
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
fn is_newer(candidate: &str, current: &str) -> bool {
    let (Ok(candidate), Ok(current)) = (Version::parse(candidate), Version::parse(current)) else {
        return false;
    };
    candidate > current
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
        div().child(
            ui::status_bar_button("update-available")
                .icon(IconName::ArrowDown)
                .label(format!("Update to {}", update.version))
                .tooltip("Open the release notes to download")
                .on_click(move |_, _, cx| cx.open_url(&update.release_notes_url)),
        )
    }
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
        assert!(is_newer("0.1.0-alpha.10", "0.1.0-alpha.9"));
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
