//! The check / run-ingest pipeline, plus the first-run workbench provisioning that has to happen
//! before it. Split out of `mod.rs` because none of it touches rendering — it reads settings,
//! rewrites the config, spawns the process and streams the output back into the log.

use std::path::{Path, PathBuf};

use gpui::*;
use workbench_integration::{
    WbInfo, config::WorkbenchConfigHandler, provision_workbench, run_ingest_streaming,
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

impl Workspace {
    pub(super) fn run_ingest(&mut self, check: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_idle() || !self.ingest_ready(cx) {
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
                self.provision_then_run(check, dest, window, cx);
                return;
            }
        }

        if check {
            self.op = Operation::CheckRunning;
        } else {
            if self.stage != WorkflowStage::CheckPassed {
                return;
            }
            self.op = Operation::IngestRunning;
        }

        WindowLock::set(true, cx);
        cx.notify();
        let label = if check { "CHECK" } else { "RUN" };
        self.append_log(&format!("--- {} started ---", label), window, cx);

        // selected_value() is the file path (stored directly in DetailSelectItem::value)
        let config_path = match self.saved_config_select.read(cx).selected_value() {
            Some(p) => PathBuf::from(p.as_ref()),
            None => {
                self.append_log("[ERROR] No task config selected", window, cx);
                self.op = Operation::None;
                WindowLock::set(false, cx);
                cx.notify();
                return;
            }
        };

        // Provisioning above guarantees `workbench_path` is populated whenever workbench was
        // app-managed, so settings are now the single source of truth.
        let wb_info = match workbench_info(cx) {
            Ok(info) => info,
            Err(e) => {
                self.append_log(&format!("[ERROR] {e}"), window, cx);
                self.op = Operation::None;
                WindowLock::set(false, cx);
                cx.notify();
                return;
            }
        };

        let server_url = match self.server_select.read(cx).selected_value() {
            Some(url) => url.to_string(),
            None => {
                self.append_log("[ERROR] No server selected", window, cx);
                self.op = Operation::None;
                WindowLock::set(false, cx);
                cx.notify();
                return;
            }
        };
        let credentials_file = AppSettings::get(cx)
            .server_configs
            .iter()
            .find(|s| s.server_url.as_ref() == server_url.as_str())
            .map(|s| PathBuf::from(s.credentials_file.as_ref()))
            .unwrap_or_default();

        let mut config_handler = match WorkbenchConfigHandler::new(config_path).load() {
            Ok(h) => h,
            Err(e) => {
                self.append_log(&format!("[ERROR] Failed to load config: {e}"), window, cx);
                self.op = Operation::None;
                WindowLock::set(false, cx);
                cx.notify();
                return;
            }
        };

        if let Err(e) = config_handler.update_config_fields(
            &server_url,
            credentials_file,
            &wb_info.install_path,
        ) {
            self.append_log(&format!("[ERROR] Failed to update config: {e}"), window, cx);
            self.op = Operation::None;
            WindowLock::set(false, cx);
            cx.notify();
            return;
        }

        let (rx, stdin_sink) = match run_ingest_streaming(&wb_info, &config_handler, check) {
            Ok(r) => r,
            Err(e) => {
                self.append_log(&format!("[ERROR] Failed to start ingest: {e}"), window, cx);
                self.op = Operation::None;
                WindowLock::set(false, cx);
                cx.notify();
                return;
            }
        };

        // A server marked "always confirm" overrides auto-accept. The point of the flag is that
        // a production host cannot be run against unattended by forgetting a global switch, so
        // the server's answer wins whenever the two disagree.
        let confirms = AppSettings::get(cx)
            .server_configs
            .iter()
            .any(|s| s.server_url.as_ref() == server_url.as_str() && s.needs_confirmation);
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

        let entity = cx.entity().clone();
        spawn_stream_to_log(
            entity,
            rx,
            stdin_sink,
            auto_accept,
            window,
            cx,
            move |this, cx| {
                this.op = Operation::None;
                WindowLock::set(false, cx);
                this.stage = if check {
                    WorkflowStage::CheckPassed
                } else {
                    WorkflowStage::Ready
                };
                cx.notify();
            },
        );
    }

    /// Download the workbench tool into `dest` on a background thread, then re-enter `run_ingest`.
    /// Used for first-run provisioning when the user opted in via the installer.
    fn provision_then_run(
        &mut self,
        check: bool,
        dest: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.op = if check {
            Operation::CheckRunning
        } else {
            Operation::IngestRunning
        };
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
                            this.run_ingest(check, window, cx);
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
