//! What the app asks Workbench to do, as command-line arguments.
//!
//! Workbench's whole CLI surface (`./workbench --help`, upstream `workbench` lines 3218-3288)
//! is one required `--config` plus a handful of modes and run-time overrides. Each one the app
//! exposes is modelled here so the window never builds argument strings by hand, and so the
//! mapping is unit-tested against the flag names Workbench actually parses.

use std::ffi::OsString;

/// One invocation of Workbench against a config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkbenchAction {
    /// `--check`: validate input and exit. `contact_sheet` adds `--contactsheet`, which in a
    /// `create` task writes an HTML contact sheet of the input CSV.
    Check { contact_sheet: bool },
    /// The task itself.
    Run,
    /// `--get_csv_template`: a CSV with a column per field of the config's `content_type`,
    /// written to `<input_dir>/<input_csv>.csv_file_template`.
    CsvTemplate,
    /// `--print_config`: the effective configuration, defaults included.
    PrintConfig,
    /// `--quick_delete_node <url>`: one node and (unless `delete_media_with_nodes: false`) its
    /// media and files.
    QuickDeleteNode(String),
    /// `--quick_delete_media <url>`: one media and its files.
    QuickDeleteMedia(String),
}

impl WorkbenchAction {
    /// Whether this changes content on the server. Used to decide what needs confirming and
    /// what a server marked "always confirm" applies to.
    pub fn is_destructive(&self) -> bool {
        matches!(
            self,
            Self::Run | Self::QuickDeleteNode(_) | Self::QuickDeleteMedia(_)
        )
    }

    /// A short name for the log banner.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Check { .. } => "CHECK",
            Self::Run => "RUN",
            Self::CsvTemplate => "CSV TEMPLATE",
            Self::PrintConfig => "PRINT CONFIG",
            Self::QuickDeleteNode(_) => "QUICK DELETE NODE",
            Self::QuickDeleteMedia(_) => "QUICK DELETE MEDIA",
        }
    }
}

/// Run-time overrides Workbench accepts on the command line, so a one-off change ("start at
/// row 200", "resume from node 12687") never has to be written into the user's config.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunOptions {
    /// `--skip_user_prompts`: answer "y" to `remind_user_to_run_check` and `user_prompts`.
    pub skip_user_prompts: bool,
    /// `--csv_start_row`: the first data row to process (Workbench counts from 1, header
    /// excluded).
    pub csv_start_row: Option<u64>,
    /// `--csv_stop_row`: the last data row to process.
    pub csv_stop_row: Option<u64>,
    /// `--recovery_mode_starting_from_node_id`: resume an interrupted `create`, skipping rows
    /// already in the CSV ID to node ID map (docs: "Recovering from interrupted create tasks").
    pub recovery_from_node_id: Option<u64>,
}

impl RunOptions {
    /// A start row after the stop row processes nothing; say so before Workbench does.
    pub fn validate(&self) -> Result<(), String> {
        if let (Some(start), Some(stop)) = (self.csv_start_row, self.csv_stop_row)
            && start > stop
        {
            return Err(format!(
                "The start row ({start}) is after the stop row ({stop})."
            ));
        }
        Ok(())
    }
}

/// The arguments after `workbench --config <file>`.
///
/// Row ranges and recovery mode only mean something to Check and Run; they are left off the
/// other modes rather than passed and silently ignored.
pub fn workbench_args(action: &WorkbenchAction, options: &RunOptions) -> Vec<OsString> {
    let mut args: Vec<OsString> = Vec::new();
    match action {
        WorkbenchAction::Check { contact_sheet } => {
            args.push("--check".into());
            if *contact_sheet {
                args.push("--contactsheet".into());
            }
        }
        WorkbenchAction::Run => {}
        WorkbenchAction::CsvTemplate => args.push("--get_csv_template".into()),
        WorkbenchAction::PrintConfig => args.push("--print_config".into()),
        WorkbenchAction::QuickDeleteNode(url) => {
            args.push("--quick_delete_node".into());
            args.push(url.trim().into());
        }
        WorkbenchAction::QuickDeleteMedia(url) => {
            args.push("--quick_delete_media".into());
            args.push(url.trim().into());
        }
    }
    if options.skip_user_prompts {
        args.push("--skip_user_prompts".into());
    }
    if matches!(action, WorkbenchAction::Check { .. } | WorkbenchAction::Run) {
        if let Some(row) = options.csv_start_row {
            args.push("--csv_start_row".into());
            args.push(row.to_string().into());
        }
        if let Some(row) = options.csv_stop_row {
            args.push("--csv_stop_row".into());
            args.push(row.to_string().into());
        }
        if let Some(node) = options.recovery_from_node_id {
            args.push("--recovery_mode_starting_from_node_id".into());
            args.push(node.to_string().into());
        }
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: Vec<OsString>) -> Vec<String> {
        args.into_iter().map(|a| a.into_string().unwrap()).collect()
    }

    #[test]
    fn each_action_maps_to_the_flag_workbench_parses() {
        let none = RunOptions::default();
        assert_eq!(
            strings(workbench_args(
                &WorkbenchAction::Check {
                    contact_sheet: false
                },
                &none
            )),
            ["--check"]
        );
        assert_eq!(
            strings(workbench_args(
                &WorkbenchAction::Check {
                    contact_sheet: true
                },
                &none
            )),
            ["--check", "--contactsheet"]
        );
        assert!(workbench_args(&WorkbenchAction::Run, &none).is_empty());
        assert_eq!(
            strings(workbench_args(&WorkbenchAction::CsvTemplate, &none)),
            ["--get_csv_template"]
        );
        assert_eq!(
            strings(workbench_args(&WorkbenchAction::PrintConfig, &none)),
            ["--print_config"]
        );
        assert_eq!(
            strings(workbench_args(
                &WorkbenchAction::QuickDeleteNode(" https://x.org/node/3 ".into()),
                &none
            )),
            ["--quick_delete_node", "https://x.org/node/3"]
        );
        assert_eq!(
            strings(workbench_args(
                &WorkbenchAction::QuickDeleteMedia("https://x.org/media/5/edit".into()),
                &none
            )),
            ["--quick_delete_media", "https://x.org/media/5/edit"]
        );
    }

    #[test]
    fn overrides_apply_to_check_and_run_only() {
        let options = RunOptions {
            skip_user_prompts: true,
            csv_start_row: Some(10),
            csv_stop_row: Some(20),
            recovery_from_node_id: Some(12687),
        };
        assert_eq!(
            strings(workbench_args(&WorkbenchAction::Run, &options)),
            [
                "--skip_user_prompts",
                "--csv_start_row",
                "10",
                "--csv_stop_row",
                "20",
                "--recovery_mode_starting_from_node_id",
                "12687"
            ]
        );
        assert_eq!(
            strings(workbench_args(&WorkbenchAction::CsvTemplate, &options)),
            ["--get_csv_template", "--skip_user_prompts"]
        );
    }

    #[test]
    fn a_backwards_row_range_is_refused() {
        let backwards = RunOptions {
            csv_start_row: Some(5),
            csv_stop_row: Some(2),
            ..Default::default()
        };
        assert!(backwards.validate().is_err());
        assert!(RunOptions::default().validate().is_ok());
    }

    #[test]
    fn only_runs_and_deletes_are_destructive() {
        assert!(WorkbenchAction::Run.is_destructive());
        assert!(WorkbenchAction::QuickDeleteNode(String::new()).is_destructive());
        assert!(
            !WorkbenchAction::Check {
                contact_sheet: true
            }
            .is_destructive()
        );
        assert!(!WorkbenchAction::CsvTemplate.is_destructive());
    }
}
