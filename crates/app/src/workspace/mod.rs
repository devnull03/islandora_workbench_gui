mod gdrive_log;
mod log_viewer;
mod streaming;

use log_viewer::LogViewer;

use std::path::{Path, PathBuf};

use gpui::*;
use gpui_component::{
    ActiveTheme, Disableable, IconName, Sizable, StyledExt,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputEvent, InputState},
    label::Label,
    select::{Select, SelectEvent, SelectState},
    v_flex,
};
use workbench_integration::{
    WbInfo, WorkbenchConfigHandler, language_url_from_server_base, process_google_sheet_source,
    provision_workbench, run_ingest_streaming,
};

use crate::helpers::get_file;
use settings::AppSettings;
use settings::path_picker::PathPickerBrowseRow;
use ui::{DetailSelectItem, LabeledSelect};
use window_wrapper::WindowLock;

use self::gdrive_log::{
    preprocess_error_message, preprocess_start_message, preprocess_success_messages,
};
use self::streaming::spawn_stream_to_log;
use crate::helpers::{
    per_user_workbench_dir, registry_install, reveal_in_folder, workbench_input_data_dir,
};
use config_builder::open_config_builder;

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

/// What async operation is currently running. Drives loading spinners and blanket-disables inputs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Operation {
    None,
    GdriveBusy,
    CheckRunning,
    IngestRunning,
}

/// How far through the workflow the user has progressed. Drives which actions are unlocked.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum WorkflowStage {
    Unfilled,
    Ready,
    GdriveProcessed,
    CheckPassed,
}

pub struct Workspace {
    op: Operation,
    stage: WorkflowStage,
    log_viewer: Entity<LogViewer>,
    log_expanded: bool,

    gdrive_link: Entity<InputState>,
    ingest_files_dir: Entity<InputState>,
    input_source_select: Entity<SelectState<Vec<DetailSelectItem>>>,
    processor_select: Entity<SelectState<Vec<DetailSelectItem>>>,
    saved_config_select: Entity<SelectState<Vec<DetailSelectItem>>>,
    server_select: Entity<SelectState<Vec<DetailSelectItem>>>,
    synced_task_labels: Vec<SharedString>,
    synced_server_labels: Vec<SharedString>,

    /// Keep input/select subscriptions alive so typing and selections re-validate buttons.
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let log_viewer = cx.new(LogViewer::new);

        let gdrive_link = cx.new(|cx| InputState::new(window, cx).placeholder("Sheet URL..."));

        let ingest_files_dir =
            cx.new(|cx| InputState::new(window, cx).placeholder("Directory for ingest files..."));

        let saved_config_select = cx.new(|cx| SelectState::new(vec![], None, window, cx));

        let server_select = cx.new(|cx| SelectState::new(vec![], None, window, cx));
        let input_source_select = cx.new(|cx| {
            SelectState::new(
                vec![DetailSelectItem {
                    label: "Google Sheet → CSV".into(),
                    subtitle: "Built-in source adapter".into(),
                    value: "google-sheet".into(),
                    divider_above: false,
                }],
                None,
                window,
                cx,
            )
        });
        input_source_select.update(cx, |state, cx| {
            let value: SharedString = "google-sheet".into();
            state.set_selected_value(&value, window, cx);
        });
        let processor_select = cx.new(|cx| {
            SelectState::new(
                vec![DetailSelectItem {
                    label: "Workbench preprocessor".into(),
                    subtitle: "Built-in Rust importer".into(),
                    value: "workbench-preprocessor".into(),
                    divider_above: false,
                }],
                None,
                window,
                cx,
            )
        });
        processor_select.update(cx, |state, cx| {
            let value: SharedString = "workbench-preprocessor".into();
            state.set_selected_value(&value, window, cx);
        });

        // Restore persisted field values before subscriptions are wired up.
        let saved_gdrive = AppSettings::get(cx)
            .values
            .get("gdrive_link")
            .map(|v| v.text());
        let saved_ingest = AppSettings::get(cx)
            .values
            .get("ingest_files_dir")
            .map(|v| v.text());
        if let Some(v) = saved_gdrive.filter(|v| !v.is_empty()) {
            gdrive_link.update(cx, |s, cx| s.set_value(v.to_string(), window, cx));
        }
        if let Some(v) = saved_ingest.filter(|v| !v.is_empty()) {
            ingest_files_dir.update(cx, |s, cx| s.set_value(v.to_string(), window, cx));
        }

        let mut _subscriptions = Vec::new();

        // `Change`: typing / paste. `Focus`/`Blur` keep action readiness current.
        // Defer `notify` so readiness reads input state after GPUI applies the edit (paste/IME).
        // (Do not use `observe` on `InputState`: it also fires every cursor-blink tick.)
        // Defer `notify` so readiness reads input state after GPUI applies the edit (paste/IME).
        // `reset_validation` only mutates `self.phase` so it's safe to call before the defer.
        _subscriptions.push(
            cx.subscribe(&gdrive_link, |this, _, event: &InputEvent, cx| {
                if matches!(
                    event,
                    InputEvent::Change | InputEvent::Focus | InputEvent::Blur
                ) {
                    if matches!(event, InputEvent::Change) {
                        this.reset_validation();
                        let val = this.gdrive_link.read(cx).value();
                        AppSettings::set_text("gdrive_link", val, cx);
                    }
                    let workspace = cx.weak_entity();
                    cx.defer(move |app| {
                        let _ = workspace.update(app, |_, cx| cx.notify());
                    });
                }
            }),
        );
        _subscriptions.push(
            cx.subscribe(&ingest_files_dir, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.reset_validation();
                    let val = this.ingest_files_dir.read(cx).value();
                    AppSettings::set_text("ingest_files_dir", val, cx);
                    cx.notify();
                }
            }),
        );
        _subscriptions.push(cx.subscribe(
            &saved_config_select,
            |this, _, event: &SelectEvent<Vec<DetailSelectItem>>, cx| {
                let SelectEvent::Confirm(value) = event;
                AppSettings::set_default_task_config(value.clone(), cx);
                this.reset_validation();
                cx.notify();
            },
        ));
        _subscriptions.push(cx.subscribe(
            &server_select,
            |this, _, event: &SelectEvent<Vec<DetailSelectItem>>, cx| {
                let SelectEvent::Confirm(value) = event;
                AppSettings::set_default_server(value.clone(), cx);
                this.reset_validation();
                cx.notify();
            },
        ));

        Self {
            op: Operation::None,
            stage: WorkflowStage::Unfilled,
            log_viewer,
            log_expanded: false,
            gdrive_link,
            ingest_files_dir,
            input_source_select,
            processor_select,
            saved_config_select,
            server_select,
            synced_task_labels: Vec::new(),
            synced_server_labels: Vec::new(),
            _subscriptions,
        }
    }

    fn is_idle(&self) -> bool {
        self.op == Operation::None
    }

    fn reset_validation(&mut self) {
        if self.op != Operation::None {
            return;
        }
        self.stage = WorkflowStage::Unfilled;
    }

    /// Processing needs a Sheet URL, a saved server (for language mapping JSON), and Workbench path
    /// (outputs go to `{workbench}/input_data/metadata.csv`).
    fn process_ready(&self, cx: &App) -> bool {
        if self.gdrive_link.read(cx).value().trim().is_empty() {
            return false;
        }
        if self.server_select.read(cx).selected_value().is_none() {
            return false;
        }
        workbench_input_data_dir(cx).is_some()
    }

    /// Check / Run Ingest need a local ingest dir plus both selects chosen.
    fn ingest_ready(&self, cx: &App) -> bool {
        let dir_ok = !self.ingest_files_dir.read(cx).value().trim().is_empty();
        let task_ok = self.saved_config_select.read(cx).selected_value().is_some();
        let server_ok = self.server_select.read(cx).selected_value().is_some();
        dir_ok && task_ok && server_ok
    }

    fn sync_select_items(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = AppSettings::get(cx);
        let task_configs = settings.task_configs.clone();
        let server_configs = settings.server_configs.clone();
        let default_task = settings.default_task_config.clone();
        let default_server = settings.default_server.clone();
        let task_labels: Vec<SharedString> = task_configs.iter().map(|t| t.label.clone()).collect();
        let server_labels: Vec<SharedString> =
            server_configs.iter().map(|s| s.label.clone()).collect();

        if task_labels != self.synced_task_labels {
            let first_population = self.synced_task_labels.is_empty();
            self.synced_task_labels = task_labels.clone();
            let items: Vec<DetailSelectItem> = task_configs
                .iter()
                .enumerate()
                .map(|(i, t)| DetailSelectItem::from((i, t)))
                .collect();
            self.saved_config_select.update(cx, |state, cx| {
                state.set_items(items, window, cx);
                if first_population && let Some(label) = &default_task {
                    state.set_selected_value(label, window, cx);
                }
            });
        }
        if server_labels != self.synced_server_labels {
            let first_population = self.synced_server_labels.is_empty();
            self.synced_server_labels = server_labels.clone();
            let items: Vec<DetailSelectItem> = server_configs
                .iter()
                .enumerate()
                .map(|(i, s)| DetailSelectItem::from((i, s)))
                .collect();
            self.server_select.update(cx, |state, cx| {
                state.set_items(items, window, cx);
                if first_population && let Some(label) = &default_server {
                    state.set_selected_value(label, window, cx);
                }
            });
        }
    }

    fn process_metadata(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_idle() || !self.process_ready(cx) {
            return;
        }

        let Some(server_url) = self.server_select.read(cx).selected_value() else {
            return;
        };
        let language_url = language_url_from_server_base(server_url.as_ref());

        self.op = Operation::GdriveBusy;
        cx.notify();

        let Some(input_data_dir) = workbench_input_data_dir(cx) else {
            return;
        };
        let sheet_url = self.gdrive_link.read(cx).value().to_string();
        let config_path = self
            .saved_config_select
            .read(cx)
            .selected_value()
            .map(|value| PathBuf::from(value.as_ref()));
        let metadata_csv = input_data_dir.join("metadata.csv");

        self.append_log(
            &preprocess_start_message(
                "Workbench preprocessor",
                Path::new(&sheet_url),
                &language_url,
                &metadata_csv,
                config_path.as_deref(),
            ),
            window,
            cx,
        );

        let entity = cx.entity().clone();
        cx.spawn_in(window, async move |_, cx| {
            let sheet_url = sheet_url.clone();
            let language_url = language_url.clone();
            let input_data_dir = input_data_dir.clone();
            let result = cx
                .background_spawn(async move {
                    process_google_sheet_source(&sheet_url, &input_data_dir, language_url.as_str())
                })
                .await;

            let _ = cx.update(|window, app| {
                entity.update(app, |this, cx| {
                    match &result {
                        Ok(res) => {
                            for line in preprocess_success_messages(res) {
                                this.append_log(&line, window, cx);
                            }
                            this.stage = WorkflowStage::GdriveProcessed;
                        }
                        Err(e) => {
                            this.append_log(&preprocess_error_message(e), window, cx);
                        }
                    }
                    this.op = Operation::None;
                    cx.notify();
                });
            });
        })
        .detach();
    }

    /// Append an external status event to the workspace log.
    pub fn push_log(&mut self, message: String, cx: &mut Context<Self>) {
        self.log_viewer.update(cx, |lv, cx| lv.append(&message, cx));
    }

    pub(crate) fn append_log(&self, message: &str, _window: &mut Window, cx: &mut Context<Self>) {
        self.log_viewer.update(cx, |lv, cx| lv.append(message, cx));
    }

    #[allow(dead_code)] // for an upcoming "clear logs" action
    fn clear_logs(&self, _window: &mut Window, cx: &mut Context<Self>) {
        self.log_viewer.update(cx, |lv, cx| lv.clear(cx));
    }

    fn run_ingest(&mut self, check: bool, window: &mut Window, cx: &mut Context<Self>) {
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

        // Read the run inputs straight from settings. Provisioning above guarantees `workbench_path`
        // is populated whenever workbench was app-managed, so the setting is now the single source of
        // truth. `WbInfo::new` falls back to `which("uv")` when `uv_path` is absent.
        let settings = AppSettings::get(cx);
        let wb_dir = settings
            .values
            .get("workbench_path")
            .map(|v| v.text())
            .filter(|s| !s.trim().is_empty())
            .map(|s| PathBuf::from(s.trim()));
        let uv_path = settings
            .values
            .get("uv_path")
            .map(|v| v.text())
            .filter(|s| !s.trim().is_empty())
            .map(|s| PathBuf::from(s.trim()));
        let use_uv = settings
            .values
            .get("use_uv")
            .map(|v| v.bool())
            .unwrap_or(false);

        let Some(wb_dir) = wb_dir else {
            self.append_log(
                "[ERROR] Workbench path not configured in settings",
                window,
                cx,
            );
            self.op = Operation::None;
            WindowLock::set(false, cx);
            cx.notify();
            return;
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

        let wb_info = WbInfo::new(wb_dir, use_uv, uv_path);
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

        let auto_accept = AppSettings::get(cx)
            .values
            .get("auto_accept_prompts")
            .map(|v| v.bool())
            .unwrap_or(false);

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

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_select_items(window, cx);

        let idle = self.is_idle();
        let gdrive_ok = self.process_ready(cx);
        let ingest_ok = self.ingest_ready(cx);
        let auto_accept = AppSettings::get(cx)
            .values
            .get("auto_accept_prompts")
            .map(|v| v.bool())
            .unwrap_or(false);

        let process_loading = self.op == Operation::GdriveBusy;
        let check_loading = self.op == Operation::CheckRunning;
        let run_loading = self.op == Operation::IngestRunning;

        let config_selected = self.saved_config_select.read(cx).selected_value().is_some();
        let server_selected = self.server_select.read(cx).selected_value().is_some();

        let process_disabled = !idle || !gdrive_ok;
        let ingest_actions_disabled = !idle || !ingest_ok;
        let open_processed_enabled = idle && self.stage >= WorkflowStage::GdriveProcessed;
        let run_enabled = idle && self.stage == WorkflowStage::CheckPassed;

        if self.log_expanded {
            // Compute the workspace's usable height: full viewport minus the
            // AppTitleBar (TITLE_BAR_HEIGHT = 34px) and the StatusBar (~30px).
            // Using an explicit pixel height instead of size_full() so the
            // LogViewer entity view has a concrete reference for its own h_full().
            let workspace_h = window.viewport_size().height - px(34. + 30.);
            return div()
                .w_full()
                .h(workspace_h)
                .overflow_hidden()
                .p_4()
                .relative()
                .child(self.log_viewer.clone())
                .child(
                    div().absolute().right_4().top_4().child(
                        Button::new("collapse-log")
                            .ghost()
                            .icon(IconName::Minimize)
                            .tooltip("Collapse log")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.log_expanded = false;
                                cx.notify();
                            })),
                    ),
                )
                .into_any_element();
        }

        v_flex()
            .size_full()
            .p_4()
            .gap_3()
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_3()
                    .pb_2()
                    .border_b_1()
                    .border_color(cx.theme().colors.border)
                    .child(
                        Label::new("PROFILE")
                            .text_xs()
                            .text_color(cx.theme().colors.muted_foreground),
                    )
                    .child(
                        Button::new("active-profile")
                            .outline()
                            .small()
                            .label("Default"),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("manage-profiles")
                            .ghost()
                            .small()
                            .label("Manage profiles"),
                    ),
            )
            .child(
                v_flex()
                    .w_full()
                    .flex_1()
                    .gap_3()
                    .child(
                        v_flex()
                            .w_full()
                            .gap_2()
                            .child(Label::new("1  Input source").font_semibold())
                            .child(
                                Select::new(&self.input_source_select)
                                    .w_full()
                                    .disabled(!idle),
                            )
                            .child(
                                v_flex()
                                    .w_full()
                                    .gap_2()
                                    .items_start()
                                    .p_3()
                                    .rounded_md()
                                    .bg(cx.theme().colors.secondary)
                                    .child(div().flex_1().min_w(px(0.)).child(
                                        v_flex()
                                            .w_full()
                                            .gap_1()
                                            .child(Label::new("Sheet URL").text_sm())
                                            .child(Input::new(&self.gdrive_link).w_full().disabled(!idle)),
                                    ))
                                    .child(
                                        div().flex_1().min_w(px(0.)).child(
                                            v_flex()
                                                .w_full()
                                                .gap_1()
                                                .child(Label::new("Ingest dir").text_sm())
                                                .child(PathPickerBrowseRow {
                                                    input: self.ingest_files_dir.clone(),
                                                    browse: Button::new("browse-ingest-dir")
                                                        .icon(IconName::FolderOpen)
                                                        .outline()
                                                        .disabled(!idle)
                                                        .on_click(cx.listener(
                                                            |this, _, window, cx| {
                                                                get_file(
                                                                    window,
                                                                    cx,
                                                                    &this.ingest_files_dir,
                                                                    "Select directory for ingest files"
                                                                        .into(),
                                                                    true,
                                                                );
                                                            },
                                                        )),
                                                }),
                                        ),
                                    )
                                    .child(
                                        div().flex_1().min_w(px(0.)).child(
                                            LabeledSelect::new(
                                                "Processing",
                                                &self.processor_select,
                                            )
                                            .description("Outputs a new metadata CSV path")
                                            .disabled(!idle),
                                        ),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .w_full()
                                    .gap_2()
                                    .justify_end()
                                    .child(
                                        Button::new("process-gdrive")
                                            .outline()
                                            .label("Process")
                                            .loading(process_loading)
                                            .disabled(process_disabled || process_loading)
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.process_metadata(window, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("open-gdrive-output")
                                            .outline()
                                            .icon(IconName::Folder)
                                            .label("Open processed")
                                            .disabled(!open_processed_enabled)
                                            .on_click(cx.listener(|_, _, window, cx| {
                                                let Some(dir) = workbench_input_data_dir(cx) else {
                                                    return;
                                                };
                                                let metadata_csv = dir.join("metadata.csv");
                                                if !metadata_csv.is_file() {
                                                    return;
                                                }
                                                let entity = cx.entity().clone();
                                                cx.spawn_in(window, async move |_, cx| {
                                                    if let Err(e) = reveal_in_folder(&metadata_csv)
                                                    {
                                                        let msg = format!(
                                                            "[ERROR] Failed to reveal output: {e}"
                                                        );
                                                        cx.update(|window, cx| {
                                                            entity.update(cx, |this, cx| {
                                                                this.append_log(&msg, window, cx);
                                                            });
                                                        })
                                                        .ok();
                                                    }
                                                })
                                                .detach();
                                            })),
                                    ),
                            ),
                    )
                    .child(
                        v_flex().gap_2().w_full()
                            .child(Label::new("2  Config and server").font_semibold())
                            .child(
                            h_flex()
                                .w_full()
                                .gap_4()
                                .child(
                                    h_flex()
                                        .flex_1()
                                        .items_center()
                                        .gap_1()
                                        .child(
                                            LabeledSelect::new(
                                                "Saved config (optional for processing)",
                                                &self.saved_config_select,
                                            )
                                            .placeholder("Select saved config…")
                                            .description("Workbench YAML / task profile")
                                            .disabled(!idle),
                                        )
                                        .child(
                                            Button::new("edit-config")
                                                .ghost()
                                                .small()
                                                .icon(IconName::Settings2)
                                                .tooltip("Edit selected config in the config builder")
                                                .disabled(!config_selected)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    let Some(path) = this
                                                        .saved_config_select
                                                        .read(cx)
                                                        .selected_value()
                                                    else {
                                                        return;
                                                    };
                                                    open_config_builder(
                                                        Some(PathBuf::from(path.as_ref())),
                                                        cx,
                                                    );
                                                })),
                                        )
                                        .child(
                                            Button::new("new-config")
                                                .ghost()
                                                .small()
                                                .icon(IconName::Plus)
                                                .tooltip("Create a config in the config builder")
                                                .on_click(|_, _, cx| open_config_builder(None, cx)),
                                        )
                                        .child(
                                            Button::new("reveal-config-file")
                                                .ghost()
                                                .small()
                                                .icon(IconName::FolderOpen)
                                                .tooltip("Reveal config file")
                                                .disabled(!config_selected)
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    let Some(val) = this
                                                        .saved_config_select
                                                        .read(cx)
                                                        .selected_value()
                                                    else {
                                                        return;
                                                    };
                                                    let path = PathBuf::from(val.as_ref());
                                                    let entity = cx.entity().clone();
                                                    cx.spawn_in(window, async move |_, cx| {
                                                        if let Err(e) = reveal_in_folder(&path) {
                                                            let msg = format!(
                                                                "[ERROR] Failed to reveal config: {e}"
                                                            );
                                                            cx.update(|window, cx| {
                                                                entity.update(cx, |this, cx| {
                                                                    this.append_log(
                                                                        &msg, window, cx,
                                                                    );
                                                                });
                                                            })
                                                            .ok();
                                                        }
                                                    })
                                                    .detach();
                                                })),
                                        ),
                                )
                                .child(
                                    h_flex()
                                        .flex_1()
                                        .items_center()
                                        .gap_1()
                                        .child(
                                            LabeledSelect::new(
                                                "Ingest server",
                                                &self.server_select,
                                            )
                                            .placeholder("Select server…")
                                            .description("Islandora endpoint for this run")
                                            .disabled(!idle),
                                        )
                                        .child(
                                            Button::new("reveal-credentials-file")
                                                .ghost()
                                                .small()
                                                .icon(IconName::FolderOpen)
                                                .tooltip("Reveal credentials file")
                                                .disabled(!server_selected)
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    let Some(server_url) = this
                                                        .server_select
                                                        .read(cx)
                                                        .selected_value()
                                                    else {
                                                        return;
                                                    };
                                                    let Some(path) = AppSettings::get(cx)
                                                        .server_configs
                                                        .iter()
                                                        .find(|s| {
                                                            s.server_url.as_ref()
                                                                == server_url.as_str()
                                                        })
                                                        .map(|s| {
                                                            PathBuf::from(
                                                                s.credentials_file.as_ref(),
                                                            )
                                                        })
                                                    else {
                                                        return;
                                                    };
                                                    if path.as_os_str().is_empty() {
                                                        return;
                                                    }
                                                    let entity = cx.entity().clone();
                                                    cx.spawn_in(window, async move |_, cx| {
                                                        if let Err(e) = reveal_in_folder(&path) {
                                                            let msg = format!(
                                                                "[ERROR] Failed to reveal credentials: {e}"
                                                            );
                                                            cx.update(|window, cx| {
                                                                entity.update(cx, |this, cx| {
                                                                    this.append_log(
                                                                        &msg, window, cx,
                                                                    );
                                                                });
                                                            })
                                                            .ok();
                                                        }
                                                    })
                                                    .detach();
                                                })),
                                        ),
                                ),
                        ),
                    )
                    .child(
                        v_flex().gap_2().w_full()
                            .child(Label::new("3  Review and run").font_semibold())
                            .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .gap_2()
                            .child(
                                Checkbox::new("auto-accept-prompts")
                                    .checked(auto_accept)
                                    .label("Auto-accept prompts")
                                    .disabled(!idle)
                                    .on_click(cx.listener(|_, checked: &bool, _, cx| {
                                        AppSettings::set_bool(
                                            "auto_accept_prompts",
                                            *checked,
                                            cx,
                                        );
                                        cx.notify();
                                    })),
                            )
                            .child(div().flex_1())
                            .child(
                                Button::new("check")
                                    .outline()
                                    .label("Check")
                                    .loading(check_loading)
                                    .disabled(
                                        ingest_actions_disabled || check_loading || run_loading,
                                    )
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.run_ingest(true, window, cx);
                                    })),
                            )
                            .child(
                                Button::new("run-ingest")
                                    .primary()
                                    .label("Run Ingest")
                                    .loading(run_loading)
                                    .disabled(!run_enabled || run_loading)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.run_ingest(false, window, cx);
                                    })),
                            ),
                                ),
                    ),
            )
            .child(
                div()
                    .h_full()
                    .max_h(rems(16.))
                    .w_full()
                    .relative()
                    .group("log-area")
                    .overflow_hidden()
                    .child(self.log_viewer.clone())
                    .child(
                        div()
                            .absolute()
                            .right_1()
                            .top_1()
                            .opacity(0.)
                            .group_hover("log-area", |s| s.opacity(1.))
                            .child(
                                Button::new("expand-log")
                                    .ghost()
                                    .icon(IconName::Maximize)
                                    .tooltip("Expand log")
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.log_expanded = true;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }
}
