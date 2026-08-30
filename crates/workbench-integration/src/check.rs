//! Two-stage server check, behind the **Test** button on a server row (mockup `3b`).
//!
//! Reachability and credentials are asked separately because they fail separately. An
//! unreachable host says nothing about whether the password is right, so the result reports
//! `credentials_ok: None` and the row says so — "Credentials were not tested" is a true
//! statement, and "credentials failed" would not be.

use std::path::Path;
use std::time::Duration;

use base64::Engine as _;

use crate::read_credentials;

const TIMEOUT: Duration = Duration::from_secs(8);

pub struct ServerCheck {
    pub reachable: bool,
    /// `None` means never attempted, not "failed".
    pub credentials_ok: Option<bool>,
    /// One sentence for the row, already phrased for a human.
    pub message: String,
}

/// Stage 1 reaches the host. Stage 2, only if stage 1 passed, calls Workbench's integration
/// endpoint with the credentials file — which proves the password *and* that the Islandora
/// Workbench Integration module is actually installed, the two things a run needs.
pub fn check_server(url: &str, credentials_file: Option<&Path>) -> ServerCheck {
    let base = url.trim().trim_end_matches('/');
    if base.is_empty() {
        return ServerCheck {
            reachable: false,
            credentials_ok: None,
            message: "No server URL set.".into(),
        };
    }

    match ureq::get(base).timeout(TIMEOUT).call() {
        Ok(_) => {}
        // A 4xx from the bare URL still proves something answered on the other end; only a
        // transport failure means unreachable.
        Err(ureq::Error::Status(code, _)) if code < 500 => {}
        Err(ureq::Error::Status(code, _)) => {
            return ServerCheck {
                reachable: false,
                credentials_ok: None,
                message: format!("HTTP {code}, host unreachable. Credentials were not tested."),
            };
        }
        Err(e) => {
            return ServerCheck {
                reachable: false,
                credentials_ok: None,
                message: format!("Could not reach the host ({e}). Credentials were not tested."),
            };
        }
    }

    let Some(path) = credentials_file.filter(|p| !p.as_os_str().is_empty()) else {
        return ServerCheck {
            reachable: true,
            credentials_ok: None,
            message: "Reachable. No credentials file set, so none were tested.".into(),
        };
    };

    let creds = match read_credentials(path) {
        Ok(c) => c,
        Err(e) => {
            return ServerCheck {
                reachable: true,
                credentials_ok: Some(false),
                message: format!("Reachable, but the credentials file could not be read: {e}"),
            };
        }
    };

    let encoded = base64::engine::general_purpose::STANDARD
        .encode(format!("{}:{}", creds.username, creds.password));
    let endpoint = format!("{base}/islandora_workbench_integration/version");

    match ureq::get(&endpoint)
        .timeout(TIMEOUT)
        .set("Authorization", &format!("Basic {encoded}"))
        .call()
    {
        Ok(_) => ServerCheck {
            reachable: true,
            credentials_ok: Some(true),
            message: "Reachable, credentials accepted.".into(),
        },
        Err(ureq::Error::Status(401, _)) => ServerCheck {
            reachable: true,
            credentials_ok: Some(false),
            message: "Reachable, but the credentials were rejected (401).".into(),
        },
        Err(ureq::Error::Status(403, _)) => ServerCheck {
            reachable: true,
            credentials_ok: Some(false),
            message: "Reachable, credentials accepted, but this user lacks permission (403)."
                .into(),
        },
        Err(ureq::Error::Status(404, _)) => ServerCheck {
            reachable: true,
            credentials_ok: Some(false),
            message: "Reachable, but the Workbench Integration module is not installed (404)."
                .into(),
        },
        Err(ureq::Error::Status(code, _)) => ServerCheck {
            reachable: true,
            credentials_ok: Some(false),
            message: format!("Reachable, but the check returned HTTP {code}."),
        },
        Err(e) => ServerCheck {
            reachable: true,
            credentials_ok: Some(false),
            message: format!("Reachable, but the authenticated check failed: {e}"),
        },
    }
}
