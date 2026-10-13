mod log_viewer;
mod preprocess_log;
mod run;
mod run_controls;
mod sources;
mod steps;
mod streaming;

pub use log_viewer::LogViewer;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme as _,
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
use window_wrapper::WindowLock;

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
    /// A Workbench process is running; [`Workspace::running`] says which and for what.
    Workbench,
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
    /// The CSV step 1 produced this session — the script's reported path, not an assumed
    /// `metadata.csv`. Supplied to the root config as `input_csv` at Check/Run (plan Q2), and
    /// what "Open processed" reveals. Cleared when the source or processor changes, because
    /// the file no longer reflects them; changing the config or server does not clear it.
    processed_csv: Option<PathBuf>,
    /// When `processed_csv` was made, local `HH:MM` (`2c`'s "last run 11:52"). Only read while
    /// `processed_csv` is set.
    processed_at: SharedString,
    /// A run's outcome waiting to be shown as a toast; pushed from `render`, which is where the
    /// window is.
    pending_toast: Option<gpui_component::notification::Notification>,
    /// The Workbench process running now, if any.
    running: Option<run::RunSession>,
    /// What the last finished run left behind (rollback config, contact sheet, first node…).
    /// Cleared when the server or config changes: its follow-ups only make sense for the run
    /// they came from.
    last_run: Option<run::LastRun>,
    /// Bumped per finished run, so a late background scan of an older run is dropped.
    last_run_generation: u64,

    /// The profile bar (mockup `3c`/`2c`): shown only when there is more than one profile.
    profile_select: Entity<SelectState<Vec<DetailSelectItem>>>,
    synced_profile_names: Vec<String>,
    /// `AppSettings::profile_generation` this view was last seeded from; a switch re-seeds.
    seen_profile_generation: u64,

    /// Run options (step 3, "Options"): per-session, never written into the config.
    options_open: bool,
    start_row: Entity<InputState>,
    stop_row: Entity<InputState>,
    skip_user_prompts: bool,
    contact_sheet: bool,
    /// The URL field in the quick-delete dialog, kept so a mistyped URL survives reopening it.
    quick_delete_url: Entity<InputState>,
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
    last_script_scan: Instant,

    /// `14 settings · runs 2 secondary configs · edited today`, and the config it describes.
    /// Cached because building it reads the file, which must not happen once per frame.
    config_summary: Option<SharedString>,
    /// How many configs the selected one runs after it, for step 2's `↳ N` badge.
    config_chain: usize,
    /// The config the summary describes, and the builder's write generation it was read at.
    summarised_config: Option<(SharedString, u64)>,

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
        let profile_select = cx.new(|cx| SelectState::new(vec![], None, window, cx));
        let start_row = cx.new(|cx| InputState::new(window, cx).placeholder("first"));
        let stop_row = cx.new(|cx| InputState::new(window, cx).placeholder("last"));
        let quick_delete_url =
            cx.new(|cx| InputState::new(window, cx).placeholder("https://…/node/123"));

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
        // A profile switched from Settings has to reach this window now, not at its next
        // unrelated redraw; `render` applies it.
        // A config file was written (a builder save, a run's rewrite): step 2 re-reads it.
        _subscriptions
            .push(cx.observe_global::<config_builder::ConfigFilesChanged>(|_, cx| cx.notify()));
        _subscriptions.push(cx.observe_global::<AppSettings>(|this, cx| {
            if AppSettings::get(cx).profile_generation != this.seen_profile_generation {
                cx.notify();
            }
        }));

        // `Change`: typing / paste. `Focus`/`Blur` keep action readiness current.
        // Defer `notify` so readiness reads input state after GPUI applies the edit (paste/IME).
        // (Do not use `observe` on `InputState`: it also fires every cursor-blink tick.)
        _subscriptions.push(
            cx.subscribe(&gdrive_link, |this, _, event: &InputEvent, cx| {
                if matches!(
                    event,
                    InputEvent::Change | InputEvent::Focus | InputEvent::Blur
                ) {
                    if matches!(event, InputEvent::Change) {
                        this.reset_validation();
                        this.forget_processed();
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
                    this.forget_processed();
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
                this.forget_processed();
                cx.notify();
            },
        ));
        _subscriptions.push(cx.subscribe(
            &processor_select,
            |this, _, event: &SelectEvent<Vec<DetailSelectItem>>, cx| {
                let SelectEvent::Confirm(value) = event;
                AppSettings::set_text("preprocessor", value.clone().unwrap_or_default(), cx);
                this.reset_validation();
                this.forget_processed();
                cx.notify();
            },
        ));
        _subscriptions.push(cx.subscribe(
            &saved_config_select,
            |this, _, event: &SelectEvent<Vec<DetailSelectItem>>, cx| {
                let SelectEvent::Confirm(value) = event;
                AppSettings::set_default_task_config(value.clone(), cx);
                this.reset_validation();
                this.forget_last_run();
                cx.notify();
            },
        ));
        _subscriptions.push(cx.subscribe(
            &profile_select,
            |this, _, event: &SelectEvent<Vec<DetailSelectItem>>, cx| {
                let SelectEvent::Confirm(Some(name)) = event else {
                    return;
                };
                // Never under a running Workbench: it was started with this profile's server.
                if !this.is_idle() {
                    return;
                }
                let name = name.to_string();
                if let Err(e) = AppSettings::update(cx, |s| s.switch_profile(&name)) {
                    log::warn!("[profiles] {e}");
                }
                cx.notify();
            },
        ));
        _subscriptions.push(cx.subscribe(
            &server_select,
            |this, _, event: &SelectEvent<Vec<DetailSelectItem>>, cx| {
                let SelectEvent::Confirm(value) = event;
                AppSettings::set_default_server(value.clone(), cx);
                this.reset_validation();
                this.forget_last_run();
                cx.notify();
            },
        ));

        Self {
            op: Operation::None,
            stage: WorkflowStage::Unfilled,
            processed_csv: None,
            processed_at: SharedString::default(),
            pending_toast: None,
            running: None,
            last_run: None,
            options_open: false,
            start_row,
            stop_row,
            skip_user_prompts: false,
            contact_sheet: false,
            last_run_generation: 0,
            profile_select,
            synced_profile_names: Vec::new(),
            seen_profile_generation: AppSettings::get(cx).profile_generation,
            quick_delete_url,
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
            last_script_scan: Instant::now(),
            config_summary: None,
            config_chain: 0,
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

    /// The server or config changed: the last run's roll back and resume no longer apply.
    fn forget_last_run(&mut self) {
        if self.op == Operation::None {
            self.last_run = None;
        }
    }

    /// The source or processor changed, so the processed CSV is stale.
    fn forget_processed(&mut self) {
        if self.op == Operation::None {
            self.processed_csv = None;
        }
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

    /// Any Workbench action needs a local ingest dir plus both selects chosen.
    fn workbench_ready(&self, cx: &App) -> bool {
        let dir_ok = !self.ingest_files_dir.read(cx).value().trim().is_empty();
        let task_ok = self.saved_config_select.read(cx).selected_value().is_some();
        let server_ok = self.server_select.read(cx).selected_value().is_some();
        dir_ok && task_ok && server_ok
    }

    /// A different profile is active: re-seed every field and list from it, and drop what
    /// belonged to the old one (its processed CSV, its last run).
    fn apply_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = |key: &str, cx: &App| {
            AppSettings::get(cx)
                .values
                .get(key)
                .map(|v| v.text().to_string())
                .unwrap_or_default()
        };
        for (state, key) in [
            (self.gdrive_link.clone(), "gdrive_link"),
            (self.source_csv.clone(), "source_csv"),
            (self.ingest_files_dir.clone(), "ingest_files_dir"),
        ] {
            let value = text(key, cx);
            state.update(cx, |s, cx| s.set_value(value, window, cx));
        }
        let source = Some(text("input_source", cx))
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| SOURCE_SHEET.to_string());
        let processor = Some(text("preprocessor", cx))
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| sources::PROCESSOR_BUILTIN.to_string());
        self.input_source_select.update(cx, |state, cx| {
            state.set_selected_value(&SharedString::from(source), window, cx);
        });
        // The scripts folder is a profile setting: list the new folder's scripts before
        // selecting one of them, or the selection misses and falls back to the built-in.
        let script_items = sources::processor_items(cx);
        self.synced_script_values = script_items.iter().map(|i| i.value.clone()).collect();
        self.last_script_scan = Instant::now();
        self.processor_select.update(cx, |state, cx| {
            state.set_items(script_items, window, cx);
            state.set_selected_value(&SharedString::from(processor), window, cx);
        });
        // Replace the config and server lists outright — including with empty ones, so a new
        // profile never shows the last profile's servers — and apply the profile's defaults.
        let settings = AppSettings::get(cx);
        let (task_configs, server_configs) = (
            settings.task_configs.clone(),
            settings.server_configs.clone(),
        );
        let (default_task, default_server) = (
            settings.default_task_config.clone(),
            settings.default_server.clone(),
        );
        self.synced_task_labels = task_keys(&task_configs);
        self.synced_server_labels = server_keys(&server_configs);
        self.saved_config_select.update(cx, |state, cx| {
            state.set_items(task_config_items(&task_configs), window, cx);
            state.set_selected_index(None, window, cx);
            if let Some(value) = &default_task {
                state.set_selected_value(value, window, cx);
            }
        });
        self.server_select.update(cx, |state, cx| {
            state.set_items(server_config_items(&server_configs), window, cx);
            state.set_selected_index(None, window, cx);
            if let Some(value) = &default_server {
                state.set_selected_value(value, window, cx);
            }
        });
        self.summarised_config = None;
        self.config_summary = None;
        self.config_chain = 0;
        self.processed_csv = None;
        self.last_run = None;
        self.stage = WorkflowStage::Unfilled;
    }

    fn sync_select_items(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let generation = AppSettings::get(cx).profile_generation;
        if generation != self.seen_profile_generation && self.is_idle() {
            self.seen_profile_generation = generation;
            self.apply_profile(window, cx);
        }
        let names = AppSettings::get(cx).profile_names();
        if names != self.synced_profile_names {
            self.synced_profile_names = names.clone();
            let active: SharedString = AppSettings::get(cx).active_profile_name();
            let items = names
                .iter()
                .map(|n| DetailSelectItem::new(n.clone(), n.clone()))
                .collect();
            self.profile_select.update(cx, |state, cx| {
                state.set_items(items, window, cx);
                state.set_selected_value(&active, window, cx);
            });
        } else {
            let active = AppSettings::get(cx).active_profile_name();
            if self.profile_select.read(cx).selected_value() != Some(&active) {
                self.profile_select.update(cx, |state, cx| {
                    state.set_selected_value(&active, window, cx);
                });
            }
        }
        let settings = AppSettings::get(cx);
        let task_configs = settings.task_configs.clone();
        let server_configs = settings.server_configs.clone();
        let default_task = settings.default_task_config.clone();
        let default_server = settings.default_server.clone();
        let task_labels = task_keys(&task_configs);
        let server_labels = server_keys(&server_configs);

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

        // Scripts live on disk rather than in settings. Throttle discovery so typing and spinner
        // redraws do not become a directory scan on every frame.
        if self.last_script_scan.elapsed() >= Duration::from_secs(2) {
            self.last_script_scan = Instant::now();
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
        }

        self.sync_config_summary(cx);
    }

    /// Rebuild the config summary line when the selection changes — reading and parsing the file
    /// is far too expensive to do once per frame.
    fn sync_config_summary(&mut self, cx: &App) {
        let selected = self.saved_config_select.read(cx).selected_value().cloned();
        let generation = config_builder::ConfigFilesChanged::generation(cx);
        let key = selected.clone().map(|path| (path, generation));
        if key == self.summarised_config {
            return;
        }
        self.summarised_config = key;
        let described = selected.and_then(|path| describe_config(Path::new(path.as_ref())));
        self.config_chain = described.as_ref().map_or(0, |(_, chain)| *chain);
        self.config_summary = described.map(|(summary, _)| summary);
    }

    fn process_metadata(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_idle() || !self.process_ready(cx) {
            return;
        }

        let Some(server_url) = self.server_select.read(cx).selected_value() else {
            return;
        };
        let language_url = language_url_from_server_base(server_url.as_ref());

        let Some(input_data_dir) = workbench_input_data_dir(cx) else {
            self.append_log(
                "[ERROR] Set the Workbench folder in Settings before processing.",
                window,
                cx,
            );
            return;
        };
        self.op = Operation::Preprocessing;
        // No profile switch underneath it (refused while locked); closing is still allowed, as
        // a preprocess cannot be stopped.
        WindowLock::set_preprocessing(true, cx);
        cx.notify();
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
                    WindowLock::set_preprocessing(false, cx);
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
                            this.processed_csv = Some(res.metadata_csv.clone());
                            this.processed_at =
                                chrono::Local::now().format("%H:%M").to_string().into();
                            this.stage = WorkflowStage::SourceProcessed;
                        }
                        Err(e) => {
                            this.append_log(&preprocess_error_message(e), window, cx);
                        }
                    }
                    this.op = Operation::None;
                    WindowLock::set_preprocessing(false, cx);
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
        if let Some(note) = self.pending_toast.take() {
            // After the frame: the notification list lives on the window's root, which is
            // mid-render here.
            window.defer(cx, move |window, cx| {
                use gpui_component::WindowExt as _;
                window.push_notification(note, cx);
            });
        }

        // The three steps have intrinsic heights, so a short window can need more room than it
        // has. `min_h(relative(1.))` keeps the column stretching to fill a tall window while
        // letting it exceed a short one — at which point this scrolls, instead of the overflow
        // pushing the status bar off-screen.
        // `2b`: the docked builder compresses the steps to a rail when it leaves them too little
        // room for the full form.
        let rail = config_builder::BuilderDock::main(cx)
            .and_then(|(_, pane)| pane.read(cx).docked_width(cx))
            .is_some_and(|docked| {
                window.viewport_size().width - docked < ui::tokens::RAIL_BREAKPOINT_W
            });
        let steps = div()
            .id("workspace-steps")
            .w_full()
            .flex_1()
            .min_h(px(0.))
            // Horizontal overflow is never wanted here: the steps must fit the window, and a
            // sheet URL or an absolute path is longer than any window. Without the clip the
            // controls simply run off the right edge.
            .overflow_x_hidden()
            .map(|steps| {
                if rail {
                    steps.child(self.render_rail(cx))
                } else {
                    steps.child(
                        v_flex()
                            .w_full()
                            .min_w(px(0.))
                            .min_h(relative(1.))
                            .p_4()
                            .gap_4()
                            .child(self.render_input_source(cx))
                            .child(self.render_config(cx))
                            .child(self.render_server(cx)),
                    )
                }
            })
            .overflow_y_scrollbar();

        // `2c`/`3c`: the profile bar is its own band between the title bar and the steps, so it
        // stays put while they scroll.
        v_flex()
            .size_full()
            .children(self.render_profile_bar(cx).map(|bar| {
                div()
                    .w_full()
                    .flex_none()
                    .px(ui::tokens::PROFILE_BAR_PAD_X)
                    .py(ui::tokens::PROFILE_BAR_PAD_Y)
                    .bg(cx.theme().colors.table_head)
                    .border_b_1()
                    .border_color(cx.theme().colors.border)
                    .child(bar)
            }))
            .child(steps)
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

/// What the config list shows, compared to decide whether to rebuild it: label and file, so two
/// entries with the same label (across profiles, say) still count as a change.
fn task_keys(configs: &[settings::TaskConfig]) -> Vec<SharedString> {
    configs
        .iter()
        .map(|c| format!("{}\u{0}{}", c.label, c.file_path).into())
        .collect()
}

/// The server list's equivalent of [`task_keys`].
fn server_keys(configs: &[settings::ServerConfig]) -> Vec<SharedString> {
    configs
        .iter()
        .map(|c| format!("{}\u{0}{}", c.label, c.server_url).into())
        .collect()
}

/// `14 settings · runs 2 secondary configs · edited today` — the config row's summary line in
/// mockup `2c`. Returns `None` when the file is gone or unreadable, which the row shows as a
/// broken selection rather than as a summary of nothing.
fn describe_config(path: &Path) -> Option<(SharedString, usize)> {
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
    Some((parts.join(" · ").into(), chained))
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
