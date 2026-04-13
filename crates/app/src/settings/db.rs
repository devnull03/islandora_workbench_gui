//! Persistent storage for `AppSettings`.
//!
//! Implementation notes:
//! - Stores one JSON blob in SQLite (single-row KV table).
//! - Writes are debounced in a background thread; mutations enqueue snapshots.

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

use super::{AppSettings, ServerConfig, TaskConfig, Val};

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
    file_path: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct PersistServerConfig {
    label: String,
    server_url: String,
    credentials_file: String,
}

#[derive(Clone, Serialize, Deserialize, Default)]
struct PersistSettings {
    values: HashMap<String, PersistVal>,
    task_configs: Vec<PersistTaskConfig>,
    server_configs: Vec<PersistServerConfig>,
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
            })
            .collect();

        let server_configs = s
            .server_configs
            .iter()
            .map(|srv| PersistServerConfig {
                label: srv.label.to_string(),
                server_url: srv.server_url.to_string(),
                credentials_file: srv.credentials_file.to_string(),
            })
            .collect();

        Self { values, task_configs, server_configs }
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
            })
            .collect();

        let server_configs = p
            .server_configs
            .into_iter()
            .map(|s| ServerConfig {
                label: SharedString::from(s.label),
                server_url: SharedString::from(s.server_url),
                credentials_file: SharedString::from(s.credentials_file),
            })
            .collect();

        Self { values, task_configs, server_configs }
    }
}

fn data_dir() -> Result<PathBuf> {
    let base = dirs::data_local_dir().context("Failed to resolve local data dir")?;
    Ok(base.join(APP_DIR))
}

fn db_path() -> Result<PathBuf> {
    Ok(data_dir()?.join(DB_FILE))
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
    Ok(persist.into())
}

fn save_app_settings_snapshot(snapshot: PersistSettings) -> Result<()> {
    let path = db_path()?;
    let conn = open_db(&path)?;
    let json = serde_json::to_string(&snapshot).context("Serialize settings")?;
    conn.execute(
        "INSERT INTO settings_kv(key, json) VALUES (?1, ?2)\n         ON CONFLICT(key) DO UPDATE SET json = excluded.json",
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
