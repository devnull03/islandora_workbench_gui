mod gdrive_log;
mod streaming;

use std::path::PathBuf;

use gpui::*;
use gpui_component::{
    ActiveTheme, Disableable, IconName, StyledExt,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputEvent, InputState},
    label::Label,
    select::{Select, SelectEvent, SelectState},
    v_flex,
};
use workbench_integration::{
    WbInfo, WorkbenchConfigHandler,
    language_url_from_server_base, process_google_sheet_metadata,
    run_ingest_streaming,
};

use crate::{select_items::DetailSelectItem, helpers::get_file};
use settings::AppSettings;
use settings::path_picker::PathPickerBrowseRow;

use self::gdrive_log::{
    sheet_preprocess_error_message, sheet_preprocess_start_message,
    sheet_preprocess_success_messages,
};
use crate::helpers::{
    reveal_in_folder, workbench_input_data_dir,
};
use self::streaming::spawn_stream_to_log;

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
    pending_logs: Vec<String>,

    gdrive_link: Entity<InputState>,
    collection_node_id: Entity<InputState>,
    ingest_files_dir: Entity<InputState>,
    saved_config_select: Entity<SelectState<Vec<DetailSelectItem>>>,
    server_select: Entity<SelectState<Vec<DetailSelectItem>>>,
    synced_task_labels: Vec<SharedString>,
    synced_server_labels: Vec<SharedString>,
    
    log_state: Entity<InputState>,
    /// Keep input/select subscriptions alive so typing and selections re-validate buttons.
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let log_state = cx.new(|cx| InputState::new(window, cx).multi_line(true));

        let gdrive_link = cx.new(|cx| InputState::new(window, cx).placeholder("Sheets URL…"));

        let collection_node_id = cx.new(|cx| InputState::new(window, cx).placeholder("2"));
        collection_node_id.update(cx, |state, cx| {
            state.set_value("2", window, cx);
        });

        let ingest_files_dir =
            cx.new(|cx| InputState::new(window, cx).placeholder("Directory for ingest files..."));

        let saved_config_select = cx.new(|cx| SelectState::new(vec![], None, window, cx));

        let server_select = cx.new(|cx| SelectState::new(vec![], None, window, cx));

        // Restore persisted field values before subscriptions are wired up.
        let saved_gdrive = AppSettings::get(cx).values.get("gdrive_link").map(|v| v.text());
        let saved_ingest = AppSettings::get(cx).values.get("ingest_files_dir").map(|v| v.text());
        if let Some(v) = saved_gdrive.filter(|v| !v.is_empty()) {
            gdrive_link.update(cx, |s, cx| s.set_value(v.to_string(), window, cx));
        }
        if let Some(v) = saved_ingest.filter(|v| !v.is_empty()) {
            ingest_files_dir.update(cx, |s, cx| s.set_value(v.to_string(), window, cx));
        }

        let mut _subscriptions = Vec::new();

        // `Change`: typing / paste. `Focus`/`Blur`: moving between URL and Node fields updates readiness.
        // Defer `notify` so readiness reads input state after GPUI applies the edit (paste/IME).
        // (Do not use `observe` on `InputState`: it also fires every cursor-blink tick.)
        // Defer `notify` so readiness reads input state after GPUI applies the edit (paste/IME).
        // `reset_validation` only mutates `self.phase` so it's safe to call before the defer.
        _subscriptions.push(cx.subscribe(&gdrive_link, |this, _, event: &InputEvent, cx| {
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
        }));
        _subscriptions.push(
            cx.subscribe(&collection_node_id, |this, _, event: &InputEvent, cx| {
                if matches!(
                    event,
                    InputEvent::Change | InputEvent::Focus | InputEvent::Blur
                ) {
                    if matches!(event, InputEvent::Change) {
                        this.reset_validation();
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
            pending_logs: Vec::new(),
            gdrive_link,
            collection_node_id,
            ingest_files_dir,
            saved_config_select,
            server_select,
            synced_task_labels: Vec::new(),
            synced_server_labels: Vec::new(),
            log_state,
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

    /// Process needs a sheet URL, a saved server (for language mapping JSON), and Workbench path
    /// (outputs go to `{workbench}/input_data/metadata.csv`).
    fn gdrive_ready(&self, cx: &App) -> bool {
        if self.gdrive_link.read(cx).value().trim().is_empty() {
            return false;
        }
        if self.collection_node_id.read(cx).value().trim().is_empty() {
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
                if first_population {
                    if let Some(label) = &default_task {
                        state.set_selected_value(label, window, cx);
                    }
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
                if first_population {
                    if let Some(label) = &default_server {
                        state.set_selected_value(label, window, cx);
                    }
                }
            });
        }
    }

    fn process_gdrive_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_idle() || !self.gdrive_ready(cx) {
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
        let node_id = self.collection_node_id.read(cx).value().to_string();
        let metadata_csv = input_data_dir.join("metadata.csv");

        self.append_log(
            &sheet_preprocess_start_message(
                node_id.trim(),
                sheet_url.trim(),
                &language_url,
                &metadata_csv,
            ),
            window,
            cx,
        );

        let entity = cx.entity().clone();
        cx.spawn_in(window, async move |_, cx| {
            let sheet_url = sheet_url.clone();
            let language_url = language_url.clone();
            let input_data_dir = input_data_dir.clone();
            let node_id = node_id.trim().to_string();
            let result = cx
                .background_spawn(async move {
                    process_google_sheet_metadata(
                        &sheet_url,
                        &input_data_dir,
                        language_url.as_str(),
                        node_id.as_str(),
                    )
                })
                .await;

            let _ = cx.update(|window, app| {
                let _ = entity.update(app, |this, cx| {
                    match &result {
                        Ok(res) => {
                            for line in sheet_preprocess_success_messages(res) {
                                this.append_log(&line, window, cx);
                            }
                            this.stage = WorkflowStage::GdriveProcessed;
                        }
                        Err(e) => {
                            this.append_log(&sheet_preprocess_error_message(e), window, cx);
                        }
                    }
                    this.op = Operation::None;
                    cx.notify();
                });
            });
        })
        .detach();
    }

    /// Queue a log message from a context without `window` access. Flushed on the next render.
    pub(crate) fn push_log(&mut self, message: String, cx: &mut Context<Self>) {
        self.pending_logs.push(message);
        cx.notify();
    }

    pub(crate) fn append_log(&self, message: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.log_state.update(cx, |state, cx| {
            let current = state.value();
            let new_value = if current.is_empty() {
                message.to_string()
            } else {
                format!("{}\n{}", current, message)
            };
            state.set_value(new_value, window, cx);
        });
    }

    fn clear_logs(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.log_state.update(cx, |state, cx| {
            state.set_value("".to_string(), window, cx);
        });
    }

    fn run_ingest(&mut self, check: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_idle() || !self.ingest_ready(cx) {
            return;
        }

        if check {
            self.op = Operation::CheckRunning;
        } else {
            if self.stage != WorkflowStage::CheckPassed {
                return;
            }
            self.op = Operation::IngestRunning;
        }

        cx.notify();
        let label = if check { "CHECK" } else { "RUN" };
        self.append_log(&format!("--- {} started ---", label), window, cx);

        // selected_value() is the file path (stored directly in DetailSelectItem::value)
        let config_path = match self.saved_config_select.read(cx).selected_value() {
            Some(p) => PathBuf::from(p.as_ref()),
            None => {
                self.append_log("[ERROR] No task config selected", window, cx);
                self.op = Operation::None;
                cx.notify();
                return;
            }
        };

        let settings = AppSettings::get(cx);
        let workbench_path_str = settings.values.get("workbench_path").map(|v| v.text()).unwrap_or_default();
        let use_uv = settings.values.get("use_uv").map(|v| v.bool()).unwrap_or(false);

        if workbench_path_str.trim().is_empty() {
            self.append_log("[ERROR] Workbench path not configured in settings", window, cx);
            self.op = Operation::None;
            cx.notify();
            return;
        }

        let server_url = match self.server_select.read(cx).selected_value() {
            Some(url) => url.to_string(),
            None => {
                self.append_log("[ERROR] No server selected", window, cx);
                self.op = Operation::None;
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

        let wb_info = WbInfo::new(PathBuf::from(workbench_path_str.trim()), use_uv);
        let mut config_handler = match WorkbenchConfigHandler::new(config_path).load() {
            Ok(h) => h,
            Err(e) => {
                self.append_log(&format!("[ERROR] Failed to load config: {e}"), window, cx);
                self.op = Operation::None;
                cx.notify();
                return;
            }
        };

        if let Err(e) = config_handler.update_config_fields(&server_url, credentials_file) {
            self.append_log(&format!("[ERROR] Failed to update config: {e}"), window, cx);
            self.op = Operation::None;
            cx.notify();
            return;
        }

        let rx = match run_ingest_streaming(&wb_info, &config_handler, check) {
            Ok(r) => r,
            Err(e) => {
                self.append_log(&format!("[ERROR] Failed to start ingest: {e}"), window, cx);
                self.op = Operation::None;
                cx.notify();
                return;
            }
        };

        let entity = cx.entity().clone();
        spawn_stream_to_log(entity, rx, window, cx, move |this, cx| {
            this.op = Operation::None;
            this.stage = if check {
                WorkflowStage::CheckPassed
            } else {
                WorkflowStage::Ready
            };
            cx.notify();
        });
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pending = std::mem::take(&mut self.pending_logs);
        for msg in &pending {
            self.append_log(msg, window, cx);
        }

        self.sync_select_items(window, cx);

        let idle = self.is_idle();
        let gdrive_ok = self.gdrive_ready(cx);
        let ingest_ok = self.ingest_ready(cx);

        let process_loading = self.op == Operation::GdriveBusy;
        let check_loading = self.op == Operation::CheckRunning;
        let run_loading = self.op == Operation::IngestRunning;

        let process_disabled = !idle || !gdrive_ok;
        let ingest_actions_disabled = !idle || !ingest_ok;
        let open_processed_enabled = idle && self.stage >= WorkflowStage::GdriveProcessed;
        let run_enabled = idle && self.stage == WorkflowStage::CheckPassed;

        v_flex()
            .size_full()
            .p_4()
            .gap_3()
            .child(
                v_flex()
                    .w_full()
                    .gap_3()
                    .child(
                        v_flex()
                            .gap_2()
                            .w_full()
                            .child(Label::new("Metadata sheet").font_semibold())
                            .child(
                                h_flex()
                                    .gap_2()
                                    .w_full()
                                    .items_start()
                                    .child(
                                        div().w(relative(0.7)).min_w(px(0.)).child(
                                            v_flex()
                                                .gap_1()
                                                .justify_start()
                                                .child(Label::new("URL").text_sm())
                                                .child(
                                                    Input::new(&self.gdrive_link)
                                                        .disabled(!idle)
                                                        .w_full(),
                                                )
                                                .child(
                                                    Label::new("Google sheets url")
                                                        .text_sm()
                                                        .text_color(cx.theme().muted_foreground),
                                                ),
                                        ),
                                    )
                                    .child(
                                        div().w(relative(0.3)).min_w(px(0.)).child(
                                            v_flex()
                                                .gap_1()
                                                .justify_start()
                                                .child(Label::new("Collection nid").text_sm())
                                                .child(
                                                    Input::new(&self.collection_node_id)
                                                        .disabled(!idle)
                                                        .w_full(),
                                                )
                                                .child(
                                                    Label::new(
                                                        "Collection nid",
                                                    )
                                                    .text_sm()
                                                    .text_color(cx.theme().muted_foreground),
                                                ),
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
                                                this.process_gdrive_link(window, cx);
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
                        v_flex()
                            .gap_1()
                            .w_full()
                            .child(Label::new("Ingest").font_semibold())
                            .child(PathPickerBrowseRow {
                                input: self.ingest_files_dir.clone(),
                                browse: Button::new("browse-ingest-dir")
                                    .icon(IconName::FolderOpen)
                                    .outline()
                                    .disabled(!idle)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        get_file(
                                            window,
                                            cx,
                                            &this.ingest_files_dir,
                                            "Select directory for ingest files".into(),
                                            true,
                                        );
                                    })),
                            })
                           .child(
                                Label::new("Media mapped in metadata sheet")
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground),
                            ),
                    )
                    .child(
                        v_flex().gap_2().w_full().child(
                            h_flex()
                                .w_full()
                                .gap_4()
                                .justify_around()
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .min_w(px(0.))
                                        .gap_1()
                                        .child(Label::new("Saved config").text_sm())
                                        .child(
                                            Select::new(&self.saved_config_select)
                                                .placeholder("Select saved config…")
                                                .disabled(!idle)
                                                .w_full(),
                                        )
                                        .child(
                                            Label::new("Workbench YAML / task profile")
                                                .text_sm()
                                                .text_color(cx.theme().muted_foreground),
                                        ),
                                )
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .min_w(px(0.))
                                        .gap_1()
                                        .child(Label::new("Ingest server").text_sm())
                                        .child(
                                            Select::new(&self.server_select)
                                                .placeholder("Select server…")
                                                .disabled(!idle)
                                                .w_full(),
                                        )
                                        .child(
                                            Label::new("Islandora endpoint for this run")
                                                .text_sm()
                                                .text_color(cx.theme().muted_foreground),
                                        ),
                                ),
                        ),
                    )
                    .child(
                        h_flex()
                            .justify_end()
                            .gap_2()
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
            )
            .child(
                v_flex().flex_1().min_h_0().w_full().gap_1().child(
                    Input::new(&self.log_state)
                        .disabled(true)
                        .flex_1()
                        .min_h_0()
                        .w_full(),
                ),
            )
    }
}
