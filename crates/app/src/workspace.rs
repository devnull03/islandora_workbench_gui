use std::time::Duration;

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
use workbench_integration::{StreamLine, run_command_streaming};

use crate::path_picker::PathPickerBrowseRow;
use crate::settings::AppSettings;

/// What the workspace is doing right now — drives disabled + loading on actions.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WorkspacePhase {
    Idle,
    /// Google Drive “Process” is running.
    GdriveBusy,
    /// Check or Run Ingest is running (`check` matches the Check button).
    IngestBusy { check: bool },
}

pub struct Workspace {
    phase: WorkspacePhase,
    gdrive_link: Entity<InputState>,
    ingest_files_dir: Entity<InputState>,
    saved_config_select: Entity<SelectState<Vec<SharedString>>>,
    server_select: Entity<SelectState<Vec<SharedString>>>,
    synced_task_labels: Vec<SharedString>,
    synced_server_labels: Vec<SharedString>,
    log_state: Entity<InputState>,
    /// Keep input/select subscriptions alive so typing and selections re-validate buttons.
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let log_state = cx.new(|cx| InputState::new(window, cx).multi_line(true));

        let gdrive_link =
            cx.new(|cx| InputState::new(window, cx).placeholder("Paste Google Drive link..."));

        let ingest_files_dir =
            cx.new(|cx| InputState::new(window, cx).placeholder("Directory for ingest files..."));

        let saved_config_select = cx.new(|cx| SelectState::new(vec![], None, window, cx));

        let server_select = cx.new(|cx| SelectState::new(vec![], None, window, cx));

        let mut _subscriptions = Vec::new();

        _subscriptions.push(cx.subscribe(&gdrive_link, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        }));
        _subscriptions.push(cx.subscribe(&ingest_files_dir, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        }));
        _subscriptions.push(cx.subscribe(
            &saved_config_select,
            |_, _, event: &SelectEvent<Vec<SharedString>>, cx| {
                if matches!(event, SelectEvent::Confirm(_)) {
                    cx.notify();
                }
            },
        ));
        _subscriptions.push(cx.subscribe(
            &server_select,
            |_, _, event: &SelectEvent<Vec<SharedString>>, cx| {
                if matches!(event, SelectEvent::Confirm(_)) {
                    cx.notify();
                }
            },
        ));

        Self {
            phase: WorkspacePhase::Idle,
            gdrive_link,
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

    /// Process is enabled when the URL field is non-empty and nothing else is running.
    fn gdrive_ready(&self, cx: &App) -> bool {
        !self
            .gdrive_link
            .read(cx)
            .value()
            .trim()
            .is_empty()
    }

    /// Check / Run Ingest need a local ingest dir plus both selects chosen.
    fn ingest_ready(&self, cx: &App) -> bool {
        let dir_ok = !self
            .ingest_files_dir
            .read(cx)
            .value()
            .trim()
            .is_empty();
        let task_ok = self
            .saved_config_select
            .read(cx)
            .selected_value()
            .is_some();
        let server_ok = self.server_select.read(cx).selected_value().is_some();
        dir_ok && task_ok && server_ok
    }

    fn sync_select_items(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = AppSettings::get(cx);
        let task_labels: Vec<SharedString> = settings
            .task_configs
            .iter()
            .map(|t| t.label.clone())
            .collect();
        let server_labels: Vec<SharedString> = settings
            .server_configs
            .iter()
            .map(|s| s.label.clone())
            .collect();

        if task_labels != self.synced_task_labels {
            self.synced_task_labels = task_labels.clone();
            self.saved_config_select.update(cx, |state, cx| {
                state.set_items(task_labels, window, cx);
            });
        }
        if server_labels != self.synced_server_labels {
            self.synced_server_labels = server_labels.clone();
            self.server_select.update(cx, |state, cx| {
                state.set_items(server_labels, window, cx);
            });
        }
    }

    fn process_gdrive_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.phase_idle() || !self.gdrive_ready(cx) {
            return;
        }
        self.phase = WorkspacePhase::GdriveBusy;
        cx.notify();

        let url = self.gdrive_link.read(cx).value();
        let entity = cx.entity().clone();
        cx.spawn_in(window, async move |_, cx| {
            // Stand-in for download / prep work; replace with real integration.
            cx.background_executor()
                .timer(Duration::from_millis(800))
                .await;

            cx.update(|window, cx| {
                entity.update(cx, |this, cx| {
                    this.append_log(
                        &format!("[INFO] Process Google Drive link (stub): {}", url),
                        window,
                        cx,
                    );
                    this.phase = WorkspacePhase::Idle;
                    cx.notify();
                });
            })
            .ok();
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

    fn run_dummy_ingest(&mut self, check: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.phase_idle() || !self.ingest_ready(cx) {
            return;
        }
        self.phase = WorkspacePhase::IngestBusy { check };
        cx.notify();

        self.clear_logs(window, cx);
        let start = if check {
            "[INFO] Starting ingest (check mode)..."
        } else {
            "[INFO] Starting ingest..."
        };
        self.append_log(start, window, cx);

        let dummy_messages = vec![
            (300, "[INFO] Loading configuration..."),
            (500, "[INFO] Config loaded from: /path/to/config.yml"),
            (400, "[INFO] Validating input files..."),
            (600, "[WARN] Found 3 files with missing metadata"),
            (800, "[INFO] Processing batch 1 of 5..."),
            (500, "[INFO] Successfully processed 25 items"),
            (700, "[INFO] Processing batch 2 of 5..."),
            (400, "[INFO] Successfully processed 25 items"),
            (600, "[INFO] Processing batch 3 of 5..."),
            (
                900,
                "[ERROR] Failed to process item ID: 12345 - Connection timeout",
            ),
            (500, "[INFO] Retrying failed items..."),
            (700, "[INFO] Processing batch 4 of 5..."),
            (400, "[INFO] Successfully processed 25 items"),
            (600, "[INFO] Processing batch 5 of 5..."),
            (500, "[INFO] Successfully processed 24 items"),
            (
                300,
                "[INFO] Processing complete. Total: 125 items, Success: 124, Failed: 1",
            ),
        ];

        let entity = cx.entity().clone();
        cx.spawn_in(window, async move |_, cx| {
            for (delay_ms, message) in dummy_messages {
                cx.background_executor()
                    .timer(Duration::from_millis(delay_ms))
                    .await;

                let msg = message.to_string();
                cx.update(|window, cx| {
                    entity.update(cx, |this, cx| {
                        this.append_log(&msg, window, cx);
                    });
                })
                .ok();
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

                let msg = match line {
                    StreamLine::Stdout(msg) => msg,
                    StreamLine::Stderr(msg) => format!("[STDERR] {}", msg),
                    StreamLine::Done(code) => format!("[INFO] Process exited with code: {}", code),
                    StreamLine::Error(e) => format!("[ERROR] {}", e),
                };

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
            .justify_around()
            .child(
                v_flex()
                    .h_1_2()
                    .gap_3()
                    .child(
                        v_flex()
                            .gap_1()
                            .w_full()
                            .child(Label::new("Google Drive").font_semibold())
                           .child(
                                h_flex()
                                    .gap_2()
                                    .w_full()
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.))
                                            .child(
                                                Input::new(&self.gdrive_link)
                                                    .disabled(!idle)
                                                    .w_full(),
                                            ),
                                    )
                                    .child(
                                        Button::new("process-gdrive")
                                            .outline()
                                            .label("Process")
                                            .loading(process_loading)
                                            .disabled(process_disabled || process_loading)
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.process_gdrive_link(window, cx);
                                            })),
                                    ),
                            )
                            .child(
                                Label::new(
                                    "Paste a folder or file share link, then run Process to prepare files for ingest.",
                                )
                                .text_sm()
                                .text_color(cx.theme().muted_foreground),
                            )

                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .w_full()
                            .child(Label::new("Ingest files directory").font_semibold())
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
                                Label::new(
                                    "Folder containing CSV, media, and other files Islandora Workbench should read.",
                                )
                                .text_sm()
                                .text_color(cx.theme().muted_foreground),
                            ),
                    )
                    .child(
                        v_flex()
                            .gap_2()
                            .w_full()
                            .child(
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
                                    .disabled(ingest_actions_disabled || check_loading || run_loading)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.run_dummy_ingest(true, window, cx);
                                    })),
                            )
                            .child(
                                Button::new("run-ingest")
                                    .primary()
                                    .label("Run Ingest")
                                    .loading(run_loading)
                                    .disabled(ingest_actions_disabled || check_loading || run_loading)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.run_dummy_ingest(false, window, cx);
                                    })),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .h_1_2()
                    .gap_1()
                    .child(
                        Input::new(&self.log_state)
                            .disabled(true)
                            .flex_1()
                    ),
            )
    }
}
