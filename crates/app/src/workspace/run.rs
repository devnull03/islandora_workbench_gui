//! Every Workbench invocation — Check, Run, and the other modes — plus the first-run workbench
//! provisioning that has to happen before them. Split out of `mod.rs` because none of it touches rendering — it reads settings,
//! rewrites the config, spawns the process and streams the output back into the log.

use std::path::{Path, PathBuf};

use gpui::*;
use workbench_integration::{
    ProcessControl, RunOptions, StreamingProcess, WbInfo, WorkbenchAction,
    config::{Ready, RunOverrides, WorkbenchConfigHandler},
    outcome::{self, RunReport},
    provision_workbench, run_workbench,
};

use settings::AppSettings;
use window_wrapper::WindowLock;

use super::streaming::spawn_stream_to_log;
use super::{Operation, WorkflowStage, Workspace};
use crate::helpers::{per_user_workbench_dir, registry_install};

/// Record an app-provisioned workbench install into the normal settings store, so the path becomes
/// the single source of truth — visible and editable in Settings, exactly like a user-entered value.
///
/// `uv_path`/`use_uv` are only touched when the installer actually bundled a uv. To preserve an
/// explicit user choice, `use_uv` is defaulted on only when the switch has never been set.
fn adopt_provisioned_install(dir: &Path, cx: &mut App) {
    AppSettings::set_text(
        "workbench_path",
        dir.to_string_lossy().to_string().into(),
        cx,
    );
    if let Some(uv) = registry_install().uv_path {
        AppSettings::set_text("uv_path", uv.to_string_lossy().to_string().into(), cx);
        if !AppSettings::get(cx).values.contains_key("use_uv") {
            AppSettings::set_bool("use_uv", true, cx);
        }
    }
}

/// Resolve the Python environment from settings. Both the ingest run and a preprocess script
/// need it, and both should fail the same way when Workbench has not been configured.
pub(super) fn workbench_info(cx: &App) -> anyhow::Result<WbInfo> {
    let settings = AppSettings::get(cx);
    let path = |key: &str| {
        settings
            .values
            .get(key)
            .map(|v| v.text())
            .filter(|s| !s.trim().is_empty())
            .map(|s| PathBuf::from(s.trim()))
    };
    let wb_dir = path("workbench_path")
        .ok_or_else(|| anyhow::anyhow!("Workbench path not configured in settings"))?;
    let use_uv = settings
        .values
        .get("use_uv")
        .map(|v| v.bool())
        .unwrap_or(false);
    Ok(WbInfo::new(
        wb_dir,
        path("python_path"),
        use_uv,
        path("uv_path"),
    ))
}

/// The Workbench process a workspace is running, and what it has reported so far.
pub(super) struct RunSession {
    pub action: WorkbenchAction,
    pub control: ProcessControl,
    pub report: RunReport,
    pub task: Option<String>,
    /// The server and config it ran against: follow-ups (roll back, resume) must act on the
    /// same ones, never on whatever is selected later.
    pub server_url: String,
    pub config_path: PathBuf,
    /// The Workbench log and where this run's lines start in it (0 when the log is rewritten
    /// per run, `log_file_mode: w`).
    pub log: PathBuf,
    pub log_offset: u64,
    pub output_csv: Option<PathBuf>,
    pub workbench_dir: PathBuf,
    /// The recovery-mode start node this run used, if it was a resume.
    pub recovery_from: Option<u64>,
    /// Cancel was pressed once (SIGINT sent); a second press kills.
    pub cancelling: bool,
}

impl Drop for RunSession {
    /// A session dropped with its process still going (the window closed, the app quit) stops
    /// it: in its own process group it would not get the terminal's hang-up and would outlive
    /// the app.
    fn drop(&mut self) {
        self.control.stop_if_running();
    }
}

/// What the last finished run left behind — the facts the results strip offers actions on.
/// All paths are absolute and checked once, when the run ends — rendering reads them as they
/// are and never touches the disk.
#[derive(Clone)]
pub(super) struct LastRun {
    pub action: WorkbenchAction,
    pub succeeded: bool,
    pub cancelled: bool,
    pub report: RunReport,
    pub task: Option<String>,
    pub server_url: String,
    pub config_path: PathBuf,
    pub recovery_from: Option<u64>,
    /// The Workbench log, if it exists.
    pub log: Option<PathBuf>,
    /// The rollback config a create run wrote, if it wrote one and it exists.
    pub rollback_config: Option<PathBuf>,
    pub output_csv: Option<PathBuf>,
}

impl LastRun {
    /// A create run that stopped part-way can be resumed in recovery mode. Workbench exits 0
    /// on ctrl-c, so "stopped part-way" is failed *or* cancelled. The start node is the one
    /// the whole job began at: if this run was itself a resume, its own first node is later
    /// than rows created by the attempt before it, which would then be created twice.
    pub fn resumable_from(&self) -> Option<u64> {
        let creates = matches!(self.task.as_deref(), Some("create" | "create_from_files"));
        (self.action == WorkbenchAction::Run && creates && (!self.succeeded || self.cancelled))
            .then(|| self.recovery_from.or(self.report.first_node_id))
            .flatten()
    }
}

/// How a run is aimed: the selected config unless overridden (a rollback config), and an
/// optional recovery-mode start node.
#[derive(Clone, Default)]
pub(super) struct RunTarget {
    pub config: Option<PathBuf>,
    pub recovery_from: Option<u64>,
}

/// Quick delete only works for a URL on the server the run uses: Workbench requests it with
/// the config's host and reports a misleading 404 otherwise.
pub(super) fn url_on_server(url: &str, server: &str) -> bool {
    let normalise = |s: &str| s.trim().trim_end_matches('/').to_ascii_lowercase();
    let (url, server) = (normalise(url), normalise(server));
    url == server || url.starts_with(&format!("{server}/"))
}

/// `create` and `create_from_files` write rollback files (docs: "Rolling back").
fn writes_rollback(task: Option<&str>) -> bool {
    matches!(task, Some("create" | "create_from_files"))
}

impl Workspace {
    /// Check, in the step-3 sense: `--check` (with a contact sheet if asked) on the selected
    /// config.
    pub(super) fn run_check(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let action = WorkbenchAction::Check {
            contact_sheet: self.contact_sheet,
        };
        self.start_workbench(action, RunTarget::default(), window, cx);
    }

    /// Run Ingest: the task itself, unlocked only by a Check that passed.
    pub(super) fn run_task(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.stage != WorkflowStage::CheckPassed {
            return;
        }
        self.start_workbench(WorkbenchAction::Run, RunTarget::default(), window, cx);
    }

    /// Start Workbench for `action` on the selected config — or on `target.config`, which is
    /// how a rollback config runs. Everything a run needs is resolved first and any failure is
    /// logged, so a half-started run never holds the window lock.
    pub(super) fn start_workbench(
        &mut self,
        action: WorkbenchAction,
        target: RunTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_idle() || !self.workbench_ready(cx) {
            return;
        }

        // First-run provisioning: the user opted in via the installer but `workbench_path` isn't a
        // setting yet. If a previous session already downloaded workbench, adopt that path into
        // settings; otherwise download it (in the background) and re-enter the run when it lands.
        // We only get here when the setting is empty, so this never shadows a user-entered path.
        let wb_unset = AppSettings::get(cx)
            .values
            .get("workbench_path")
            .map(|v| v.text())
            .is_none_or(|s| s.trim().is_empty());
        if wb_unset
            && registry_install().provision_workbench
            && let Some(dest) = per_user_workbench_dir()
        {
            if dest.join("pyproject.toml").exists() {
                adopt_provisioned_install(&dest, cx);
            } else {
                self.provision_then_run(action, target, dest, window, cx);
                return;
            }
        }

        match self.prepare_run(&action, &target, cx) {
            Ok(prepared) => self.launch(action, prepared, window, cx),
            Err(error) => {
                self.append_log(&format!("[ERROR] {error}"), window, cx);
                cx.notify();
            }
        }
    }

    /// Everything that can fail before a process exists: settings, the config rewrite, options.
    fn prepare_run(
        &self,
        action: &WorkbenchAction,
        target: &RunTarget,
        cx: &App,
    ) -> anyhow::Result<PreparedRun> {
        let mut options = self.run_options(action, target, cx)?;
        let config_path = match target.config.clone() {
            Some(path) => path,
            None => self
                .saved_config_select
                .read(cx)
                .selected_value()
                .map(|p| PathBuf::from(p.as_ref()))
                .ok_or_else(|| anyhow::anyhow!("No task config selected"))?,
        };
        let wb_info = workbench_info(cx)?;
        let server_url = self
            .server_select
            .read(cx)
            .selected_value()
            .map(|url| url.to_string())
            .ok_or_else(|| anyhow::anyhow!("No server selected"))?;
        if let WorkbenchAction::QuickDeleteNode(url) | WorkbenchAction::QuickDeleteMedia(url) =
            action
            && !url_on_server(url, &server_url)
        {
            anyhow::bail!("{url} is not on the selected server ({server_url}).");
        }
        let credentials_file = AppSettings::get(cx)
            .server_configs
            .iter()
            .find(|s| s.server_url.as_ref() == server_url.as_str())
            .map(|s| PathBuf::from(s.credentials_file.as_ref()))
            .unwrap_or_default();

        let mut handler = WorkbenchConfigHandler::new(config_path.clone())
            .load()
            .map_err(|e| anyhow::anyhow!("Failed to load config: {e}"))?;
        // The processed CSV belongs to the selected config's run, never to a rollback config
        // (which names the CSV of node IDs it deletes) or to the row-free modes.
        let reads_rows = matches!(action, WorkbenchAction::Check { .. } | WorkbenchAction::Run);
        let processed = if reads_rows && !self.running_rollback(action, &config_path) {
            self.processed_csv.as_deref()
        } else {
            None
        };
        let overrides = RunOverrides {
            host: &server_url,
            credentials_file_path: &credentials_file,
            input_csv: processed,
            require_input_csv: reads_rows,
        };
        handler
            .update_config_fields(&overrides, &wb_info.install_path)
            .map_err(|e| anyhow::anyhow!("Failed to update config: {e}"))?;

        let task = handler.text_setting("task").map(str::to_owned);
        let log = outcome::log_file(handler.text_setting("log_file_path"), &wb_info.install_path);
        // With `log_file_mode: w` Workbench truncates the log at start, so this run's lines
        // begin at 0 whatever the file's length was before.
        let log_rewritten = handler.text_setting("log_file_mode") == Some("w");
        let output_csv = handler.text_setting("output_csv").map(|p| {
            let path = PathBuf::from(p);
            if path.is_absolute() {
                path
            } else {
                wb_info.install_path.join(path)
            }
        });
        let confirms = AppSettings::get(cx)
            .server_configs
            .iter()
            .any(|s| s.server_url.as_ref() == server_url.as_str() && s.needs_confirmation);
        // An "always confirm" server keeps the config's own prompts for anything that changes
        // content, whatever the Options say — that is what the flag is for.
        if confirms && action.is_destructive() {
            options.skip_user_prompts = false;
        }
        Ok(PreparedRun {
            wb_info,
            handler,
            options,
            server_url,
            config_path,
            log_rewritten,
            task,
            log,
            output_csv,
            confirms,
        })
    }

    /// Is `config_path` the last run's rollback config? Its own `input_csv` must stand.
    fn running_rollback(&self, action: &WorkbenchAction, config_path: &Path) -> bool {
        *action == WorkbenchAction::Run
            && self
                .last_run
                .as_ref()
                .and_then(|run| run.rollback_config.as_deref())
                .is_some_and(|rollback| rollback == config_path)
    }

    fn launch(
        &mut self,
        action: WorkbenchAction,
        prepared: PreparedRun,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let PreparedRun {
            wb_info,
            handler,
            options,
            server_url,
            config_path,
            log_rewritten,
            task,
            log,
            output_csv,
            confirms,
        } = prepared;
        let log_offset = if log_rewritten {
            0
        } else {
            outcome::log_offset(&log)
        };
        let process = match run_workbench(&wb_info, &handler, &action, &options) {
            Ok(process) => process,
            Err(e) => {
                self.append_log(
                    &format!("[ERROR] Failed to start Workbench: {e}"),
                    window,
                    cx,
                );
                return;
            }
        };

        self.op = Operation::Workbench;
        WindowLock::set(true, cx);
        self.append_log(&format!("--- {} started ---", action.label()), window, cx);

        // A server marked "always confirm" overrides auto-accept. The point of the flag is that
        // a production host cannot be run against unattended by forgetting a global switch, so
        // the server's answer wins whenever the two disagree.
        let auto_accept = !confirms
            && AppSettings::get(cx)
                .values
                .get("auto_accept_prompts")
                .map(|v| v.bool())
                .unwrap_or(false);
        if confirms {
            self.append_log(
                "[INFO] This server is marked \"always confirm\" — prompts will be shown.",
                window,
                cx,
            );
        }

        let StreamingProcess {
            lines,
            stdin,
            control,
        } = process;
        self.running = Some(RunSession {
            action: action.clone(),
            control,
            report: RunReport::default(),
            task,
            server_url,
            config_path,
            log,
            log_offset,
            output_csv,
            workbench_dir: wb_info.install_path.clone(),
            recovery_from: options.recovery_from_node_id,
            cancelling: false,
        });
        cx.notify();

        let entity = cx.entity().clone();
        spawn_stream_to_log(
            entity,
            lines,
            stdin,
            auto_accept,
            window,
            cx,
            move |this, succeeded, cx| this.finish_run(succeeded, cx),
        );
    }

    /// The stream ended: record what the run left behind and release the window. What needs
    /// the disk — this run's part of the log, the files it wrote — is read on a background
    /// thread and folded in when it arrives.
    fn finish_run(&mut self, succeeded: bool, cx: &mut Context<Self>) {
        self.op = Operation::None;
        WindowLock::set(false, cx);
        let Some(session) = self.running.take() else {
            cx.notify();
            return;
        };
        let cancelled = session.cancelling;
        // Only a Check that passed (exit 0) and was not cancelled unlocks Run Ingest —
        // Workbench exits 0 on ctrl-c too. Anything else that ran needs a fresh Check.
        self.stage = match session.action {
            WorkbenchAction::Check { .. } if succeeded && !cancelled => WorkflowStage::CheckPassed,
            WorkbenchAction::Check { .. } | WorkbenchAction::Run => WorkflowStage::Ready,
            _ => self.stage,
        };
        let scan = RunScan {
            reads_rollback: session.action == WorkbenchAction::Run
                && writes_rollback(session.task.as_deref()),
            log: session.log.clone(),
            log_offset: session.log_offset,
            workbench_dir: session.workbench_dir.clone(),
            output_csv: session.output_csv.clone(),
            report: session.report.clone(),
        };
        let generation = self.last_run_generation.wrapping_add(1);
        self.last_run_generation = generation;
        self.last_run = Some(LastRun {
            action: session.action.clone(),
            succeeded,
            cancelled,
            report: session.report.clone(),
            task: session.task.clone(),
            server_url: session.server_url.clone(),
            config_path: session.config_path.clone(),
            recovery_from: session.recovery_from,
            log: None,
            rollback_config: None,
            output_csv: None,
        });
        cx.notify();

        cx.spawn(async move |this, cx| {
            let found = cx.background_spawn(async move { scan.run() }).await;
            this.update(cx, |this, cx| {
                if this.last_run_generation != generation {
                    return;
                }
                if let Some(run) = this.last_run.as_mut() {
                    run.report = found.report;
                    run.log = found.log;
                    run.rollback_config = found.rollback_config;
                    run.output_csv = found.output_csv;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Cancel: the first press is Workbench's documented ctrl-c (it logs and exits cleanly);
    /// a second press, for a run that ignores it, kills the process.
    pub(super) fn cancel_run(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.running.as_mut() else {
            return;
        };
        if session.cancelling {
            session.control.kill();
            self.append_log("[INFO] Stopping Workbench now.", window, cx);
        } else {
            session.cancelling = true;
            session.control.interrupt();
            self.append_log(
                if cfg!(windows) {
                    "[INFO] Workbench stopped."
                } else {
                    "[INFO] Cancel sent (ctrl-c). Workbench stops what it is doing and exits; \
                     press Stop now if it does not."
                },
                window,
                cx,
            );
        }
        cx.notify();
    }

    /// The row range, prompt skipping and recovery node for this run. Row ranges only apply to
    /// Check and Run, and must read as numbers.
    fn run_options(
        &self,
        action: &WorkbenchAction,
        target: &RunTarget,
        cx: &App,
    ) -> anyhow::Result<RunOptions> {
        let row = |state: &Entity<gpui_component::input::InputState>, what: &str| {
            let text = state.read(cx).value().trim().to_string();
            if text.is_empty() {
                Ok(None)
            } else {
                text.parse::<u64>()
                    .map(Some)
                    .map_err(|_| anyhow::anyhow!("{what} must be a whole number, not \"{text}\""))
            }
        };
        let options = RunOptions {
            skip_user_prompts: self.skip_user_prompts,
            csv_start_row: row(&self.start_row, "The start row")?,
            csv_stop_row: row(&self.stop_row, "The stop row")?,
            recovery_from_node_id: match action {
                WorkbenchAction::Run => target.recovery_from,
                _ => None,
            },
        };
        options.validate().map_err(anyhow::Error::msg)?;
        Ok(options)
    }
}

/// A run with everything resolved, ready to spawn.
struct PreparedRun {
    wb_info: WbInfo,
    handler: WorkbenchConfigHandler<Ready>,
    options: RunOptions,
    server_url: String,
    config_path: PathBuf,
    log_rewritten: bool,
    task: Option<String>,
    log: PathBuf,
    output_csv: Option<PathBuf>,
    confirms: bool,
}

/// What a finished run's files say, read off the UI thread.
struct RunScan {
    reads_rollback: bool,
    log: PathBuf,
    log_offset: u64,
    workbench_dir: PathBuf,
    output_csv: Option<PathBuf>,
    report: RunReport,
}

struct ScanResult {
    report: RunReport,
    log: Option<PathBuf>,
    rollback_config: Option<PathBuf>,
    output_csv: Option<PathBuf>,
}

impl RunScan {
    fn run(self) -> ScanResult {
        let log_text = outcome::log_since(&self.log, self.log_offset);
        let rollback_config = self
            .reads_rollback
            .then(|| outcome::rollback_config_from_log(&log_text, &self.workbench_dir))
            .flatten()
            .filter(|path| path.is_file());
        // Workbench prints "Node … created at" only without `progress_bar`, but always logs
        // it, so the log is the better count when it saw more.
        let mut from_log = RunReport::default();
        for line in log_text.lines() {
            from_log.observe(line);
        }
        let mut report = self.report;
        if from_log.nodes_created > report.nodes_created {
            report.nodes_created = from_log.nodes_created;
            report.first_node_id = from_log.first_node_id;
        }
        // Workbench prints some paths relative to its own folder.
        let absolute = |path: PathBuf| {
            if path.is_absolute() {
                path
            } else {
                self.workbench_dir.join(path)
            }
        };
        report.contact_sheet = report.contact_sheet.map(absolute).filter(|p| p.is_file());
        report.csv_template = report.csv_template.map(absolute).filter(|p| p.is_file());
        ScanResult {
            report,
            log: Some(self.log).filter(|p| p.is_file()),
            rollback_config,
            output_csv: self.output_csv.filter(|p| p.is_file()),
        }
    }
}

impl Workspace {
    /// Download the workbench tool into `dest` on a background thread, then re-enter `run_ingest`.
    /// Used for first-run provisioning when the user opted in via the installer.
    fn provision_then_run(
        &mut self,
        action: WorkbenchAction,
        target: RunTarget,
        dest: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.op = Operation::Workbench;
        WindowLock::set(true, cx);
        cx.notify();
        self.append_log(
            "[INFO] Downloading Islandora Workbench (first-time setup)...",
            window,
            cx,
        );

        let entity = cx.entity().clone();
        cx.spawn_in(window, async move |_, cx| {
            let dl_dest = dest.clone();
            let result = cx
                .background_spawn(async move { provision_workbench(&dl_dest) })
                .await;

            let _ = cx.update(|window, app| {
                entity.update(app, |this, cx| {
                    this.op = Operation::None;
                    WindowLock::set(false, cx);
                    cx.notify();
                    match result {
                        Ok(()) => {
                            this.append_log(
                                &format!("[INFO] Workbench installed to {}", dest.display()),
                                window,
                                cx,
                            );
                            // Persist the freshly downloaded path as a normal setting so it becomes
                            // the single source of truth (and shows up in Settings → Workbench Path).
                            adopt_provisioned_install(&dest, cx);
                            // The setting is now populated, so this proceeds to the actual run.
                            this.start_workbench(action.clone(), target.clone(), window, cx);
                        }
                        Err(e) => {
                            this.append_log(
                                &format!("[ERROR] Failed to download workbench: {e}"),
                                window,
                                cx,
                            );
                        }
                    }
                });
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    // Not `use super::*`: that glob-imports gpui, whose `test` macro shadows the built-in one.
    use super::{LastRun, RunReport, WorkbenchAction, url_on_server, writes_rollback};
    use std::path::PathBuf;

    fn last_run(task: &str, succeeded: bool, first: Option<u64>) -> LastRun {
        LastRun {
            action: WorkbenchAction::Run,
            succeeded,
            cancelled: false,
            server_url: String::new(),
            config_path: PathBuf::new(),
            recovery_from: None,
            report: RunReport {
                first_node_id: first,
                ..Default::default()
            },
            task: Some(task.into()),
            log: None,
            rollback_config: None,
            output_csv: None,
        }
    }

    /// Recovery mode is for an interrupted `create` that got as far as creating a node.
    #[test]
    fn only_an_interrupted_create_is_resumable() {
        assert_eq!(
            last_run("create", false, Some(33)).resumable_from(),
            Some(33)
        );
        assert_eq!(last_run("create", true, Some(33)).resumable_from(), None);
        assert_eq!(last_run("create", false, None).resumable_from(), None);
        assert_eq!(last_run("update", false, Some(33)).resumable_from(), None);
        assert_eq!(
            last_run("create_from_files", false, Some(7)).resumable_from(),
            Some(7)
        );
    }

    /// Workbench exits 0 on ctrl-c, so a cancelled create is resumable though it "succeeded".
    #[test]
    fn a_cancelled_create_is_resumable() {
        let mut run = last_run("create", true, Some(33));
        run.cancelled = true;
        assert_eq!(run.resumable_from(), Some(33));
    }

    /// Resuming a resume keeps the original start node, or the first attempt's rows would be
    /// below the new start and created again.
    #[test]
    fn a_resumed_run_resumes_from_the_original_node() {
        let mut run = last_run("create", false, Some(50));
        run.recovery_from = Some(33);
        assert_eq!(run.resumable_from(), Some(33));
    }

    #[test]
    fn quick_delete_urls_must_be_on_the_server() {
        assert!(url_on_server(
            "https://islandora.dev/node/3",
            "https://islandora.dev/"
        ));
        assert!(url_on_server(
            "HTTPS://Islandora.dev/media/5/edit",
            "https://islandora.dev"
        ));
        assert!(!url_on_server(
            "https://islandora.dev.evil/node/3",
            "https://islandora.dev"
        ));
        assert!(!url_on_server(
            "https://other.org/node/3",
            "https://islandora.dev"
        ));
    }

    #[test]
    fn only_create_tasks_write_rollback_files() {
        assert!(writes_rollback(Some("create")));
        assert!(writes_rollback(Some("create_from_files")));
        assert!(!writes_rollback(Some("update")));
        assert!(!writes_rollback(None));
    }
}
