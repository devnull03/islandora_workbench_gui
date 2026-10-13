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
    /// The Workbench log and its length before the run, so only this run's lines are read.
    pub log: PathBuf,
    pub log_offset: u64,
    pub output_csv: Option<PathBuf>,
    /// Cancel was pressed once (SIGINT sent); a second press kills.
    pub cancelling: bool,
}

/// What the last finished run left behind — the facts the results strip offers actions on.
#[derive(Clone)]
pub(super) struct LastRun {
    pub action: WorkbenchAction,
    pub succeeded: bool,
    pub cancelled: bool,
    pub report: RunReport,
    pub task: Option<String>,
    pub log: PathBuf,
    /// The rollback config a create run wrote, if it wrote one and it exists.
    pub rollback_config: Option<PathBuf>,
    pub output_csv: Option<PathBuf>,
}

impl LastRun {
    /// A create run that stopped part-way can be resumed in recovery mode from its first node.
    pub fn resumable_from(&self) -> Option<u64> {
        let creates = matches!(self.task.as_deref(), Some("create"));
        (self.action == WorkbenchAction::Run && creates && !self.succeeded)
            .then_some(self.report.first_node_id)
            .flatten()
    }
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
        self.start_workbench(action, None, window, cx);
    }

    /// Run Ingest: the task itself, unlocked only by a Check that passed.
    pub(super) fn run_task(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.stage != WorkflowStage::CheckPassed {
            return;
        }
        self.start_workbench(WorkbenchAction::Run, None, window, cx);
    }

    /// Start Workbench for `action` on the selected config — or on `config_override`, which is
    /// how a rollback config runs. Everything a run needs is resolved first and any failure is
    /// logged, so a half-started run never holds the window lock.
    pub(super) fn start_workbench(
        &mut self,
        action: WorkbenchAction,
        config_override: Option<PathBuf>,
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
                self.provision_then_run(action, dest, window, cx);
                return;
            }
        }

        match self.prepare_run(&action, config_override, cx) {
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
        config_override: Option<PathBuf>,
        cx: &App,
    ) -> anyhow::Result<PreparedRun> {
        let options = self.run_options(action, cx)?;
        let config_path = match config_override {
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
        let processed = if reads_rows && !self.running_rollback(action, &config_path, cx) {
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
        Ok(PreparedRun {
            wb_info,
            handler,
            options,
            task,
            log,
            output_csv,
            confirms,
        })
    }

    /// Is `config_path` the last run's rollback config? Its own `input_csv` must stand.
    fn running_rollback(&self, action: &WorkbenchAction, config_path: &Path, _: &App) -> bool {
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
            task,
            log,
            output_csv,
            confirms,
        } = prepared;
        let log_offset = outcome::log_offset(&log);
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
            log,
            log_offset,
            output_csv,
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

    /// The stream ended: record what the run left behind and release the window.
    fn finish_run(&mut self, succeeded: bool, cx: &mut Context<Self>) {
        self.op = Operation::None;
        WindowLock::set(false, cx);
        let Some(session) = self.running.take() else {
            cx.notify();
            return;
        };
        // Only a Check Workbench passed (exit 0) unlocks Run Ingest. Anything else that ran —
        // the task, a rollback — needs a fresh Check before the next run.
        self.stage = match session.action {
            WorkbenchAction::Check { .. } if succeeded => WorkflowStage::CheckPassed,
            WorkbenchAction::Check { .. } | WorkbenchAction::Run => WorkflowStage::Ready,
            _ => self.stage,
        };
        let rollback_config = (session.action == WorkbenchAction::Run
            && writes_rollback(session.task.as_deref()))
        .then(|| {
            let text = outcome::log_since(&session.log, session.log_offset);
            let workbench_dir = workbench_info(cx).ok()?.install_path;
            outcome::rollback_config_from_log(&text, &workbench_dir)
        })
        .flatten()
        .filter(|path| path.is_file());
        self.last_run = Some(LastRun {
            action: session.action,
            succeeded,
            cancelled: session.cancelling,
            report: session.report,
            task: session.task,
            log: session.log,
            rollback_config,
            output_csv: session.output_csv.filter(|path| path.is_file()),
        });
        cx.notify();
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
                "[INFO] Cancel sent (ctrl-c). Workbench stops after the current item; press \
                 Cancel again to stop it immediately.",
                window,
                cx,
            );
        }
        cx.notify();
    }

    /// The row range, prompt skipping and recovery node for this run. Row ranges only apply to
    /// Check and Run, and must read as numbers.
    fn run_options(&self, action: &WorkbenchAction, cx: &App) -> anyhow::Result<RunOptions> {
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
                WorkbenchAction::Run => self.recovery_from_node,
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
    task: Option<String>,
    log: PathBuf,
    output_csv: Option<PathBuf>,
    confirms: bool,
}

impl Workspace {
    /// Download the workbench tool into `dest` on a background thread, then re-enter `run_ingest`.
    /// Used for first-run provisioning when the user opted in via the installer.
    fn provision_then_run(
        &mut self,
        action: WorkbenchAction,
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
                            this.start_workbench(action.clone(), None, window, cx);
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
    use super::{LastRun, RunReport, WorkbenchAction, writes_rollback};
    use std::path::PathBuf;

    fn last_run(task: &str, succeeded: bool, first: Option<u64>) -> LastRun {
        LastRun {
            action: WorkbenchAction::Run,
            succeeded,
            cancelled: false,
            report: RunReport {
                first_node_id: first,
                ..Default::default()
            },
            task: Some(task.into()),
            log: PathBuf::new(),
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
    }

    #[test]
    fn only_create_tasks_write_rollback_files() {
        assert!(writes_rollback(Some("create")));
        assert!(writes_rollback(Some("create_from_files")));
        assert!(!writes_rollback(Some("update")));
        assert!(!writes_rollback(None));
    }
}
