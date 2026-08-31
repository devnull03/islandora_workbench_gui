mod log_viewer;
mod preprocess_log;
mod run;
mod sources;
mod steps;
mod streaming;

pub use log_viewer::LogViewer;

use std::path::{Path, PathBuf};

use gpui::*;
use gpui_component::{
    dock::{BasePanel, Panel, PanelControl, PanelEvent},
    input::{InputEvent, InputState},
    scroll::ScrollableElement,
    select::{SelectEvent, SelectState},
    v_flex,
};
use workbench_integration::{
    InputSource, PreprocessJob, config::ConfigDraft, language_url_from_server_base, run_preprocess,
};

use settings::AppSettings;
use ui::DetailSelectItem;

use self::sources::{
    ProcessorChoice, SOURCE_CSV, SOURCE_SHEET, server_config_items, task_config_items,
};

use self::preprocess_log::{
    preprocess_error_message, preprocess_start_message, preprocess_success_messages,
};
use crate::helpers::workbench_input_data_dir;

/// What async operation is currently running. Drives loading spinners and blanket-disables inputs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Operation {
    None,
    Preprocessing,
    CheckRunning,
    IngestRunning,
}

/// How far through the workflow the user has progressed. Drives which actions are unlocked.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum WorkflowStage {
    Unfilled,
    Ready,
    SourceProcessed,
    CheckPassed,
}

pub struct Workspace {
    op: Operation,
    stage: WorkflowStage,
    /// The log lives in the bottom dock now, not in this view — this handle is only how run
    /// output reaches it. Owned by `main` and handed to both, so the panel and the writer are
    /// the same log.
    log_viewer: Entity<LogViewer>,
    focus_handle: FocusHandle,

    /// One field per source rather than one shared field: switching source back and forth must
    /// not lose the URL you already pasted, and the two persist under their own settings keys.
    gdrive_link: Entity<InputState>,
    source_csv: Entity<InputState>,
    ingest_files_dir: Entity<InputState>,
    input_source_select: Entity<SelectState<Vec<DetailSelectItem>>>,
    processor_select: Entity<SelectState<Vec<DetailSelectItem>>>,
    saved_config_select: Entity<SelectState<Vec<DetailSelectItem>>>,
    server_select: Entity<SelectState<Vec<DetailSelectItem>>>,
    synced_task_labels: Vec<SharedString>,
    synced_server_labels: Vec<SharedString>,
    synced_script_values: Vec<SharedString>,

    /// `14 settings · runs 2 secondary configs · edited today`, and the config it describes.
    /// Cached because building it reads the file, which must not happen once per frame.
    config_summary: Option<SharedString>,
    summarised_config: Option<SharedString>,

    /// Keep input/select subscriptions alive so typing and selections re-validate buttons.
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub fn new(log_viewer: Entity<LogViewer>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let gdrive_link = cx.new(|cx| InputState::new(window, cx).placeholder("Sheet URL..."));

        let source_csv =
            cx.new(|cx| InputState::new(window, cx).placeholder("Path to a CSV file..."));

        let ingest_files_dir =
            cx.new(|cx| InputState::new(window, cx).placeholder("Directory for ingest files..."));

        let saved_config_select = cx.new(|cx| SelectState::new(vec![], None, window, cx));

        let server_select = cx.new(|cx| SelectState::new(vec![], None, window, cx));
        let input_source_select =
            cx.new(|cx| SelectState::new(sources::source_items(), None, window, cx));
        let processor_select =
            cx.new(|cx| SelectState::new(sources::processor_items(cx), None, window, cx));

        // Restore persisted field values before subscriptions are wired up, so restoring a value
        // does not look like the user editing it.
        let restore =
            |state: &Entity<InputState>, key: &str, window: &mut Window, cx: &mut Context<Self>| {
                let saved = AppSettings::get(cx).values.get(key).map(|v| v.text());
                if let Some(v) = saved.filter(|v| !v.is_empty()) {
                    state.update(cx, |s, cx| s.set_value(v.to_string(), window, cx));
                }
            };
        restore(&gdrive_link, "gdrive_link", window, cx);
        restore(&source_csv, "source_csv", window, cx);
        restore(&ingest_files_dir, "ingest_files_dir", window, cx);

        let saved_source = AppSettings::get(cx)
            .values
            .get("input_source")
            .map(|v| v.text())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| SOURCE_SHEET.into());
        input_source_select.update(cx, |state, cx| {
            state.set_selected_value(&saved_source, window, cx);
        });

        let saved_processor = AppSettings::get(cx)
            .values
            .get("preprocessor")
            .map(|v| v.text())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| sources::PROCESSOR_BUILTIN.into());
        processor_select.update(cx, |state, cx| {
            state.set_selected_value(&saved_processor, window, cx);
        });

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
            cx.subscribe(&source_csv, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.reset_validation();
                    let val = this.source_csv.read(cx).value();
                    AppSettings::set_text("source_csv", val, cx);
                    cx.notify();
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
            &input_source_select,
            |this, _, event: &SelectEvent<Vec<DetailSelectItem>>, cx| {
                let SelectEvent::Confirm(value) = event;
                AppSettings::set_text("input_source", value.clone().unwrap_or_default(), cx);
                this.reset_validation();
                cx.notify();
            },
        ));
        _subscriptions.push(cx.subscribe(
            &processor_select,
            |this, _, event: &SelectEvent<Vec<DetailSelectItem>>, cx| {
                let SelectEvent::Confirm(value) = event;
                AppSettings::set_text("preprocessor", value.clone().unwrap_or_default(), cx);
                this.reset_validation();
                cx.notify();
            },
        ));
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
            focus_handle: cx.focus_handle(),
            gdrive_link,
            source_csv,
            ingest_files_dir,
            input_source_select,
            processor_select,
            saved_config_select,
            server_select,
            synced_task_labels: Vec::new(),
            synced_server_labels: Vec::new(),
            synced_script_values: Vec::new(),
            config_summary: None,
            summarised_config: None,
            _subscriptions,
        }
    }

    /// Which source is picked. Defaults to the sheet so a fresh install behaves as before.
    pub(super) fn source_key(&self, cx: &App) -> SharedString {
        self.input_source_select
            .read(cx)
            .selected_value()
            .cloned()
            .unwrap_or_else(|| SOURCE_SHEET.into())
    }

    /// The input field the current source uses.
    pub(super) fn source_field(&self, cx: &App) -> &Entity<InputState> {
        if self.source_key(cx) == SOURCE_CSV {
            &self.source_csv
        } else {
            &self.gdrive_link
        }
    }

    fn processor_choice(&self, cx: &App) -> ProcessorChoice {
        self.processor_select
            .read(cx)
            .selected_value()
            .map(|v| sources::processor_for(v.as_ref()))
            .unwrap_or(ProcessorChoice::Builtin)
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

    /// Processing needs the current source's field filled, a saved server (for the language
    /// mapping JSON) and a Workbench path — outputs go to `{workbench}/input_data/metadata.csv`.
    fn process_ready(&self, cx: &App) -> bool {
        if self.source_field(cx).read(cx).value().trim().is_empty() {
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
            let items = task_config_items(&task_configs);
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
            let items = server_config_items(&server_configs);
            self.server_select.update(cx, |state, cx| {
                state.set_items(items, window, cx);
                if first_population && let Some(label) = &default_server {
                    state.set_selected_value(label, window, cx);
                }
            });
        }

        // Scripts come from a folder, not from settings rows, so the only way to notice one was
        // added is to look. Comparing values keeps that to a directory read per render.
        let script_items = sources::processor_items(cx);
        let script_values: Vec<SharedString> =
            script_items.iter().map(|i| i.value.clone()).collect();
        if script_values != self.synced_script_values {
            self.synced_script_values = script_values;
            let keep = self.processor_select.read(cx).selected_value().cloned();
            self.processor_select.update(cx, |state, cx| {
                state.set_items(script_items, window, cx);
                // A script that disappeared leaves nothing selected; the caller sees `Builtin`.
                if let Some(value) = keep {
                    state.set_selected_value(&value, window, cx);
                }
            });
        }

        self.sync_config_summary(cx);
    }

    /// Rebuild the config summary line when the selection changes — reading and parsing the file
    /// is far too expensive to do once per frame.
    fn sync_config_summary(&mut self, cx: &App) {
        let selected = self.saved_config_select.read(cx).selected_value();
        if selected == self.summarised_config.as_ref() {
            return;
        }
        self.summarised_config = selected.cloned();
        self.config_summary = selected.and_then(|path| describe_config(Path::new(path.as_ref())));
    }

    fn process_metadata(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_idle() || !self.process_ready(cx) {
            return;
        }

        let Some(server_url) = self.server_select.read(cx).selected_value() else {
            return;
        };
        let language_url = language_url_from_server_base(server_url.as_ref());

        self.op = Operation::Preprocessing;
        cx.notify();

        let Some(input_data_dir) = workbench_input_data_dir(cx) else {
            return;
        };
        let source_key = self.source_key(cx);
        let source_value = self.source_field(cx).read(cx).value().to_string();
        let processor = self.processor_choice(cx);
        let config_path = self
            .saved_config_select
            .read(cx)
            .selected_value()
            .map(|value| PathBuf::from(value.as_ref()));
        let metadata_csv = input_data_dir.join("metadata.csv");

        // Only a script needs Python, and failing here beats failing after a sheet download.
        let workbench = if processor.needs_workbench() {
            match run::workbench_info(cx) {
                Ok(wb) => Some(wb),
                Err(e) => {
                    self.append_log(&format!("[ERROR] {e}"), window, cx);
                    self.op = Operation::None;
                    cx.notify();
                    return;
                }
            }
        } else {
            None
        };

        self.append_log(
            &preprocess_start_message(
                &processor.label(),
                &source_value,
                &language_url,
                &metadata_csv,
                config_path.as_deref(),
            ),
            window,
            cx,
        );

        let entity = cx.entity().clone();
        cx.spawn_in(window, async move |_, cx| {
            let result = cx
                .background_spawn(async move {
                    let source = if source_key == SOURCE_CSV {
                        InputSource::CsvFile(Path::new(&source_value))
                    } else {
                        InputSource::GoogleSheet(&source_value)
                    };
                    run_preprocess(PreprocessJob {
                        source,
                        processor: processor.as_processor(),
                        output_dir: &input_data_dir,
                        language_url: &language_url,
                        config_file: config_path.as_deref(),
                        workbench: workbench.as_ref(),
                    })
                })
                .await;

            let _ = cx.update(|window, app| {
                entity.update(app, |this, cx| {
                    match &result {
                        Ok(res) => {
                            for line in preprocess_success_messages(res) {
                                this.append_log(&line, window, cx);
                            }
                            this.stage = WorkflowStage::SourceProcessed;
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

        // The three steps have intrinsic heights, so a short window can need more room than it
        // has. `min_h(relative(1.))` keeps the column stretching to fill a tall window while
        // letting it exceed a short one — at which point this scrolls, instead of the overflow
        // pushing the status bar off-screen.
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
                    .child(self.render_server(cx)),
            )
            .overflow_y_scrollbar()
    }
}

// --- Dock panel ---

impl Focusable for Workspace {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for Workspace {}

impl BasePanel for Workspace {
    fn panel_name(&self) -> &'static str {
        "Workspace"
    }

    /// The centre of the window. There is nothing behind it to close it onto.
    fn closable(&self, _cx: &App) -> bool {
        false
    }

    fn zoomable(&self, _cx: &App) -> bool {
        false
    }
}

impl Panel for Workspace {
    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        SharedString::from("Workbench")
    }

    fn zoom_control(&self, _cx: &App) -> Option<PanelControl> {
        None
    }
}

/// `14 settings · runs 2 secondary configs · edited today` — the config row's summary line in
/// mockup `2c`. Returns `None` when the file is gone or unreadable, which the row shows as a
/// broken selection rather than as a summary of nothing.
fn describe_config(path: &Path) -> Option<SharedString> {
    let draft = ConfigDraft::load(path).ok()?;
    let mut parts = vec![format!("{} settings", draft.values.len())];

    let chained = draft.secondary_tasks().len();
    if chained > 0 {
        parts.push(format!(
            "runs {chained} secondary config{}",
            if chained == 1 { "" } else { "s" }
        ));
    }
    if let Some(edited) = edited_ago(path) {
        parts.push(edited);
    }
    Some(parts.join(" · ").into())
}

/// Relative rather than a date: "edited today" is what the mockup shows, and it needs no date
/// formatting crate to say it.
fn edited_ago(path: &Path) -> Option<String> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let days = std::time::SystemTime::now()
        .duration_since(modified)
        .ok()?
        .as_secs()
        / 86_400;
    Some(match days {
        0 => "edited today".to_string(),
        1 => "edited yesterday".to_string(),
        d => format!("edited {d} days ago"),
    })
}
