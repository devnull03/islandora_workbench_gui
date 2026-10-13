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
/// `has_key` says whether the config sets a key at all (Workbench checks presence, so
/// `password: 12345` counts); `env_has` says whether a variable is already in the app's
/// environment; relative paths resolve against `workbench_dir`, where Workbench runs.
///
/// `Err` when Workbench could not log in whatever the user typed: no username and no
/// credentials file means there is no one to log in as.
pub fn secret_needed(
    config_text: impl Fn(&str) -> Option<String>,
    has_key: impl Fn(&str) -> bool,
    env_has: impl Fn(&str) -> bool,
    workbench_dir: &Path,
) -> Result<Option<Secret>, String> {
    let username = config_text("username");
    let credentials = config_text("credentials_file_path");
    if username.is_none() && credentials.is_none() {
        return Err(
            "This server has no credentials file and the config names no username, so \
             Workbench has no one to log in as. Set a credentials file for the server in \
             Settings."
                .into(),
        );
    }
    if has_key("password") || env_has("ISLANDORA_WORKBENCH_PASSWORD") {
        return Ok(None);
    }
    Ok(match (&username, credentials) {
        (None, Some(path)) => {
            let encrypted = credentials_file_is_encrypted(&resolve(&path, workbench_dir));
            let has_key_file = has_key("credentials_key_file_path")
                || env_has("ISLANDORA_WORKBENCH_ENCRYPTION_KEY");
            (encrypted && !has_key_file).then_some(Secret::EncryptionKey {
                credentials_file: path,
            })
        }
        _ => Some(Secret::Password { username }),
    })
}

/// Whether a credentials file is a single-line (encrypted) one — the server Test cannot read it.
pub fn credentials_file_is_encrypted(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|text| text.lines().count() == 1)
}

/// `path` as Workbench opens it: `~` expanded, relative to the Workbench folder.
pub fn resolve(path: &str, workbench_dir: &Path) -> std::path::PathBuf {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    let expanded = match (path, path.strip_prefix("~/"), home) {
        ("~", _, Some(home)) => std::path::PathBuf::from(home),
        (_, Some(rest), Some(home)) => Path::new(&home).join(rest),
        _ => path.into(),
    };
    if expanded.is_absolute() {
        expanded
    } else {
        workbench_dir.join(expanded)
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
        let wb = tree.root.as_path();
        let ask = |pairs: &[(&str, &str)], env: &dyn Fn(&str) -> bool| {
            let has = |k: &str| pairs.iter().any(|(key, _)| *key == k);
            secret_needed(lookup(pairs), has, env, wb)
        };

        // A password in the config — any value, even a number — or in the environment.
        assert_eq!(
            ask(&[("password", "12345"), ("username", "a")], &none),
            Ok(None)
        );
        assert_eq!(
            ask(&[("username", "a")], &|v| v
                == "ISLANDORA_WORKBENCH_PASSWORD"),
            Ok(None)
        );
        // A plain credentials file answers it.
        assert_eq!(ask(&[("credentials_file_path", &plain)], &none), Ok(None));
        // An encrypted one needs its key, unless a key file or the variable gives it.
        assert!(matches!(
            ask(&[("credentials_file_path", &sealed)], &none),
            Ok(Some(Secret::EncryptionKey { .. }))
        ));
        assert_eq!(
            ask(
                &[
                    ("credentials_file_path", &sealed),
                    ("credentials_key_file_path", "/k")
                ],
                &none
            ),
            Ok(None)
        );
        // A relative credentials path is Workbench's, not the app's.
        assert!(matches!(
            ask(&[("credentials_file_path", "sealed.yml")], &none),
            Ok(Some(Secret::EncryptionKey { .. }))
        ));
        // A username with no password: Workbench asks for the password.
        assert_eq!(
            ask(&[("username", "admin")], &none),
            Ok(Some(Secret::Password {
                username: Some("admin".into())
            }))
        );
        // Nobody to log in as: refuse before launching rather than ask for a useless password.
        assert!(ask(&[], &none).is_err());
    }
}
