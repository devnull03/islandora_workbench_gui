use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::marker::PhantomData;

// --- States ---
pub struct Pending;
pub struct Ready;

#[derive(Debug, Serialize, Deserialize)]
struct WorkbenchConfigFile {
    host: String,
    credentials_file_path: String,
    input_csv: String,

    #[serde(flatten)]
    other: IndexMap<String, Value>,
}

/// A handler for the workbench configuration file using the Type State pattern.
///
/// The handler starts in the `Pending` state and must be `load()`ed to transition
/// to the `Ready` state, at which point the configuration can be updated.
pub struct WorkbenchConfigHandler<S> {
    state: PhantomData<S>,
    file_path: String,
    config: Option<WorkbenchConfigFile>,
}

impl WorkbenchConfigHandler<Pending> {
    /// Creates a new handler for a config file path.
    pub fn new(config_file_path: &str) -> Self {
        Self {
            state: PhantomData,
            file_path: config_file_path.to_string(),
            config: None,
        }
    }

    /// Loads the configuration from disk and transitions to the `Ready` state.
    pub fn load(self) -> anyhow::Result<WorkbenchConfigHandler<Ready>> {
        let file = std::fs::File::open(&self.file_path)?;
        let config: WorkbenchConfigFile = serde_yaml::from_reader(file)?;

        Ok(WorkbenchConfigHandler {
            state: PhantomData,
            file_path: self.file_path,
            config: Some(config),
        })
    }
}

impl WorkbenchConfigHandler<Ready> {
    /// Updates the host and credentials in the config file, preserving all other fields.
    pub fn update_config_fields(
        &mut self,
        host: &str,
        credentials_file_path: &str,
    ) -> anyhow::Result<()> {
        // Safe to unwrap because the transition to 'Ready' guarantees the config exists.
        let config = self
            .config
            .as_mut()
            .expect("Ready state must have a config");

        config.host = host.to_string();
        config.credentials_file_path = credentials_file_path.to_string();

        let yaml = serde_yaml::to_string(&config)?;
        std::fs::write(&self.file_path, yaml)?;

        Ok(())
    }

    /// Returns the currently loaded host.
    pub fn host(&self) -> &str {
        &self.config.as_ref().unwrap().host
    }

    /// Returns the currently loaded credentials file path.
    pub fn credentials_file_path(&self) -> &str {
        &self.config.as_ref().unwrap().credentials_file_path
    }
}

impl<S> WorkbenchConfigHandler<S> {
    pub fn update_file_path(&mut self, file_path: &str) -> anyhow::Result<()> {
        self.file_path = file_path.to_string();
        Ok(())
    }
}
