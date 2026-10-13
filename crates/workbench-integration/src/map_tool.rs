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
    /// Dump only the rows whose value in this column (`csv_id`, say) appears more than once.
    ExportDuplicates { csv: PathBuf, column: String },
    /// Copy another map database's entries into this one.
    MergeFrom(PathBuf),
}

impl MapAction {
    pub fn label(&self) -> &'static str {
        match self {
            MapAction::ExportCsv(_) => "Export the CSV ID map",
            MapAction::RemoveBefore(_) => "Remove map entries before a date",
            MapAction::RemoveAfter(_) => "Remove map entries after a date",
            MapAction::RemoveForConfigs(_) => "Remove map entries for configs",
            MapAction::RemoveDeletedNodes(_) => "Remove map entries for deleted nodes",
            MapAction::ExportDuplicates { .. } => "Export duplicate IDs from the CSV ID map",
            MapAction::MergeFrom(_) => "Merge another CSV ID map into this one",
        }
    }

    /// The script's flags for this action, given the map at `db`. Merging names the map as the
    /// target and takes no `--db_path`.
    fn args(&self, db: &Path) -> Vec<String> {
        let db = db.display().to_string();
        let with_db =
            |flag: &str, value: String| vec!["--db_path".into(), db.clone(), flag.into(), value];
        match self {
            MapAction::ExportCsv(path) => with_db("--csv_path", path.display().to_string()),
            MapAction::RemoveBefore(date) => with_db("--remove_entries_before", date.clone()),
            MapAction::RemoveAfter(date) => with_db("--remove_entries_after", date.clone()),
            MapAction::RemoveForConfigs(list) => {
                with_db("--remove_entries_with_config_files", list.clone())
            }
            MapAction::RemoveDeletedNodes(host) => {
                with_db("--remove_entries_for_deleted_nodes", host.clone())
            }
            MapAction::ExportDuplicates { csv, column } => {
                let mut args = with_db("--csv_path", csv.display().to_string());
                args.extend(["--nonunique".into(), column.clone()]);
                args
            }
            MapAction::MergeFrom(source) => vec![
                "--target_db_path".into(),
                db,
                "--source_db_path".into(),
                source.display().to_string(),
            ],
        }
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
    crate::secrets::resolve(&path.to_string_lossy(), workbench_dir)
}

/// The command that runs `action` on the map at `db`.
pub fn map_tool_command(wb: &WbInfo, db: &Path, action: &MapAction) -> Command {
    let mut cmd = wb.python_command();
    cmd.arg(Path::new("scripts").join("manage_csv_to_node_id_map.py"))
        .env("PYTHONUNBUFFERED", "1")
        .args(action.args(db));
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
        let db = Path::new("/m.db");
        assert_eq!(
            MapAction::RemoveBefore("2025-01-01".into()).args(db),
            vec![
                "--db_path",
                "/m.db",
                "--remove_entries_before",
                "2025-01-01"
            ]
        );
        assert_eq!(
            MapAction::ExportDuplicates {
                csv: "/tmp/d.csv".into(),
                column: "csv_id".into()
            }
            .args(db),
            vec![
                "--db_path",
                "/m.db",
                "--csv_path",
                "/tmp/d.csv",
                "--nonunique",
                "csv_id"
            ]
        );
        // Merging names the map as the target, with no --db_path.
        assert_eq!(
            MapAction::MergeFrom("/other.db".into()).args(db),
            vec!["--target_db_path", "/m.db", "--source_db_path", "/other.db"]
        );
    }
}
