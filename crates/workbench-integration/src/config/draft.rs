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
    /// The file this was loaded from had comments. [`Self::save`] re-serializes, which drops
    /// them, so the builder asks before the first save of such a file (plan Q8, minimum
    /// option: warn rather than round-trip comments).
    pub source_has_comments: bool,
}

/// Written by the app at run time, so the builder never edits them (mockup `1a`'s locked band).
pub const APP_SUPPLIED: [&str; 3] = ["host", "credentials_file_path", "input_csv"];

impl ConfigDraft {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let mut draft = Self::from_yaml(&text)?;
        draft.source_has_comments = super::has_comments(&text);
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
            source_has_comments: false,
        })
    }

    /// The YAML this draft writes.
    ///
    /// A `null` value means **unset** — a row the user added but left empty (an integer field
    /// cleared back to "use the default", a nullable enum set to `none`). Unset keys are left
    /// out, so Workbench applies its own default rather than reading an explicit null or a 0
    /// (§04; plan 1.5, P0-5). The row stays in the builder; only the file omits it.
    pub fn to_yaml(&self) -> String {
        let set: IndexMap<&String, &Value> =
            self.values.iter().filter(|(_, v)| !v.is_null()).collect();
        // An empty draft serialises as `{}`, which is valid YAML but reads as noise in the
        // preview panel before anything has been added.
        if set.is_empty() {
            return String::new();
        }
        serde_yaml::to_string(&set).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_yaml())?;
        Ok(())
    }

    /// Record that the file on disk is now this draft's own output, so there are no comments
    /// left to lose.
    pub fn mark_saved(&mut self, path: &Path) {
        self.path = Some(path.to_path_buf());
        self.source_has_comments = false;
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

    /// [`Self::secondary_tasks`] resolved the way Workbench will resolve them — see
    /// [`resolve_secondary_task`].
    pub fn resolved_secondary_tasks(&self, workbench_dir: Option<&Path>) -> Vec<PathBuf> {
        self.secondary_tasks()
            .into_iter()
            .map(|p| resolve_secondary_task(&p, self.path.as_deref(), workbench_dir))
            .collect()
    }

    /// Run `child` after this config, unless it already does. Always stored **absolute**: a
    /// relative entry means "relative to wherever Workbench runs", which is not where the
    /// config lives, so a link the builder writes must not depend on it (plan 1.4, P0-4).
    /// Returns whether anything changed.
    pub fn link_secondary_task(&mut self, child: &Path, workbench_dir: Option<&Path>) -> bool {
        let child = std::path::absolute(child).unwrap_or_else(|_| child.to_path_buf());
        if self
            .resolved_secondary_tasks(workbench_dir)
            .iter()
            .any(|existing| super::chain::same_path(existing, &child))
        {
            return false;
        }
        let mut tasks = self.secondary_tasks();
        tasks.push(child);
        self.set_secondary_tasks(&tasks);
        true
    }
}

/// Where a `secondary_tasks` entry points.
///
/// Workbench turns each entry into `os.path.abspath(entry)` in the process that runs the
/// primary config, and the app always starts that process in the Workbench install directory —
/// so a relative entry, at any depth of a chain, is relative to **the Workbench directory**, not
/// to the config that names it. The run-time rewrite and the builder both resolve through this
/// one function so they cannot disagree again.
///
/// When the Workbench directory is not known yet (Settings has no path), the config's own
/// folder is the best available guess for display; Check/Run cannot happen in that state.
pub fn resolve_secondary_task(
    entry: &Path,
    owner: Option<&Path>,
    workbench_dir: Option<&Path>,
) -> PathBuf {
    if entry.is_absolute() {
        return entry.to_path_buf();
    }
    match workbench_dir.or_else(|| owner.and_then(Path::parent)) {
        Some(base) => base.join(entry),
        None => entry.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::test_support::TempTree;

    /// Q8: the builder needs to know a save will drop comments, and stop warning once it has
    /// written the file itself.
    #[test]
    fn loading_a_commented_file_is_remembered_until_saved() {
        let tree = TempTree::new();
        let commented = tree.write("hand.yml", "# by hand\ntask: create\n");
        let plain = tree.write("plain.yml", "task: create\n");

        let mut draft = ConfigDraft::load(&commented).unwrap();
        assert!(draft.source_has_comments);
        assert!(!ConfigDraft::load(&plain).unwrap().source_has_comments);

        draft.save(&commented).unwrap();
        draft.mark_saved(&commented);
        assert!(!draft.source_has_comments);
        assert!(!ConfigDraft::load(&commented).unwrap().source_has_comments);
    }

    /// P0-5: an unset (null) value keeps its row but is not written.
    #[test]
    fn unset_values_are_left_out_of_the_yaml() {
        let mut draft = ConfigDraft::from_yaml("task: create\n").unwrap();
        draft.values.insert("adaptive_pause".into(), Value::Null);
        assert!(draft.values.contains_key("adaptive_pause"));
        assert_eq!(draft.to_yaml(), "task: create\n");
    }

    /// P0-4: relative entries follow Workbench (its directory), not the config's folder.
    #[test]
    fn relative_entries_resolve_against_the_workbench_directory() {
        let owner = Path::new("/library/root.yml");
        let workbench = Path::new("/opt/workbench");
        assert_eq!(
            resolve_secondary_task(Path::new("pages.yml"), Some(owner), Some(workbench)),
            Path::new("/opt/workbench/pages.yml")
        );
        assert_eq!(
            resolve_secondary_task(Path::new("/x/pages.yml"), Some(owner), Some(workbench)),
            Path::new("/x/pages.yml")
        );
        // No Workbench directory configured: the config's folder is the display fallback.
        assert_eq!(
            resolve_secondary_task(Path::new("pages.yml"), Some(owner), None),
            Path::new("/library/pages.yml")
        );
    }

    #[test]
    fn links_are_absolute_and_not_duplicated() {
        let tree = TempTree::new();
        let child = tree.write("lib/pages.yml", "task: create\n");
        let mut root = ConfigDraft::from_yaml("task: create\n").unwrap();
        root.path = Some(tree.root.join("lib/root.yml"));

        assert!(root.link_secondary_task(&child, Some(&tree.root.join("wb"))));
        assert!(!root.link_secondary_task(&child, Some(&tree.root.join("wb"))));
        assert_eq!(root.secondary_tasks(), vec![child.clone()]);
        assert!(root.secondary_tasks()[0].is_absolute());
    }
}
