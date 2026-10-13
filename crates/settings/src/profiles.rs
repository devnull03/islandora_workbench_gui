//! Profiles (Stage 5, mockup `3c`): everything institutional — servers, the config library,
//! the input source and its values, preprocess scripts, builder defaults — bundled under a name,
//! so one machine can serve several institutions and a setup can be handed over as a file.
//!
//! **The active profile lives in [`AppSettings`]'s ordinary fields.** Every existing reader
//! (`values`, `server_configs`, `task_configs`, the defaults) keeps working unchanged; the
//! *other* profiles are stored as [`Profile`] snapshots. Switching snapshots the live fields
//! into the outgoing profile and loads the incoming one over them. Machine facts — where
//! Workbench and Python are, the theme, window sizes — are never in a profile.
//!
//! Plan question Q1 (does the config library belong to the profile?) is answered with its
//! recommended default: a profile carries a *pointer* to a library folder
//! (`config_library_dir`) and its list of configs; the YAML files stay ordinary files on this
//! machine, and two profiles may point at the same folder.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{AppSettings, ServerConfig, TaskConfig, Val};

/// Settings keys that belong to a profile. Everything else in `values` is about this machine.
pub const PROFILE_KEYS: &[&str] = &[
    "input_source",
    "gdrive_link",
    "source_csv",
    "source_worksheet",
    "ingest_files_dir",
    "preprocessor",
    "preprocess_scripts_dir",
    "config_library_dir",
    "auto_accept_prompts",
    "builder_show_yaml",
    "builder_check_paths",
    "builder_warn_clashes",
];

/// The name the first profile gets, and the one an upgraded settings file starts with.
pub const DEFAULT_PROFILE: &str = "Default";

/// One profile, as stored for an inactive profile and as exported to a `.profile.yml`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    #[serde(default)]
    pub values: BTreeMap<String, ProfileValue>,
    #[serde(default)]
    pub servers: Vec<ProfileServer>,
    #[serde(default)]
    pub configs: Vec<ProfileConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_server: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_config: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProfileValue {
    Bool(bool),
    Text(String),
}

/// A server, by reference: the credentials *file's path*, never its contents. An exported
/// profile therefore carries no password (Stage 5's one hard rule about export).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileServer {
    pub label: String,
    pub url: String,
    #[serde(default)]
    pub credentials_file: String,
    #[serde(default)]
    pub needs_confirmation: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileConfig {
    pub label: String,
    #[serde(default)]
    pub task: String,
    pub file: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// What a profile card shows about it, one chip each: `2 servers`, `6 configs`,
/// `source: CSV file`.
pub fn facts(profile: &Profile) -> Vec<String> {
    let plural = |n: usize, one: &str| format!("{n} {one}{}", if n == 1 { "" } else { "s" });
    let mut parts = vec![
        plural(profile.servers.len(), "server"),
        plural(profile.configs.len(), "config"),
    ];
    if let Some(ProfileValue::Text(source)) = profile.values.get("input_source") {
        parts.push(format!(
            "source: {}",
            match source.as_str() {
                "csv-file" => "CSV file",
                "google-sheet" => "Google Sheet",
                other => other,
            }
        ));
    }
    parts
}

impl AppSettings {
    /// The active profile, as a snapshot of the live fields.
    pub fn snapshot_profile(&self) -> Profile {
        Profile {
            name: self.active_profile.clone(),
            values: PROFILE_KEYS
                .iter()
                .filter_map(|key| {
                    self.values.get(*key).map(|value| {
                        let value = match value {
                            Val::Bool(b) => ProfileValue::Bool(*b),
                            Val::Text(t) => ProfileValue::Text(t.to_string()),
                        };
                        ((*key).to_string(), value)
                    })
                })
                .collect(),
            servers: self
                .server_configs
                .iter()
                .map(|s| ProfileServer {
                    label: s.label.to_string(),
                    url: s.server_url.to_string(),
                    credentials_file: s.credentials_file.to_string(),
                    needs_confirmation: s.needs_confirmation,
                })
                .collect(),
            configs: self
                .task_configs
                .iter()
                .map(|c| ProfileConfig {
                    label: c.label.to_string(),
                    task: c.task_name.to_string(),
                    file: c.file_path.to_string(),
                    description: c.description.to_string(),
                })
                .collect(),
            default_server: self.default_server.as_ref().map(|s| s.to_string()),
            default_config: self.default_task_config.as_ref().map(|s| s.to_string()),
        }
    }

    /// Load `profile` over the live institutional fields; machine settings are untouched.
    fn load_profile(&mut self, profile: &Profile) {
        for key in PROFILE_KEYS {
            self.values.remove(*key);
        }
        for (key, value) in &profile.values {
            if PROFILE_KEYS.contains(&key.as_str()) {
                let value = match value {
                    ProfileValue::Bool(b) => Val::Bool(*b),
                    ProfileValue::Text(t) => Val::Text(t.clone().into()),
                };
                self.values.insert(key.clone(), value);
            }
        }
        self.server_configs = profile
            .servers
            .iter()
            .map(|s| ServerConfig {
                label: s.label.clone().into(),
                server_url: s.url.clone().into(),
                credentials_file: s.credentials_file.clone().into(),
                needs_confirmation: s.needs_confirmation,
                last_check: self
                    .check_cache
                    .get(&(s.url.clone(), s.credentials_file.clone()))
                    .cloned(),
            })
            .collect();
        self.task_configs = profile
            .configs
            .iter()
            .map(|c| TaskConfig {
                label: c.label.clone().into(),
                task_name: c.task.clone().into(),
                file_path: c.file.clone().into(),
                description: c.description.clone().into(),
            })
            .collect();
        self.default_server = profile.default_server.clone().map(Into::into);
        self.default_task_config = profile.default_config.clone().map(Into::into);
        self.active_profile = profile.name.clone();
        self.profile_generation = self.profile_generation.wrapping_add(1);
    }

    /// Every profile name, the active one included, in display order (the active one sits at
    /// `active_profile_index`, so switching does not reshuffle the list).
    pub fn profile_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.profiles.iter().map(|p| p.name.clone()).collect();
        let at = self.active_profile_index.min(names.len());
        names.insert(at, self.active_profile.clone());
        names
    }

    /// Every profile as it stands, the active one from the live fields.
    pub fn all_profiles(&self) -> Vec<Profile> {
        self.profile_names()
            .into_iter()
            .map(|name| {
                if name == self.active_profile {
                    self.snapshot_profile()
                } else {
                    self.profiles
                        .iter()
                        .find(|p| p.name == name)
                        .cloned()
                        .unwrap_or_default()
                }
            })
            .collect()
    }

    /// Make `name` the active profile. The outgoing one is saved as it stands.
    /// Add or update a config-library entry, matched on its file, in profile `profile` —
    /// the live lists when it is the active one, its stored snapshot otherwise. A builder
    /// opened under one profile and saved after a switch still files its config there.
    ///
    /// A profile renamed or deleted since the builder opened is no longer there to receive it;
    /// then the active profile does, rather than the save landing in no library at all.
    pub fn upsert_library_entry(&mut self, profile: &str, entry: TaskConfig) {
        let known = self.profiles.iter().any(|p| p.name == profile);
        if profile == self.active_profile || !known {
            match self
                .task_configs
                .iter_mut()
                .find(|c| c.file_path == entry.file_path)
            {
                Some(existing) => *existing = entry,
                None => self.task_configs.push(entry),
            }
            return;
        }
        let Some(stored) = self.profiles.iter_mut().find(|p| p.name == profile) else {
            return;
        };
        let config = ProfileConfig {
            label: entry.label.to_string(),
            task: entry.task_name.to_string(),
            file: entry.file_path.to_string(),
            description: entry.description.to_string(),
        };
        match stored.configs.iter_mut().find(|c| c.file == config.file) {
            Some(existing) => *existing = config,
            None => stored.configs.push(config),
        }
    }

    pub fn switch_profile(&mut self, name: &str) -> Result<(), String> {
        if name == self.active_profile {
            return Ok(());
        }
        let mut all = self.all_profiles();
        let Some(index) = all.iter().position(|p| p.name == name) else {
            return Err(format!("There is no profile called {name}."));
        };
        let incoming = all.remove(index);
        for server in &self.server_configs {
            if let Some(check) = &server.last_check {
                self.check_cache.insert(
                    (
                        server.server_url.to_string(),
                        server.credentials_file.to_string(),
                    ),
                    check.clone(),
                );
            }
        }
        self.profiles = all;
        self.active_profile_index = index;
        self.load_profile(&incoming);
        Ok(())
    }

    /// Add a profile: empty, or a copy of `from`. Returns the name it got (made unique).
    pub fn add_profile(&mut self, name: &str, from: Option<&str>) -> Result<String, String> {
        let name = self.unique_name(name.trim());
        let mut profile = match from {
            Some(source) => self
                .all_profiles()
                .into_iter()
                .find(|p| p.name == source)
                .ok_or_else(|| format!("There is no profile called {source}."))?,
            None => Profile::default(),
        };
        profile.name = name.clone();
        self.profiles.push(profile);
        Ok(name)
    }

    pub fn rename_profile(&mut self, from: &str, to: &str) -> Result<(), String> {
        let to = to.trim();
        if to.is_empty() {
            return Err("A profile needs a name.".into());
        }
        if from != to && self.profile_names().iter().any(|n| n == to) {
            return Err(format!("There is already a profile called {to}."));
        }
        if self.active_profile == from {
            self.active_profile = to.to_string();
            return Ok(());
        }
        let profile = self
            .profiles
            .iter_mut()
            .find(|p| p.name == from)
            .ok_or_else(|| format!("There is no profile called {from}."))?;
        profile.name = to.to_string();
        Ok(())
    }

    /// Delete an inactive profile. The active one cannot go: switch away first.
    pub fn delete_profile(&mut self, name: &str) -> Result<(), String> {
        if name == self.active_profile {
            return Err("Switch to another profile before deleting this one.".into());
        }
        let Some(display) = self.profile_names().iter().position(|n| n == name) else {
            return Err(format!("There is no profile called {name}."));
        };
        self.profiles.retain(|p| p.name != name);
        if display < self.active_profile_index {
            self.active_profile_index -= 1;
        }
        Ok(())
    }

    /// Add an imported profile, renamed if its name is taken.
    pub fn import_profile(&mut self, mut profile: Profile) -> String {
        let fallback = if profile.name.trim().is_empty() {
            "Imported".to_string()
        } else {
            profile.name.trim().to_string()
        };
        profile.name = self.unique_name(&fallback);
        profile
            .values
            .retain(|key, _| PROFILE_KEYS.contains(&key.as_str()));
        let name = profile.name.clone();
        self.profiles.push(profile);
        name
    }

    /// Whether a profile is called `name`.
    pub fn has_profile(&self, name: &str) -> bool {
        name == self.active_profile || self.profiles.iter().any(|p| p.name == name)
    }

    /// Import `profile` over the one with the same name — a colleague's updated copy — keeping
    /// its place in the list. Refused while it is the active profile and a run could be using
    /// it (the caller checks); a name nobody has is simply added.
    pub fn replace_profile(&mut self, mut profile: Profile) -> String {
        let name = profile.name.trim().to_string();
        profile.name = name.clone();
        profile
            .values
            .retain(|key, _| PROFILE_KEYS.contains(&key.as_str()));
        if name == self.active_profile {
            self.load_profile(&profile);
        } else if let Some(existing) = self.profiles.iter_mut().find(|p| p.name == name) {
            *existing = profile;
        } else {
            self.profiles.push(profile);
        }
        name
    }

    fn unique_name(&self, wanted: &str) -> String {
        let base = if wanted.is_empty() { "Profile" } else { wanted };
        let names = self.profile_names();
        if !names.iter().any(|n| n == base) {
            return base.to_string();
        }
        (2..)
            .map(|n| format!("{base} {n}"))
            .find(|candidate| !names.contains(candidate))
            .expect("an unused name exists")
    }
}

/// A `.profile.yml` for `profile`: paths and settings only — never a password, because a
/// profile never holds one (servers carry their credentials *file's* path).
pub fn export_yaml(profile: &Profile) -> Result<String, String> {
    let body = serde_yaml::to_string(profile).map_err(|e| e.to_string())?;
    Ok(format!(
        "# Islandora Workbench GUI profile \"{}\".\n# Credentials files are referenced by path; \
         their contents are not included.\n{body}",
        profile.name
    ))
}

pub fn import_yaml(text: &str) -> Result<Profile, String> {
    serde_yaml::from_str(text).map_err(|e| format!("Not a profile file: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> AppSettings {
        let mut s = AppSettings::default();
        s.values
            .insert("input_source".into(), Val::Text("csv-file".into()));
        s.values
            .insert("workbench_path".into(), Val::Text("/opt/wb".into()));
        s.server_configs.push(ServerConfig {
            label: "prod".into(),
            server_url: "https://prod.example".into(),
            credentials_file: "/secrets/prod.yml".into(),
            needs_confirmation: true,
            last_check: None,
        });
        s
    }

    /// Re-importing a profile replaces it in place, the active one included.
    #[test]
    fn replacing_a_profile_keeps_its_place() {
        let mut s = settings();
        s.add_profile("B", None).unwrap();
        let mut updated = s.all_profiles()[0].clone();
        updated.servers.clear();
        s.replace_profile(updated);
        assert!(
            s.server_configs.is_empty(),
            "the active profile was reloaded"
        );
        let mut b = s.all_profiles()[1].clone();
        b.values
            .insert("workbench_path".into(), ProfileValue::Text("/x".into()));
        s.replace_profile(b);
        assert_eq!(s.profile_names(), vec!["Default", "B"]);
        assert!(
            !s.all_profiles()[1].values.contains_key("workbench_path"),
            "machine keys never come in with a profile"
        );
    }

    /// A save from a builder opened under another profile lands in that profile's library.
    #[test]
    fn library_entries_go_to_the_profile_they_came_from() {
        let mut s = settings();
        let other = s.add_profile("Other", None).unwrap();
        s.switch_profile(&other).unwrap();
        let entry = TaskConfig {
            label: "Items".into(),
            task_name: "create".into(),
            file_path: "/lib/items.yml".into(),
            description: "".into(),
        };
        s.upsert_library_entry(DEFAULT_PROFILE, entry.clone());
        assert!(s.task_configs.is_empty());
        s.upsert_library_entry(DEFAULT_PROFILE, entry.clone());
        s.switch_profile(DEFAULT_PROFILE).unwrap();
        assert_eq!(s.task_configs.len(), 1);
        // Opened under a profile that has since gone: the active one takes it.
        s.task_configs.clear();
        s.upsert_library_entry("Renamed away", entry);
        assert_eq!(s.task_configs.len(), 1);
    }

    /// A server's Test result survives switching away and back, without being stored in the
    /// profile (which is what an export writes).
    #[test]
    fn test_results_survive_a_round_trip() {
        let mut s = settings();
        s.server_configs[0].last_check = Some(crate::CheckResult::now(true, Some(true), "ok"));
        let other = s.add_profile("Other", None).unwrap();
        s.switch_profile(&other).unwrap();
        assert!(!export_yaml(&s.all_profiles()[0]).unwrap().contains("ok"));
        s.switch_profile(DEFAULT_PROFILE).unwrap();
        assert!(s.server_configs[0].last_check.is_some());
    }

    /// Deleting a profile listed before the active one keeps the active one where it was in
    /// the list, and renaming the active profile renames only it.
    #[test]
    fn delete_before_the_active_profile_and_rename_it() {
        let mut s = settings();
        s.add_profile("B", None).unwrap();
        s.add_profile("C", None).unwrap();
        s.switch_profile("C").unwrap();
        assert_eq!(s.profile_names(), vec!["Default", "B", "C"]);
        s.delete_profile("Default").unwrap();
        assert_eq!(s.profile_names(), vec!["B", "C"]);
        assert_eq!(s.active_profile, "C");
        s.rename_profile("C", "Main").unwrap();
        assert_eq!(s.profile_names(), vec!["B", "Main"]);
        assert!(s.rename_profile("Main", "B").is_err());
    }

    #[test]
    fn switching_swaps_institutional_fields_and_keeps_machine_ones() {
        let mut s = settings();
        let name = s.add_profile("SFU", None).unwrap();
        s.switch_profile(&name).unwrap();
        assert_eq!(s.active_profile, "SFU");
        assert!(s.server_configs.is_empty());
        assert!(!s.values.contains_key("input_source"));
        // Machine facts stay.
        assert_eq!(s.values["workbench_path"].text().as_ref(), "/opt/wb");

        s.switch_profile(DEFAULT_PROFILE).unwrap();
        assert_eq!(s.server_configs.len(), 1);
        assert_eq!(s.values["input_source"].text().as_ref(), "csv-file");
        assert_eq!(
            s.profile_names(),
            vec!["Default".to_string(), "SFU".to_string()]
        );
    }

    #[test]
    fn duplicate_rename_delete() {
        let mut s = settings();
        let copy = s.add_profile("Default", Some("Default")).unwrap();
        assert_eq!(copy, "Default 2");
        assert_eq!(s.all_profiles()[1].servers.len(), 1);
        s.rename_profile("Default 2", "UFV").unwrap();
        assert!(s.rename_profile("UFV", "Default").is_err());
        assert!(s.delete_profile("Default").is_err());
        s.delete_profile("UFV").unwrap();
        assert_eq!(s.profile_names(), vec!["Default".to_string()]);
    }

    /// Stage 5's hard rule: an export carries no secrets.
    #[test]
    fn export_carries_paths_not_secrets_and_round_trips() {
        let s = settings();
        let profile = s.snapshot_profile();
        let yaml = export_yaml(&profile).unwrap();
        assert!(yaml.contains("/secrets/prod.yml"));
        assert!(!yaml.to_lowercase().contains("password"));
        assert!(!yaml.contains("workbench_path"));
        assert_eq!(import_yaml(&yaml).unwrap(), profile);

        let mut other = AppSettings::default();
        let name = other.import_profile(import_yaml(&yaml).unwrap());
        assert_eq!(name, "Default 2");
    }

    #[test]
    fn summaries_read_like_the_mockup() {
        assert_eq!(
            facts(&settings().snapshot_profile()),
            ["1 server", "0 configs", "source: CSV file"]
        );
    }
}
