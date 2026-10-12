use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::path::{Path, PathBuf};

// --- States ---
pub struct Pending {
    file_path: PathBuf,
}

pub struct Ready {
    pub file_path: PathBuf,
    pub config: WorkbenchConfigFile,
}

/// The parts of a Workbench config the app reads or writes at run time; everything else rides
/// along in `other`.
///
/// `host`, `credentials_file_path` and `input_csv` are optional because the builder never writes
/// them (they are app-supplied, see [`super::APP_SUPPLIED`]): a config authored there has none of
/// the three until [`WorkbenchConfigHandler::update_config_fields`] supplies them. Absent stays
/// absent on the way out — an empty `host: ''` would be worse than no key at all.
#[derive(Debug, Serialize, Deserialize)]
pub struct WorkbenchConfigFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    credentials_file_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    input_csv: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    secondary_tasks: Option<Vec<PathBuf>>,

    #[serde(flatten)]
    other: IndexMap<String, Value>,
}

/// A handler for the workbench configuration file using the Type State pattern.
pub struct WorkbenchConfigHandler<S> {
    pub state: S,
}

impl WorkbenchConfigHandler<Pending> {
    /// Creates a new handler for a config file path.
    pub fn new(config_file_path: PathBuf) -> Self {
        Self {
            state: Pending {
                file_path: config_file_path,
            },
        }
    }

    /// Loads the configuration from disk and transitions to the `Ready` state.
    pub fn load(self) -> anyhow::Result<WorkbenchConfigHandler<Ready>> {
        let file = std::fs::File::open(&self.state.file_path)?;
        let config: WorkbenchConfigFile = serde_yaml::from_reader(file)?;

        Ok(WorkbenchConfigHandler {
            state: Ready {
                file_path: self.state.file_path,
                config,
            },
        })
    }
}

impl WorkbenchConfigHandler<Ready> {
    /// Updates the host and credentials in the config file, preserving all other fields.
    /// Recursively applies the same update to any secondary tasks. Relative secondary task
    /// paths are resolved against `workbench_dir` (the workbench install directory), which
    /// is the same base workbench itself uses when it resolves them at runtime.
    pub fn update_config_fields(
        &mut self,
        host: &str,
        credentials_file_path: PathBuf,
        workbench_dir: &Path,
    ) -> anyhow::Result<()> {
        log::debug!(
            "[config] updating {:?}  host={:?}",
            self.state.file_path,
            host
        );

        self.state.config.host = Some(host.to_string());
        self.state.config.credentials_file_path = Some(credentials_file_path.clone());

        let yaml = serde_yaml::to_string(&self.state.config)?;
        std::fs::write(&self.state.file_path, yaml)?;

        if let Some(secondary_tasks) = self.state.config.secondary_tasks.clone() {
            for task_path in secondary_tasks {
                let resolved = if task_path.is_absolute() {
                    task_path.clone()
                } else {
                    workbench_dir.join(&task_path)
                };
                log::debug!(
                    "[config] secondary task raw={:?}  resolved={:?}",
                    task_path,
                    resolved
                );

                let mut handler = WorkbenchConfigHandler::new(resolved).load()?;
                handler.update_config_fields(host, credentials_file_path.clone(), workbench_dir)?;
            }
        }

        Ok(())
    }

    /// Returns the currently loaded host, if the file has one.
    pub fn host(&self) -> Option<&str> {
        self.state.config.host.as_deref()
    }

    /// Returns the currently loaded credentials file path, if the file has one.
    pub fn credentials_file_path(&self) -> Option<&Path> {
        self.state.config.credentials_file_path.as_deref()
    }

    pub fn path(&self) -> &Path {
        &self.state.file_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::test_support::TempTree;

    /// A config the builder wrote has none of the app-supplied keys. It has to load, and the
    /// run-time rewrite has to add them — `missing field \`host\`` was the bug.
    #[test]
    fn builder_config_without_app_supplied_keys_loads_and_gains_them() {
        let tree = TempTree::new();
        let path = tree.write("builder.yml", "task: create\nrollback_dir: ./rollback\n");

        let mut handler = WorkbenchConfigHandler::new(path.clone()).load().unwrap();
        assert_eq!(handler.host(), None);
        handler
            .update_config_fields(
                "https://islandora.dev",
                PathBuf::from("/creds.yml"),
                &tree.root,
            )
            .unwrap();

        let written = std::fs::read_to_string(&path).unwrap();
        let reloaded: IndexMap<String, Value> = serde_yaml::from_str(&written).unwrap();
        assert_eq!(reloaded["host"].as_str(), Some("https://islandora.dev"));
        assert_eq!(reloaded["credentials_file_path"].as_str(), Some("/creds.yml"));
        assert_eq!(reloaded["task"].as_str(), Some("create"));
        // Nothing the app did not supply appears out of nowhere.
        assert!(!reloaded.contains_key("input_csv"));
        assert!(!reloaded.contains_key("secondary_tasks"));
    }
}
