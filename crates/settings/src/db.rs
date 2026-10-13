//! Persistent storage for `AppSettings`.
//!
//! Implementation notes:
//! - Stores one JSON blob in SQLite (single-row KV table).
//! - Writes are debounced in a background thread; mutations enqueue snapshots.
//! - Includes optional main window size and display for restore on launch (not position).
//! - JSON includes `settings_version` for forward-compatible migrations.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::Duration,
};

use anyhow::{Context as _, Result};
use gpui::SharedString;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::{
    AppSettings, CheckResult, MainWindowBounds, SETTINGS_SCHEMA_VERSION, ServerConfig, TaskConfig,
    Val,
};

const APP_DIR: &str = "islandora_workbench_gui";
const DB_FILE: &str = "settings.sqlite3";
const SETTINGS_KEY: &str = "app_settings_v1";

#[derive(Clone, Serialize, Deserialize)]
enum PersistVal {
    Text(String),
    Bool(bool),
}

#[derive(Clone, Serialize, Deserialize)]
struct PersistTaskConfig {
    label: String,
    task_name: String,
    /// Added in schema v3. `serde(default)` is the whole migration, as it was for the server
    /// fields below: a config saved before this existed simply has no description, which is the
    /// truth.
    #[serde(default)]
    description: String,
    file_path: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct PersistServerConfig {
    label: String,
    server_url: String,
    credentials_file: String,
    /// Added in schema v2. `serde(default)` is the whole migration: a v1 blob has neither field,
    /// and "never tested, no confirmation required" is the right reading of its silence.
    #[serde(default)]
    needs_confirmation: bool,
    #[serde(default)]
    last_check: Option<PersistCheckResult>,
}

#[derive(Clone, Serialize, Deserialize)]
struct PersistCheckResult {
    at: u64,
    reachable: bool,
    credentials_ok: Option<bool>,
    message: String,
}

fn default_persist_settings_version() -> u32 {
    1
}

#[derive(Clone, Serialize, Deserialize)]
struct PersistSettings {
    /// Schema version of this blob; missing in older files defaults to `1`.
    #[serde(default = "default_persist_settings_version")]
    settings_version: u32,
    values: HashMap<String, PersistVal>,
    task_configs: Vec<PersistTaskConfig>,
    server_configs: Vec<PersistServerConfig>,
    #[serde(default)]
    main_window_bounds: Option<MainWindowBounds>,
    #[serde(default)]
    default_task_config: Option<String>,
    #[serde(default)]
    default_server: Option<String>,
    /// Added in schema v3 (profiles). A v2 file has neither, which reads as one profile
    /// called "Default" holding everything it already had — the whole migration.
    #[serde(default)]
    active_profile: Option<String>,
    #[serde(default)]
    profiles: Vec<crate::profiles::Profile>,
    #[serde(default)]
    active_profile_index: usize,
}

impl From<&AppSettings> for PersistSettings {
    fn from(s: &AppSettings) -> Self {
        let values = s
            .values
            .iter()
            .map(|(k, v)| {
                let pv = match v {
                    Val::Text(t) => PersistVal::Text(t.to_string()),
                    Val::Bool(b) => PersistVal::Bool(*b),
                };
                (k.clone(), pv)
            })
            .collect();

        let task_configs = s
            .task_configs
            .iter()
            .map(|t| PersistTaskConfig {
                label: t.label.to_string(),
                task_name: t.task_name.to_string(),
                file_path: t.file_path.to_string(),
                description: t.description.to_string(),
            })
            .collect();

        let server_configs = s
            .server_configs
            .iter()
            .map(|srv| PersistServerConfig {
                label: srv.label.to_string(),
                server_url: srv.server_url.to_string(),
                credentials_file: srv.credentials_file.to_string(),
                needs_confirmation: srv.needs_confirmation,
                last_check: srv.last_check.as_ref().map(|c| PersistCheckResult {
                    at: c.at,
                    reachable: c.reachable,
                    credentials_ok: c.credentials_ok,
                    message: c.message.to_string(),
                }),
            })
            .collect();

        Self {
            settings_version: SETTINGS_SCHEMA_VERSION,
            values,
            task_configs,
            server_configs,
            main_window_bounds: s.main_window_bounds.clone(),
            default_task_config: s.default_task_config.as_ref().map(|v| v.to_string()),
            default_server: s.default_server.as_ref().map(|v| v.to_string()),
            active_profile: Some(s.active_profile.clone()),
            profiles: s.profiles.clone(),
            active_profile_index: s.active_profile_index,
        }
    }
}

impl From<PersistSettings> for AppSettings {
    fn from(p: PersistSettings) -> Self {
        let values = p
            .values
            .into_iter()
            .map(|(k, v)| {
                let vv = match v {
                    PersistVal::Text(t) => Val::Text(SharedString::from(t)),
                    PersistVal::Bool(b) => Val::Bool(b),
                };
                (k, vv)
            })
            .collect();

        let task_configs = p
            .task_configs
            .into_iter()
            .map(|t| TaskConfig {
                label: SharedString::from(t.label),
                task_name: SharedString::from(t.task_name),
                file_path: SharedString::from(t.file_path),
                description: SharedString::from(t.description),
            })
            .collect();

        let server_configs = p
            .server_configs
            .into_iter()
            .map(|s| ServerConfig {
                label: SharedString::from(s.label),
                server_url: SharedString::from(s.server_url),
                credentials_file: SharedString::from(s.credentials_file),
                needs_confirmation: s.needs_confirmation,
                last_check: s.last_check.map(|c| CheckResult {
                    at: c.at,
                    reachable: c.reachable,
                    credentials_ok: c.credentials_ok,
                    message: SharedString::from(c.message),
                }),
            })
            .collect();

        Self {
            values,
            task_configs,
            server_configs,
            main_window_bounds: p.main_window_bounds,
            default_task_config: p.default_task_config.map(SharedString::from),
            default_server: p.default_server.map(SharedString::from),
            active_profile: p
                .active_profile
                .unwrap_or_else(|| crate::profiles::DEFAULT_PROFILE.to_string()),
            profiles: p.profiles,
            active_profile_index: p.active_profile_index,
            profile_generation: 0,
            check_cache: Default::default(),
            settings_schema_version: p.settings_version,
        }
    }
}

/// The app's own folder under the OS local data dir — the settings DB, the logs, and anything
/// else the app writes for itself. `None` only when the OS reports no local data dir at all.
pub fn data_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|base| base.join(APP_DIR))
}

fn db_path() -> Result<PathBuf> {
    let base = data_dir().context("Failed to resolve local data dir")?;
    Ok(base.join(DB_FILE))
}

fn ensure_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS settings_kv (
          key TEXT PRIMARY KEY,
          json TEXT NOT NULL
        );
        "#,
    )
    .context("Failed to create schema")?;
    Ok(())
}

fn open_db(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("Create dir {parent:?}"))?;
    }
    let conn = Connection::open(path).with_context(|| format!("Open DB at {path:?}"))?;
    ensure_schema(&conn)?;
    Ok(conn)
}

pub fn load_app_settings() -> Result<AppSettings> {
    let path = db_path()?;
    let conn = open_db(&path)?;
    let json: Option<String> = conn
        .query_row(
            "SELECT json FROM settings_kv WHERE key = ?1",
            params![SETTINGS_KEY],
            |row| row.get(0),
        )
        .optional()
        .context("Query settings")?;

    let Some(json) = json else {
        return Ok(AppSettings::default());
    };

    let persist: PersistSettings =
        serde_json::from_str(&json).context("Deserialize persisted settings")?;
    // Written by a newer build: this one would drop whatever it does not know (profiles, once)
    // on its next save. Keep the newer row aside first, once, so it can be recovered.
    if persist.settings_version > SETTINGS_SCHEMA_VERSION {
        let backup = format!("{SETTINGS_KEY}_from_v{}", persist.settings_version);
        conn.execute(
            "INSERT OR IGNORE INTO settings_kv(key, json) VALUES (?1, ?2)",
            params![backup, json],
        )
        .context("Back up newer settings")?;
    }
    Ok(persist.into())
}

fn save_app_settings_snapshot(snapshot: PersistSettings) -> Result<()> {
    let path = db_path()?;
    let conn = open_db(&path)?;
    let json = serde_json::to_string(&snapshot).context("Serialize settings")?;
    conn.execute(
        "INSERT INTO settings_kv(key, json) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET json = excluded.json",
        params![SETTINGS_KEY, json],
    )
    .context("Upsert settings row")?;
    Ok(())
}

/// Background writer handle. Call `enqueue_save` on every mutation; writes are debounced.
#[derive(Clone)]
pub struct SettingsWriter {
    tx: mpsc::Sender<PersistSettings>,
}

impl SettingsWriter {
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel::<PersistSettings>();

        thread::spawn(move || {
            let debounce = Duration::from_millis(450);
            let mut pending: Option<PersistSettings> = None;

            loop {
                match rx.recv_timeout(debounce) {
                    Ok(s) => {
                        pending = Some(s);
                        // Drain bursts quickly.
                        while let Ok(s2) = rx.try_recv() {
                            pending = Some(s2);
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if let Some(s) = pending.take() {
                            let _ = save_app_settings_snapshot(s);
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        if let Some(s) = pending.take() {
                            let _ = save_app_settings_snapshot(s);
                        }
                        break;
                    }
                }
            }
        });

        Self { tx }
    }

    pub fn enqueue_save(&self, settings: &AppSettings) {
        let _ = self.tx.send(PersistSettings::from(settings));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A schema-2 file (before profiles) loads as one profile, "Default", holding everything it
    /// had, and writes back as schema 3.
    #[test]
    fn a_v2_file_becomes_the_default_profile() {
        let v2 = r#"{
            "settings_version": 2,
            "values": {"input_source": {"Text": "csv-file"}, "workbench_path": {"Text": "/wb"}},
            "task_configs": [{"label": "Main", "task_name": "create", "file_path": "/l/main.yml"}],
            "server_configs": [{"label": "dev", "server_url": "https://dev", "credentials_file": "/c.yml"}],
            "default_server": "https://dev"
        }"#;
        let persisted: PersistSettings = serde_json::from_str(v2).unwrap();
        let settings = AppSettings::from(persisted);
        assert_eq!(settings.active_profile, "Default");
        assert!(settings.profiles.is_empty());
        assert_eq!(settings.server_configs.len(), 1);
        assert_eq!(settings.snapshot_profile().configs.len(), 1);

        let again = PersistSettings::from(&settings);
        assert_eq!(again.settings_version, SETTINGS_SCHEMA_VERSION);
        assert_eq!(again.active_profile.as_deref(), Some("Default"));
    }
}
