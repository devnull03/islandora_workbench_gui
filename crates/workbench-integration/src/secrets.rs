//! The one secret Workbench may ask for at start-up, and how to hand it over instead.
//!
//! Workbench reads a password from `getpass` when a config names a user but no password, and
//! an encryption key when the credentials file is encrypted (`WorkbenchConfig.get_password`).
//! `getpass` talks to the terminal, not to stdout, so the app cannot see or answer that prompt.
//! Workbench also reads both from the environment (`ISLANDORA_WORKBENCH_PASSWORD`,
//! `ISLANDORA_WORKBENCH_ENCRYPTION_KEY`), so the app asks first and sets the variable for that
//! one run — never written to a file, never logged.

use std::path::Path;

/// What a run will ask for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Secret {
    /// The Drupal user's password.
    Password { username: Option<String> },
    /// The key to the encrypted credentials file.
    EncryptionKey { credentials_file: String },
}

impl Secret {
    pub fn env_var(&self) -> &'static str {
        match self {
            Secret::Password { .. } => "ISLANDORA_WORKBENCH_PASSWORD",
            Secret::EncryptionKey { .. } => "ISLANDORA_WORKBENCH_ENCRYPTION_KEY",
        }
    }

    /// What the dialog asks.
    pub fn question(&self) -> String {
        match self {
            Secret::Password {
                username: Some(user),
            } => format!("Password for the Drupal user {user}"),
            Secret::Password { username: None } => "Password for the Drupal user".to_string(),
            Secret::EncryptionKey { credentials_file } => {
                format!("Encryption key for {credentials_file}")
            }
        }
    }
}

/// What the config, as it will be run, leaves Workbench to ask for — following
/// `WorkbenchConfig.get_password` step by step. `config_text` looks a setting up as text;
/// `env_has` says whether a variable is already in the app's environment.
pub fn secret_needed(
    config_text: impl Fn(&str) -> Option<String>,
    env_has: impl Fn(&str) -> bool,
) -> Option<Secret> {
    if config_text("password").is_some() || env_has("ISLANDORA_WORKBENCH_PASSWORD") {
        return None;
    }
    let username = config_text("username");
    let credentials = config_text("credentials_file_path");
    match (&username, credentials) {
        (None, Some(path)) => {
            let encrypted = std::fs::read_to_string(expand_home(&path))
                .is_ok_and(|text| text.lines().count() == 1);
            let has_key = config_text("credentials_key_file_path").is_some()
                || env_has("ISLANDORA_WORKBENCH_ENCRYPTION_KEY");
            (encrypted && !has_key).then_some(Secret::EncryptionKey {
                credentials_file: path,
            })
        }
        _ => Some(Secret::Password { username }),
    }
}

/// Whether a credentials file is a single-line (encrypted) one — the server Test cannot read it.
pub fn credentials_file_is_encrypted(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|text| text.lines().count() == 1)
}

fn expand_home(path: &str) -> std::path::PathBuf {
    match (path.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => Path::new(&home).join(rest),
        _ => path.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::test_support::TempTree;

    fn lookup<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn follows_workbench_password_order() {
        let tree = TempTree::new();
        let plain = tree.write("plain.yml", "username: admin\npassword: x\n");
        let sealed = tree.write("sealed.yml", "gAAAAABkZXhhbXBsZQ==\n");
        let (plain, sealed) = (plain.display().to_string(), sealed.display().to_string());
        let none = |_: &str| false;

        // A password in the config, or in the environment: nothing to ask.
        assert_eq!(secret_needed(lookup(&[("password", "x")]), none), None);
        assert_eq!(
            secret_needed(lookup(&[]), |v| v == "ISLANDORA_WORKBENCH_PASSWORD"),
            None
        );
        // A plain credentials file answers it.
        assert_eq!(
            secret_needed(lookup(&[("credentials_file_path", &plain)]), none),
            None
        );
        // An encrypted one needs its key, unless a key file or the variable gives it.
        assert!(matches!(
            secret_needed(lookup(&[("credentials_file_path", &sealed)]), none),
            Some(Secret::EncryptionKey { .. })
        ));
        assert_eq!(
            secret_needed(
                lookup(&[
                    ("credentials_file_path", &sealed),
                    ("credentials_key_file_path", "/k")
                ]),
                none
            ),
            None
        );
        // A username with no password, or nothing at all: Workbench asks for the password.
        assert_eq!(
            secret_needed(lookup(&[("username", "admin")]), none),
            Some(Secret::Password {
                username: Some("admin".into())
            })
        );
        assert!(matches!(
            secret_needed(lookup(&[]), none),
            Some(Secret::Password { username: None })
        ));
    }
}
