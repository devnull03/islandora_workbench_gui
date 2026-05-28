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

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkbenchConfigFile {
    host: String,
    credentials_file_path: PathBuf,
    input_csv: PathBuf,
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
        eprintln!(
            "[config] updating {:?}  host={:?}",
            self.state.file_path, host
        );

        self.state.config.host = host.to_string();
        self.state.config.credentials_file_path = credentials_file_path.clone();

        let yaml = serde_yaml::to_string(&self.state.config)?;
        std::fs::write(&self.state.file_path, yaml)?;

        if let Some(secondary_tasks) = self.state.config.secondary_tasks.clone() {
            for task_path in secondary_tasks {
                let resolved = if task_path.is_absolute() {
                    task_path.clone()
                } else {
                    workbench_dir.join(&task_path)
                };
                eprintln!(
                    "[config] secondary task raw={:?}  resolved={:?}",
                    task_path, resolved
                );

                let mut handler = WorkbenchConfigHandler::new(resolved).load()?;
                handler.update_config_fields(host, credentials_file_path.clone(), workbench_dir)?;
            }
        }

        Ok(())
    }

    /// Returns the currently loaded host.
    pub fn host(&self) -> &str {
        &self.state.config.host
    }

    /// Returns the currently loaded credentials file path.
    pub fn credentials_file_path(&self) -> &std::path::Path {
        &self.state.config.credentials_file_path
    }

    pub fn path(&self) -> &std::path::Path {
        &self.state.file_path
    }
}
