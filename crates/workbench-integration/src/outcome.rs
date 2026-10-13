//! What a Workbench run produced, read from its output and its log.
//!
//! Workbench reports results as prose, so the app reads the few lines it acts on: which nodes a
//! run created (for recovery mode), where a contact sheet or CSV template was written (to open
//! it), and where the rollback config went (to offer "Roll back"). The patterns are taken from
//! upstream `workbench` / `workbench_utils.py`; each has a test with the line Workbench prints.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Facts gathered while a run streams.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunReport {
    /// The first node this run created — what recovery mode needs to resume it.
    pub first_node_id: Option<u64>,
    /// How many nodes it reported creating.
    pub nodes_created: usize,
    /// `Contact sheet is at <path>.`
    pub contact_sheet: Option<PathBuf>,
    /// `CSV template saved at <path>.`
    pub csv_template: Option<PathBuf>,
}

impl RunReport {
    /// Fold one line of Workbench's stdout into the report.
    pub fn observe(&mut self, line: &str) {
        if let Some(node) = created_node_id(line) {
            self.nodes_created += 1;
            self.first_node_id.get_or_insert(node);
        }
        if let Some(path) = sentence_path(line, "Contact sheet is at ") {
            self.contact_sheet = Some(path);
        }
        if let Some(path) = sentence_path(line, "CSV template saved at ") {
            self.csv_template = Some(path);
        }
    }
}

/// `Node for "Title" (record 01) created at http://host/node/33.` → `33`. Also matches the
/// `create_from_files` and paged-child forms, which share "created at <node uri>".
fn created_node_id(line: &str) -> Option<u64> {
    let rest = &line[line.find(" created at ")? + " created at ".len()..];
    let after = &rest[rest.rfind("/node/")? + "/node/".len()..];
    let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// The path in `<prefix><path>.` — the trailing full stop is Workbench's, not the path's.
fn sentence_path(line: &str, prefix: &str) -> Option<PathBuf> {
    let rest = line.trim().strip_prefix(prefix)?;
    let path = rest.strip_suffix('.').unwrap_or(rest).trim();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// Where Workbench writes its log for a config whose `log_file_path` is `configured` (default
/// `workbench.log`), relative paths being relative to the Workbench directory it runs in.
pub fn log_file(configured: Option<&str>, workbench_dir: &Path) -> PathBuf {
    let path = PathBuf::from(
        configured
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("workbench.log"),
    );
    if path.is_absolute() {
        path
    } else {
        workbench_dir.join(path)
    }
}

/// The length of the log before a run starts, so only that run's lines are read afterwards.
pub fn log_offset(log: &Path) -> u64 {
    std::fs::metadata(log).map(|m| m.len()).unwrap_or(0)
}

/// The log written since `offset` (all of it if the file was rotated or truncated meanwhile).
pub fn log_since(log: &Path, offset: u64) -> String {
    let Ok(mut file) = std::fs::File::open(log) else {
        return String::new();
    };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let start = if offset <= len { offset } else { 0 };
    let mut bytes = Vec::new();
    if file.seek(SeekFrom::Start(start)).is_ok() {
        let _ = file.read_to_end(&mut bytes);
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The rollback config a `create` / `create_from_files` run wrote, from its log
/// (`Writing rollback configuration file to <path>.`). The first one is the run's own: with
/// secondary tasks each config writes another, after the root's.
pub fn rollback_config_from_log(log_text: &str, workbench_dir: &Path) -> Option<PathBuf> {
    const MARKER: &str = "Writing rollback configuration file to ";
    let path = log_text
        .lines()
        .filter_map(|line| line.find(MARKER).map(|at| &line[at + MARKER.len()..]))
        .map(|rest| rest.trim().strip_suffix('.').unwrap_or(rest.trim()))
        .find(|p| !p.is_empty())
        .map(PathBuf::from)?;
    Some(if path.is_absolute() {
        path
    } else {
        workbench_dir.join(path)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn created_nodes_are_counted_and_the_first_is_kept() {
        let mut report = RunReport::default();
        for line in [
            "OK, connection to Drupal at http://localhost verified.",
            "Node for \"Small boats in Havana Harbour\" (record 01) created at http://localhost/node/33.",
            "+ Node for child \"Page 1\" created at http://localhost/node/34.",
            "Node for \"IMG_1\" created at https://islandora.dev/node/35 (CSV row 3).",
            "Media for IMG_1410.tif created and added to http://localhost/node/33.",
        ] {
            report.observe(line);
        }
        assert_eq!(report.first_node_id, Some(33));
        assert_eq!(report.nodes_created, 3);
    }

    #[test]
    fn output_paths_are_read_without_the_full_stop() {
        let mut report = RunReport::default();
        report.observe("Contact sheet is at /wb/contact_sheet_output/contact_sheet.htm.");
        report.observe("CSV template saved at input_data/metadata.csv.csv_file_template.");
        assert_eq!(
            report.contact_sheet.as_deref(),
            Some(Path::new("/wb/contact_sheet_output/contact_sheet.htm"))
        );
        assert_eq!(
            report.csv_template.as_deref(),
            Some(Path::new("input_data/metadata.csv.csv_file_template"))
        );
    }

    #[test]
    fn the_rollback_config_comes_from_the_log() {
        let log = "\
12-Oct-26 10:00:00 - INFO - Writing rollback CSV to input_data/rollback.csv.
12-Oct-26 10:00:00 - INFO - Writing rollback configuration file to /wb/rollback.2026_10_12_10_00_00.yml.
12-Oct-26 10:00:09 - INFO - Writing rollback configuration file to /wb/rollback.yml.secondary.
";
        assert_eq!(
            rollback_config_from_log(log, Path::new("/wb")),
            Some(PathBuf::from("/wb/rollback.2026_10_12_10_00_00.yml"))
        );
        assert_eq!(
            rollback_config_from_log(
                "x - INFO - Writing rollback configuration file to rollback.yml.",
                Path::new("/wb")
            ),
            Some(PathBuf::from("/wb/rollback.yml"))
        );
        assert_eq!(
            rollback_config_from_log("nothing here", Path::new("/wb")),
            None
        );
    }

    #[test]
    fn only_the_new_part_of_the_log_is_read() {
        let dir = std::env::temp_dir().join(format!("wb-log-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("workbench.log");
        std::fs::write(&log, "old line\n").unwrap();
        let offset = log_offset(&log);
        std::fs::write(&log, "old line\nnew line\n").unwrap();
        assert_eq!(log_since(&log, offset), "new line\n");
        // Truncated meanwhile: read it all.
        std::fs::write(&log, "fresh\n").unwrap();
        assert_eq!(log_since(&log, offset), "fresh\n");
        assert_eq!(
            log_file(None, Path::new("/wb")),
            Path::new("/wb/workbench.log")
        );
        assert_eq!(
            log_file(Some("/var/log/wb.log"), Path::new("/wb")),
            Path::new("/var/log/wb.log")
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
