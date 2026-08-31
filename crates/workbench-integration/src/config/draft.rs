//! Editable Workbench configuration documents and their YAML persistence.

use indexmap::IndexMap;
use serde_yaml::Value;
use std::path::{Path, PathBuf};

/// A config being edited in the builder.
///
/// Unlike [`super::WorkbenchConfigHandler`], this keeps an open, stable-order map of settings.
/// App-supplied settings may be present when an existing file is loaded, but are excluded from
/// builder controls.
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

    /// [`Self::secondary_tasks`] with relative paths resolved against this config's own folder,
    /// so the builder can tell a moved file from a merely relative one.
    ///
    /// Workbench itself resolves them against the *workbench* directory (see
    /// [`super::WorkbenchConfigHandler::update_config_fields`]), which the builder does not know.
    /// In practice a config and its children sit together, and this is only used to decide what
    /// to show — the value written to YAML is always the path the user gave.
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
