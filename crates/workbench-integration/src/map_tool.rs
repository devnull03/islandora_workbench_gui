//! The CSV ID → node ID map's maintenance tool, `scripts/manage_csv_to_node_id_map.py`, which
//! ships with Workbench (docs: "The CSV ID to node ID map"). The app runs it in Workbench's own
//! Python, against the database the selected config uses.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::WbInfo;

/// What to do to the map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapAction {
    /// Dump the whole map to a CSV.
    ExportCsv(PathBuf),
    /// Delete entries older than this date (`yyyy-mm-dd`, optionally with `hh:mm`).
    RemoveBefore(String),
    /// Delete entries newer than this date.
    RemoveAfter(String),
    /// Delete entries made by these config files (as they were passed to Workbench).
    RemoveForConfigs(String),
    /// Delete entries whose node no longer exists on this host.
    RemoveDeletedNodes(String),
}

impl MapAction {
    pub fn label(&self) -> &'static str {
        match self {
            MapAction::ExportCsv(_) => "Export the CSV ID map",
            MapAction::RemoveBefore(_) => "Remove map entries before a date",
            MapAction::RemoveAfter(_) => "Remove map entries after a date",
            MapAction::RemoveForConfigs(_) => "Remove map entries for configs",
            MapAction::RemoveDeletedNodes(_) => "Remove map entries for deleted nodes",
        }
    }

    fn args(&self) -> Vec<String> {
        let (flag, value) = match self {
            MapAction::ExportCsv(path) => ("--csv_path", path.display().to_string()),
            MapAction::RemoveBefore(date) => ("--remove_entries_before", date.clone()),
            MapAction::RemoveAfter(date) => ("--remove_entries_after", date.clone()),
            MapAction::RemoveForConfigs(list) => {
                ("--remove_entries_with_config_files", list.clone())
            }
            MapAction::RemoveDeletedNodes(host) => {
                ("--remove_entries_for_deleted_nodes", host.clone())
            }
        };
        vec![flag.to_string(), value]
    }
}

/// Where the map database is, as Workbench works it out (`WorkbenchConfig`): the config's
/// `csv_id_to_node_id_map_path`, else `csv_id_to_node_id_map_dir` (the system temp folder by
/// default) joined with `csv_id_to_node_id_map_filename`. Relative paths are relative to the
/// Workbench folder, which Workbench runs from.
pub fn map_db_path(config_text: impl Fn(&str) -> Option<String>, workbench_dir: &Path) -> PathBuf {
    let path = config_text("csv_id_to_node_id_map_path")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let dir = config_text("csv_id_to_node_id_map_dir")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir);
            let file = config_text("csv_id_to_node_id_map_filename")
                .unwrap_or_else(|| "csv_id_to_node_id_map.db".into());
            dir.join(file)
        });
    if path.is_absolute() {
        path
    } else {
        workbench_dir.join(path)
    }
}

/// The command that runs `action` on the map at `db`.
pub fn map_tool_command(wb: &WbInfo, db: &Path, action: &MapAction) -> Command {
    let mut cmd = wb.python_command();
    cmd.arg(Path::new("scripts").join("manage_csv_to_node_id_map.py"))
        .env("PYTHONUNBUFFERED", "1")
        .arg("--db_path")
        .arg(db)
        .args(action.args());
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_map_is_where_workbench_looks() {
        let wb = Path::new("/wb");
        let none = |_: &str| None;
        assert_eq!(
            map_db_path(none, wb),
            std::env::temp_dir().join("csv_id_to_node_id_map.db")
        );
        let dir = |k: &str| (k == "csv_id_to_node_id_map_dir").then(|| "maps".to_string());
        assert_eq!(
            map_db_path(dir, wb),
            Path::new("/wb/maps/csv_id_to_node_id_map.db")
        );
        let full = |k: &str| (k == "csv_id_to_node_id_map_path").then(|| "/data/m.db".to_string());
        assert_eq!(map_db_path(full, wb), Path::new("/data/m.db"));
    }

    #[test]
    fn actions_become_the_script_flags() {
        assert_eq!(
            MapAction::RemoveBefore("2025-01-01".into()).args(),
            vec!["--remove_entries_before", "2025-01-01"]
        );
        assert_eq!(
            MapAction::ExportCsv("/tmp/map.csv".into()).args(),
            vec!["--csv_path", "/tmp/map.csv"]
        );
    }
}
