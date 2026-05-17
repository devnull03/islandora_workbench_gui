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
    IngestParams, StreamLine, format_stream_line, language_url_from_server_base,
    process_google_sheet_metadata, run_command_streaming, run_ingest_streaming,
};

use crate::select_items::DetailSelectItem;
use settings::path_picker::PathPickerBrowseRow;
use settings::{AppSettings, ServerConfig};

/// What the workspace is doing right now — drives disabled + loading on actions.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WorkspacePhase {
    Idle,
    /// Google Sheets “Process” (preprocessor) is running.
    GdriveBusy,
    /// Check or Run Ingest is running (`check` matches the Check button).
    IngestBusy {
        check: bool,
    },
}

pub struct Workspace {
    phase: WorkspacePhase,
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

        let mut _subscriptions = Vec::new();

        // `Change`: typing / paste. `Focus`/`Blur`: moving between URL and Node fields updates readiness.
        // Defer `notify` so readiness reads input state after GPUI applies the edit (paste/IME).
        // (Do not use `observe` on `InputState`: it also fires every cursor-blink tick.)
        _subscriptions.push(cx.subscribe(&gdrive_link, |_, _, event: &InputEvent, cx| {
            if matches!(
                event,
                InputEvent::Change | InputEvent::Focus | InputEvent::Blur
            ) {
                let workspace = cx.weak_entity();
                cx.defer(move |app| {
                    let _ = workspace.update(app, |_, cx| cx.notify());
                });
            }
        }));
        _subscriptions.push(
            cx.subscribe(&collection_node_id, |_, _, event: &InputEvent, cx| {
                if matches!(
                    event,
                    InputEvent::Change | InputEvent::Focus | InputEvent::Blur
                ) {
                    let workspace = cx.weak_entity();
                    cx.defer(move |app| {
                        let _ = workspace.update(app, |_, cx| cx.notify());
                    });
                }
            }),
        );
        _subscriptions.push(
            cx.subscribe(&ingest_files_dir, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
        );
        _subscriptions.push(cx.subscribe(
            &saved_config_select,
            |_, _, event: &SelectEvent<Vec<DetailSelectItem>>, cx| {
                if matches!(event, SelectEvent::Confirm(_)) {
                    cx.notify();
                }
            },
        ));
        _subscriptions.push(cx.subscribe(
            &server_select,
            |_, _, event: &SelectEvent<Vec<DetailSelectItem>>, cx| {
                if matches!(event, SelectEvent::Confirm(_)) {
                    cx.notify();
                }
            },
        ));

        Self {
            phase: WorkspacePhase::Idle,
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

    fn phase_idle(&self) -> bool {
        matches!(self.phase, WorkspacePhase::Idle)
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
        let Some(label) = self.server_select.read(cx).selected_value() else {
            return false;
        };
        if server_config_for_label(cx, label).is_none() {
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
        let task_labels: Vec<SharedString> =
            task_configs.iter().map(|t| t.label.clone()).collect();
        let server_labels: Vec<SharedString> =
            server_configs.iter().map(|s| s.label.clone()).collect();

        if task_labels != self.synced_task_labels {
            self.synced_task_labels = task_labels.clone();
            let items: Vec<DetailSelectItem> = task_configs
                .iter()
                .enumerate()
                .map(|(i, t)| DetailSelectItem::from((i, t)))
                .collect();
            self.saved_config_select.update(cx, |state, cx| {
                state.set_items(items, window, cx);
            });
        }
        if server_labels != self.synced_server_labels {
            self.synced_server_labels = server_labels.clone();
            let items: Vec<DetailSelectItem> = server_configs
                .iter()
                .enumerate()
                .map(|(i, s)| DetailSelectItem::from((i, s)))
                .collect();
            self.server_select.update(cx, |state, cx| {
                state.set_items(items, window, cx);
            });
        }
    }

    fn process_gdrive_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.phase_idle() || !self.gdrive_ready(cx) {
            return;
        }

        let Some(server) = self
            .server_select
            .read(cx)
            .selected_value()
            .and_then(|label| server_config_for_label(cx, label))
        else {
            return;
        };
        let language_url = language_url_from_server_base(server.server_url.as_ref());

        self.phase = WorkspacePhase::GdriveBusy;
        cx.notify();

        let Some(input_data_dir) = workbench_input_data_dir(cx) else {
            return;
        };
        let sheet_url = self.gdrive_link.read(cx).value().to_string();
        let node_id = self.collection_node_id.read(cx).value().to_string();
        let metadata_csv = input_data_dir.join("metadata.csv");

        self.append_log(
            &format!(
                "[INFO] Running sheet preprocessor (--full, node={})…\n\
                 [INFO] Sheet URL: {}\n\
                 [INFO] Language mapping URL: {}\n\
                 [INFO] Output: {} (and items CSV in the same folder)",
                node_id.trim(),
                sheet_url.trim(),
                language_url,
                metadata_csv.display(),
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
                            this.append_log(
                                &format!(
                                    "[INFO] Sheet preprocessor finished: rows={}, cells_modified={}, validation_failures={}",
                                    res.processing_stats.total_rows,
                                    res.processing_stats.cells_modified,
                                    res.processing_stats.validation_failures
                                ),
                                window,
                                cx,
                            );
                            this.append_log(
                                &format!("[INFO] Processed CSV: {}", res.processed_output_path),
                                window,
                                cx,
                            );
                            if let (Some(path), Some(stats)) =
                                (res.items_output_path.as_ref(), res.items_stats.as_ref())
                            {
                                this.append_log(
                                    &format!(
                                        "[INFO] Items CSV: {} (items={}, unique_parents={}, skipped={})",
                                        path,
                                        stats.total_items,
                                        stats.unique_parents,
                                        stats.skipped_rows
                                    ),
                                    window,
                                    cx,
                                );
                            }
                        }
                        Err(e) => {
                            this.append_log(
                                &format!("[ERROR] Sheet preprocessor failed: {:#}", e),
                                window,
                                cx,
                            );
                        }
                    }
                    this.phase = WorkspacePhase::Idle;
                    cx.notify();
                });
            });
        })
        .detach();
    }

    fn get_file(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        input: &Entity<InputState>,
        prompt: SharedString,
        is_folder: bool,
    ) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: !is_folder,
            directories: is_folder,
            multiple: false,
            prompt: Some(prompt),
        });

        let input = input.clone();
        cx.spawn_in(window, async move |_, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(path) = paths.first()
            {
                cx.update(|window, cx| {
                    input.update(cx, |state, cx| {
                        state.set_value(path.to_string_lossy().to_string(), window, cx);
                    });
                })
                .ok();
            }
        })
        .detach();
    }

    fn append_log(&self, message: &str, window: &mut Window, cx: &mut Context<Self>) {
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
        if !self.phase_idle() || !self.ingest_ready(cx) {
            return;
        }
        self.phase = WorkspacePhase::IngestBusy { check };
        cx.notify();

        self.clear_logs(window, cx);

        let ingest_files_dir = PathBuf::from(self.ingest_files_dir.read(cx).value().trim());
        let task_label = self
            .saved_config_select
            .read(cx)
            .selected_value()
            .map(|s| s.to_string())
            .unwrap_or_default();
        let server_label = self
            .server_select
            .read(cx)
            .selected_value()
            .map(|s| s.to_string())
            .unwrap_or_default();

        let params = IngestParams {
            check,
            ingest_files_dir: ingest_files_dir.as_path(),
            task_label: task_label.as_str(),
            server_label: server_label.as_str(),
        };
        let rx = run_ingest_streaming(params);

        let entity = cx.entity().clone();
        cx.spawn_in(window, async move |_, cx| {
            while let Ok(line) = rx.recv() {
                let should_break = matches!(line, StreamLine::Done(_) | StreamLine::Error(_));
                let msg = format_stream_line(&line);

                cx.update(|window, cx| {
                    entity.update(cx, |this, cx| {
                        this.append_log(&msg, window, cx);
                    });
                })
                .ok();

                if should_break {
                    break;
                }
            }
            cx.update(|_, cx| {
                entity.update(cx, |this, cx| {
                    this.phase = WorkspacePhase::Idle;
                    cx.notify();
                });
            })
            .ok();
        })
        .detach();
    }

    /// Run a command and stream its output to logs (reserved for workbench CLI integration).
    #[allow(dead_code)]
    fn run_command(
        &self,
        program: &str,
        args: &[&str],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_logs(window, cx);
        self.append_log(
            &format!("[INFO] Running: {} {}", program, args.join(" ")),
            window,
            cx,
        );

        let rx = match run_command_streaming(program, args) {
            Ok(rx) => rx,
            Err(e) => {
                self.append_log(&format!("[ERROR] Failed to start: {}", e), window, cx);
                return;
            }
        };

        let entity = cx.entity().clone();
        cx.spawn_in(window, async move |_, cx| {
            while let Ok(line) = rx.recv() {
                let should_break = matches!(line, StreamLine::Done(_) | StreamLine::Error(_));

                let msg = format_stream_line(&line);

                cx.update(|window, cx| {
                    entity.update(cx, |this, cx| {
                        this.append_log(&msg, window, cx);
                    });
                })
                .ok();

                if should_break {
                    break;
                }
            }
        })
        .detach();
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_select_items(window, cx);

        let idle = self.phase_idle();
        let gdrive_ok = self.gdrive_ready(cx);
        let ingest_ok = self.ingest_ready(cx);

        let process_loading = matches!(self.phase, WorkspacePhase::GdriveBusy);
        let check_loading = matches!(self.phase, WorkspacePhase::IngestBusy { check: true });
        let run_loading = matches!(self.phase, WorkspacePhase::IngestBusy { check: false });

        let process_disabled = !idle || !gdrive_ok;
        let ingest_actions_disabled = !idle || !ingest_ok;

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
                                                        "Node id of main collection in islandora",
                                                    )
                                                    .text_sm()
                                                    .text_color(cx.theme().muted_foreground),
                                                ),
                                        ),
                                    ),
                            )
                            .child(
                                h_flex().w_full().justify_end().child(
                                    Button::new("process-gdrive")
                                        .outline()
                                        .label("Process")
                                        .loading(process_loading)
                                        .disabled(process_disabled || process_loading)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.process_gdrive_link(window, cx);
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
                                        this.get_file(
                                            window,
                                            cx,
                                            &this.ingest_files_dir,
                                            "Select directory for ingest files".into(),
                                            true,
                                        );
                                    })),
                            })
                            .child(
                                Label::new("CSV, media, and config files for Workbench.")
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
                                    .disabled(
                                        ingest_actions_disabled || check_loading || run_loading,
                                    )
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

fn server_config_for_label<'a>(cx: &'a App, label: &SharedString) -> Option<&'a ServerConfig> {
    AppSettings::get(cx)
        .server_configs
        .iter()
        .find(|s| &s.label == label)
}

/// `{workbench_path}/input_data` from Settings (Workbench Path).
fn workbench_input_data_dir(cx: &App) -> Option<PathBuf> {
    let raw = AppSettings::get(cx).values.get("workbench_path")?.text();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(PathBuf::from(trimmed).join("input_data"))
}
