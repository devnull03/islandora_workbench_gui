mod gdrive_log;
mod log_viewer;
mod run;
mod steps;
mod streaming;

use log_viewer::LogViewer;

use std::path::{Path, PathBuf};

use gpui::*;
use gpui_component::{
    input::{InputEvent, InputState},
    scroll::ScrollableElement,
    select::{SelectEvent, SelectState},
    v_flex,
};
use workbench_integration::{language_url_from_server_base, process_google_sheet_source};

use settings::AppSettings;
use ui::DetailSelectItem;

use self::gdrive_log::{
    preprocess_error_message, preprocess_start_message, preprocess_success_messages,
};
use crate::helpers::workbench_input_data_dir;

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
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_select_items(window, cx);

        if self.log_expanded {
            return self.render_log_expanded(cx).into_any_element();
        }

        // The three steps have intrinsic heights and the log has a floor, so a short window can
        // need more room than it has. `min_h(relative(1.))` keeps the column stretching to fill a
        // tall window (so the log still grows) while letting it exceed a short one — at which
        // point this scrolls, instead of the overflow pushing the status bar off-screen.
        div()
            .size_full()
            .child(
                v_flex()
                    .w_full()
                    .min_h(relative(1.))
                    .p_4()
                    .gap_4()
                    .child(self.render_input_source(cx))
                    .child(self.render_config(cx))
                    .child(self.render_server(cx))
                    .child(self.render_log(cx)),
            )
            .overflow_y_scrollbar()
            .into_any_element()
    }
}
