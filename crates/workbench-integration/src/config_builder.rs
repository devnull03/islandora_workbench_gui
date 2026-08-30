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

// --- Draft: the config the builder edits ---

/// A config being edited in the builder.
///
/// Deliberately *not* the [`WorkbenchConfigFile`] struct above: that one exists to rewrite
/// `host` / `credentials_file_path` into a file the app is about to run, and it names those
/// fields explicitly. The builder needs the opposite — an open map of whatever the user has
/// added, in a stable order, with the three app-supplied settings absent entirely.
#[derive(Debug, Clone, Default)]
pub struct ConfigDraft {
    /// Where it will be written. `None` until the first save.
    pub path: Option<PathBuf>,
    /// Display name in the config library. Defaults to the file stem.
    pub label: String,
    /// Settings the user has added, in the order they were added.
    pub values: IndexMap<String, Value>,
}

/// Written by the app at run time, so the builder never edits them (mockup `1a`'s locked band).
pub const APP_SUPPLIED: [&str; 3] = ["host", "credentials_file_path", "input_csv"];

impl ConfigDraft {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let mut draft = Self::from_yaml(&text)?;
        draft.label = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        draft.path = Some(path.to_path_buf());
        Ok(draft)
    }

    pub fn from_yaml(text: &str) -> anyhow::Result<Self> {
        let values: IndexMap<String, Value> = serde_yaml::from_str(text)?;
        Ok(Self {
            path: None,
            label: String::new(),
            values,
        })
    }

    pub fn to_yaml(&self) -> String {
        // An empty draft serialises as `{}`, which is valid YAML but reads as noise in the
        // preview panel before anything has been added.
        if self.values.is_empty() {
            return String::new();
        }
        serde_yaml::to_string(&self.values).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_yaml())?;
        Ok(())
    }

    /// `true` when the key is one the app writes at run time, so the builder must not offer it.
    pub fn is_app_supplied(key: &str) -> bool {
        APP_SUPPLIED.contains(&key)
    }

    /// Reads `secondary_tasks` as a list of paths. Tolerates the single-string form Workbench
    /// also accepts.
    pub fn secondary_tasks(&self) -> Vec<PathBuf> {
        match self.values.get("secondary_tasks") {
            Some(Value::Sequence(items)) => items
                .iter()
                .filter_map(|v| v.as_str().map(PathBuf::from))
                .collect(),
            Some(Value::String(one)) => vec![PathBuf::from(one)],
            _ => Vec::new(),
        }
    }

    pub fn set_secondary_tasks(&mut self, paths: &[PathBuf]) {
        if paths.is_empty() {
            self.values.shift_remove("secondary_tasks");
            return;
        }
        let seq = paths
            .iter()
            .map(|p| Value::String(p.to_string_lossy().into_owned()))
            .collect();
        self.values
            .insert("secondary_tasks".into(), Value::Sequence(seq));
    }
}

impl ConfigDraft {
    /// [`Self::secondary_tasks`] with relative paths resolved against this config's own folder,
    /// so the builder can tell a moved file from a merely relative one.
    ///
    /// Workbench itself resolves them against the *workbench* directory (see
    /// [`WorkbenchConfigHandler::update_config_fields`]), which the builder does not know. In
    /// practice a config and its children sit together, and this is only used to decide what to
    /// show — the value written to YAML is always the path the user gave.
    pub fn resolved_secondary_tasks(&self) -> Vec<PathBuf> {
        let base = self.path.as_deref().and_then(Path::parent);
        self.secondary_tasks()
            .into_iter()
            .map(|p| match (&base, p.is_absolute()) {
                (Some(base), false) => base.join(p),
                _ => p,
            })
            .collect()
    }
}
