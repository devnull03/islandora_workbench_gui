use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::path::{Path, PathBuf};

use std::collections::HashSet;

use super::draft::resolve_secondary_task;
use super::{chain, patch};

// --- States ---
pub struct Pending {
    file_path: PathBuf,
}

pub struct Ready {
    pub file_path: PathBuf,
    pub config: WorkbenchConfigFile,
    /// The file as read, so a run-time update can patch the lines it changes and leave the
    /// rest — comments, quoting, order — exactly as the user wrote it.
    text: String,
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
        let text = std::fs::read_to_string(&self.state.file_path)?;
        let config: WorkbenchConfigFile = serde_yaml::from_str(&text)?;

        Ok(WorkbenchConfigHandler {
            state: Ready {
                file_path: self.state.file_path,
                config,
                text,
            },
        })
    }
}

/// What the app supplies to a config at run time — the three keys in [`super::APP_SUPPLIED`].
#[derive(Debug, Clone, Copy)]
pub struct RunOverrides<'a> {
    /// The server picked in step 3.
    pub host: &'a str,
    /// That server's credentials file, from Settings.
    pub credentials_file_path: &'a Path,
    /// The CSV step 1 produced in this session, if it ran. Written into the **root** config
    /// only: a secondary config runs against its own `input_csv` (typically a different sheet,
    /// such as pages or media), so overwriting it with the root's would be wrong.
    pub input_csv: Option<&'a Path>,
    /// Refuse when neither the processed CSV nor the file supplies `input_csv` (plan Q2). True
    /// for Check and Run; false for modes that never read the rows (a CSV template, quick
    /// delete, print config), where Workbench's own default is harmless.
    pub require_input_csv: bool,
}

impl WorkbenchConfigHandler<Ready> {
    /// Write the run-time overrides into this config and, recursively, host and credentials
    /// into every secondary config, preserving all other fields.
    ///
    /// `input_csv` follows the rule settled for plan question Q2: the app's processed CSV wins
    /// when this session has one; otherwise the file's own `input_csv` stands; when neither
    /// exists this refuses *before writing anything*, rather than letting Workbench fall back
    /// to a default CSV the user never chose.
    ///
    /// Relative secondary task paths are resolved against `workbench_dir` (the workbench
    /// install directory), which is the same base workbench itself uses when it resolves them
    /// at runtime.
    pub fn update_config_fields(
        &mut self,
        overrides: &RunOverrides<'_>,
        workbench_dir: &Path,
    ) -> anyhow::Result<()> {
        if overrides.require_input_csv
            && overrides.input_csv.is_none()
            && self.state.config.input_csv.is_none()
        {
            anyhow::bail!(
                "No input CSV: process a source in step 1, or set input_csv in {}.",
                self.state.file_path.display()
            );
        }
        self.apply(
            overrides,
            overrides.input_csv,
            workbench_dir,
            &mut HashSet::new(),
        )
    }

    /// Write host, credentials and (for the root only) `input_csv` into this file, then host
    /// and credentials into every secondary config — the part a whole chain shares.
    fn apply(
        &mut self,
        overrides: &RunOverrides<'_>,
        input_csv: Option<&Path>,
        workbench_dir: &Path,
        visited: &mut HashSet<PathBuf>,
    ) -> anyhow::Result<()> {
        // A hand-written chain can loop (the builder refuses to make one). Workbench would run
        // it forever; the rewrite must at least terminate.
        if !visited.insert(chain::identity(&self.state.file_path)) {
            anyhow::bail!(
                "{} runs itself through secondary_tasks; remove the loop before running.",
                self.state.file_path.display()
            );
        }
        log::debug!(
            "[config] updating {:?}  host={:?}",
            self.state.file_path,
            overrides.host
        );

        self.state.config.host = Some(overrides.host.to_string());
        self.state.config.credentials_file_path =
            Some(overrides.credentials_file_path.to_path_buf());
        let mut entries = vec![
            ("host", overrides.host.to_string()),
            (
                "credentials_file_path",
                overrides
                    .credentials_file_path
                    .to_string_lossy()
                    .into_owned(),
            ),
        ];
        if let Some(csv) = input_csv {
            self.state.config.input_csv = Some(csv.to_path_buf());
            entries.push(("input_csv", csv.to_string_lossy().into_owned()));
        }
        self.write_entries(&entries)?;

        if let Some(secondary_tasks) = self.state.config.secondary_tasks.clone() {
            for task_path in secondary_tasks {
                let resolved = resolve_secondary_task(
                    &task_path,
                    Some(&self.state.file_path),
                    Some(workbench_dir),
                );
                log::debug!(
                    "[config] secondary task raw={:?}  resolved={:?}",
                    task_path,
                    resolved
                );

                let mut handler = WorkbenchConfigHandler::new(resolved.clone())
                    .load()
                    .map_err(|e| anyhow::anyhow!("secondary config {}: {e}", resolved.display()))?;
                handler.apply(overrides, None, workbench_dir, visited)?;
            }
        }

        Ok(())
    }

    /// Write `entries` into the file by patching only their lines (plan 1.3, Q3). The patch is
    /// re-parsed and must equal "the original document with exactly these keys set"; anything
    /// else — a document shape the patcher declines, or one it misread — falls back to
    /// re-serializing, which is correct but drops comments. Nothing is written when the file
    /// already says this, so a repeat Check does not touch its modification time.
    fn write_entries(&mut self, entries: &[(&str, String)]) -> anyhow::Result<()> {
        let pairs: Vec<(&str, &str)> = entries.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let patched = patch::set_top_level_scalars(&self.state.text, &pairs)
            .filter(|patched| patch_is_exact(&self.state.text, patched, &pairs));
        let text = match patched {
            Some(text) => text,
            None => {
                log::warn!(
                    "[config] {:?} could not be patched in place; rewriting it (comments are lost)",
                    self.state.file_path
                );
                serde_yaml::to_string(&self.state.config)?
            }
        };
        if text != self.state.text {
            std::fs::write(&self.state.file_path, &text)?;
            self.state.text = text;
        }
        Ok(())
    }

    /// Any other top-level setting, as loaded (`task`, `log_file_path`, `output_csv`, …).
    pub fn setting(&self, key: &str) -> Option<&Value> {
        self.state.config.other.get(key)
    }

    /// A text setting, trimmed; `None` when absent, empty or not text.
    pub fn text_setting(&self, key: &str) -> Option<&str> {
        self.setting(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }

    /// The CSV this config will read, if it names one.
    pub fn input_csv(&self) -> Option<&Path> {
        self.state.config.input_csv.as_deref()
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

/// Does `patched` parse to `original` with exactly `entries` set as strings, and nothing else
/// changed? The patcher works on text; this is the check that it understood the document.
fn patch_is_exact(original: &str, patched: &str, entries: &[(&str, &str)]) -> bool {
    let (Ok(mut expected), Ok(actual)) = (
        serde_yaml::from_str::<IndexMap<String, Value>>(original),
        serde_yaml::from_str::<IndexMap<String, Value>>(patched),
    ) else {
        return false;
    };
    for (key, value) in entries {
        expected.insert((*key).to_string(), Value::String((*value).to_string()));
    }
    expected == actual
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
                &RunOverrides {
                    host: "https://islandora.dev",
                    credentials_file_path: Path::new("/creds.yml"),
                    input_csv: Some(Path::new("/wb/input_data/metadata.csv")),
                    require_input_csv: true,
                },
                &tree.root,
            )
            .unwrap();

        let written = std::fs::read_to_string(&path).unwrap();
        let reloaded: IndexMap<String, Value> = serde_yaml::from_str(&written).unwrap();
        assert_eq!(reloaded["host"].as_str(), Some("https://islandora.dev"));
        assert_eq!(
            reloaded["credentials_file_path"].as_str(),
            Some("/creds.yml")
        );
        assert_eq!(reloaded["task"].as_str(), Some("create"));
        // Nothing the app did not supply appears out of nowhere.
        assert!(!reloaded.contains_key("secondary_tasks"));
    }

    /// Q3: the run-time update must not cost the user their comments, quoting or key order.
    #[test]
    fn run_time_update_keeps_the_rest_of_the_file_byte_identical() {
        let tree = TempTree::new();
        let original = "\
# Reviewed 2026-10-01 — do not reorder
task: create
host: 'https://old.example.org'  # replaced at run time
id_field: \"id\"

media_use_tid: http://pcdm.org/use#OriginalFile   # keep
";
        let path = tree.write("commented.yml", original);
        WorkbenchConfigHandler::new(path.clone())
            .load()
            .unwrap()
            .update_config_fields(
                &RunOverrides {
                    host: "https://new.example.org",
                    credentials_file_path: Path::new("/creds.yml"),
                    input_csv: Some(Path::new("/wb/input_data/metadata.csv")),
                    require_input_csv: true,
                },
                &tree.root,
            )
            .unwrap();

        let expected = original.replace(
            "host: 'https://old.example.org'  # replaced at run time\n",
            "host: https://new.example.org\n",
        ) + "credentials_file_path: /creds.yml\ninput_csv: /wb/input_data/metadata.csv\n";
        assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
    }

    /// A document the patcher declines is still updated correctly, by re-serializing.
    #[test]
    fn flow_mapping_falls_back_to_rewriting() {
        let tree = TempTree::new();
        let path = tree.write("flow.yml", "{task: create, input_csv: a.csv}\n");
        WorkbenchConfigHandler::new(path.clone())
            .load()
            .unwrap()
            .update_config_fields(
                &RunOverrides {
                    host: "https://h.org",
                    credentials_file_path: Path::new("/c.yml"),
                    input_csv: None,
                    require_input_csv: true,
                },
                &tree.root,
            )
            .unwrap();
        let reloaded = WorkbenchConfigHandler::new(path).load().unwrap();
        assert_eq!(reloaded.host(), Some("https://h.org"));
        assert_eq!(reloaded.input_csv(), Some(Path::new("a.csv")));
    }

    /// P0-4: a chain linked in the builder — configs in a library folder, Workbench elsewhere —
    /// must be found and patched at run time.
    #[test]
    fn a_chain_linked_by_the_builder_is_found_at_run_time() {
        let tree = TempTree::new();
        let workbench = tree.root.join("workbench");
        std::fs::create_dir_all(&workbench).unwrap();
        let child = tree.write("library/pages.yml", "task: add_media\n");
        let root_path = tree.write("library/root.yml", "task: create\n");

        let mut root = crate::config::ConfigDraft::load(&root_path).unwrap();
        assert!(root.link_secondary_task(&child, Some(&workbench)));
        root.save(&root_path).unwrap();

        WorkbenchConfigHandler::new(root_path)
            .load()
            .unwrap()
            .update_config_fields(
                &RunOverrides {
                    host: "https://islandora.dev",
                    credentials_file_path: Path::new("/creds.yml"),
                    input_csv: Some(Path::new("/wb/metadata.csv")),
                    require_input_csv: true,
                },
                &workbench,
            )
            .unwrap();
        let child = WorkbenchConfigHandler::new(child).load().unwrap();
        assert_eq!(child.host(), Some("https://islandora.dev"));
    }

    #[test]
    fn a_looping_chain_is_an_error_not_a_stack_overflow() {
        let tree = TempTree::new();
        let a = tree.root.join("a.yml");
        let b = tree.write(
            "b.yml",
            &format!("task: create\nsecondary_tasks: [{}]\n", a.display()),
        );
        tree.write(
            "a.yml",
            &format!("task: create\nsecondary_tasks: [{}]\n", b.display()),
        );
        let error = WorkbenchConfigHandler::new(a)
            .load()
            .unwrap()
            .update_config_fields(
                &RunOverrides {
                    host: "https://h.org",
                    credentials_file_path: Path::new("/c.yml"),
                    input_csv: Some(Path::new("/m.csv")),
                    require_input_csv: true,
                },
                &tree.root,
            )
            .unwrap_err();
        assert!(error.to_string().contains("runs itself"));
    }

    /// Q2: the processed CSV goes into the root config only — a child keeps its own sheet.
    #[test]
    fn processed_csv_goes_to_the_root_and_not_to_children() {
        let tree = TempTree::new();
        let workbench = tree.root.join("workbench");
        std::fs::create_dir_all(&workbench).unwrap();
        let child = tree.write("configs/pages.yml", "task: create\ninput_csv: pages.csv\n");
        let root = tree.write(
            "configs/root.yml",
            &format!("task: create\nsecondary_tasks: [{}]\n", child.display()),
        );
        let processed = workbench.join("input_data/metadata.csv");

        WorkbenchConfigHandler::new(root.clone())
            .load()
            .unwrap()
            .update_config_fields(
                &RunOverrides {
                    host: "https://islandora.dev",
                    credentials_file_path: Path::new("/creds.yml"),
                    input_csv: Some(&processed),
                    require_input_csv: true,
                },
                &workbench,
            )
            .unwrap();

        let root = WorkbenchConfigHandler::new(root).load().unwrap();
        assert_eq!(root.input_csv(), Some(processed.as_path()));
        let child = WorkbenchConfigHandler::new(child).load().unwrap();
        assert_eq!(child.input_csv(), Some(Path::new("pages.csv")));
        assert_eq!(child.host(), Some("https://islandora.dev"));
    }

    /// With no processed CSV the file's own `input_csv` stands; with neither, nothing is written
    /// and the error says what to do.
    #[test]
    fn input_csv_falls_back_to_the_file_and_refuses_when_neither_has_one() {
        let tree = TempTree::new();
        let overrides = RunOverrides {
            host: "https://islandora.dev",
            credentials_file_path: Path::new("/creds.yml"),
            input_csv: None,
            require_input_csv: true,
        };

        let own = tree.write("own.yml", "task: create\ninput_csv: mine.csv\n");
        let mut handler = WorkbenchConfigHandler::new(own.clone()).load().unwrap();
        handler
            .update_config_fields(&overrides, &tree.root)
            .unwrap();
        let reloaded = WorkbenchConfigHandler::new(own).load().unwrap();
        assert_eq!(reloaded.input_csv(), Some(Path::new("mine.csv")));

        let bare = tree.write("bare.yml", "task: create\n");
        let mut handler = WorkbenchConfigHandler::new(bare.clone()).load().unwrap();
        let error = handler
            .update_config_fields(&overrides, &tree.root)
            .unwrap_err();
        assert!(error.to_string().contains("No input CSV"));
        assert_eq!(std::fs::read_to_string(&bare).unwrap(), "task: create\n");

        // A mode that never reads the rows does not need one.
        let mut handler = WorkbenchConfigHandler::new(bare.clone()).load().unwrap();
        handler
            .update_config_fields(
                &RunOverrides {
                    require_input_csv: false,
                    ..overrides
                },
                &tree.root,
            )
            .unwrap();
        let reloaded = WorkbenchConfigHandler::new(bare).load().unwrap();
        assert_eq!(reloaded.host(), Some("https://islandora.dev"));
        assert_eq!(reloaded.input_csv(), None);
    }
}
