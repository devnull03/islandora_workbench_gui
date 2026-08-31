//! Config-builder UI: search the Workbench setting catalogue, edit settings, and save YAML.
//! control that fits the value's shape, watch local validation, and save the YAML to the config
//! library.
//!
//! Mockups `1a` (search open) and `1b` (settings added, validation, YAML panel). Chaining is in
//! [`chain`], the per-shape controls in [`editors`], the palette in [`search`]. Plan:
//! `docs/plans/stage-1-config-builder.md`.
//!
//! **The draft is the single source of truth.** Every input is seeded from
//! [`ConfigDraft::values`] and writes back into it on change; nothing derives the config from
//! the widget tree. Removing a row drops that setting's inputs so the survivors are rebuilt
//! from the draft rather than left holding shifted values.

mod chain;
mod editors;
mod search;
mod yaml_panel;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

use gpui::*;
use gpui_component::{
    ActiveTheme, Disableable, IconName, Root, Sizable, StyledExt, TitleBar,
    button::ButtonVariants,
    h_flex,
    input::EditorState,
    input::{InputEvent, InputState},
    label::Label,
    scroll::ScrollableElement,
    select::{SelectEvent, SelectState},
    v_flex,
};
use serde_yaml::Value;
use settings::{AppSettings, TaskConfig};
use workbench_integration::config::{
    ConfigDraft,
    catalog::{self, SettingDef},
    chain::SecondaryConfigNode,
    validate::{Problem, Severity, validate},
};

use ui::{DetailSelectItem, app_button};

actions!(config_builder, [OpenConfigBuilder]);

/// Open windows, keyed by the config they are editing (`None` for an unsaved new config), so a
/// second Edit on the same file focuses the window that already has it rather than opening a
/// rival copy with a divergent draft.
#[derive(Default)]
pub struct ConfigBuilderWindows {
    open: HashMap<Option<PathBuf>, AnyWindowHandle>,
}

impl Global for ConfigBuilderWindows {}

/// Width of the read-only YAML preview, and of the editor column beside it. The window is sized
/// as the sum, and grows and shrinks by the panel's width when it is shown or hidden — the panel
/// appearing must not squeeze the editors it exists to explain.
pub(crate) const YAML_PANEL_WIDTH: Pixels = px(340.);
const FALLBACK_EDITOR_WIDTH: Pixels = px(600.);
const FALLBACK_EDITOR_HEIGHT: Pixels = px(800.);

/// Open the builder on `path`, or on a blank draft when `path` is `None`.
pub fn open_config_builder(path: Option<PathBuf>, cx: &mut App) {
    if let Some(handle) = cx.global::<ConfigBuilderWindows>().open.get(&path).copied() {
        if handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
        {
            return;
        }
        cx.global_mut::<ConfigBuilderWindows>().open.remove(&path);
    }

    // The starting width has to agree with whether the YAML panel is showing, or the window
    // opens the wrong size and the first toggle corrects it with a visible jump.
    let with_yaml = AppSettings::get(cx)
        .values
        .get("builder_show_yaml")
        .map(|v| v.bool())
        .unwrap_or(true);
    // The editable side opens at the main window's last/current size. YAML is an additional
    // right-hand surface, never width stolen from the form the user is trying to edit.
    let editor_size = AppSettings::get(cx)
        .main_window_bounds
        .as_ref()
        .filter(|bounds| {
            bounds.width.is_finite()
                && bounds.height.is_finite()
                && bounds.width >= 520.
                && bounds.height >= 420.
        })
        .map(|bounds| size(px(bounds.width), px(bounds.height)))
        .unwrap_or_else(|| size(FALLBACK_EDITOR_WIDTH, FALLBACK_EDITOR_HEIGHT));
    let width = if with_yaml {
        editor_size.width + YAML_PANEL_WIDTH
    } else {
        editor_size.width
    };
    let bounds = Bounds::centered(None, size(width, editor_size.height), cx);
    let options = WindowOptions {
        titlebar: Some(TitleBar::title_bar_options()),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(Size::new(px(520.0), px(420.0))),
        ..Default::default()
    };

    cx.spawn(async move |cx| {
        let key = path.clone();
        let opened = cx.open_window(options, |window, cx| {
            let view = cx.new(|cx| ConfigBuilder::new(path, window, cx));
            cx.new(|cx| Root::new(view, window, cx))
        });
        if let Ok(handle) = opened {
            cx.update(|cx| {
                cx.global_mut::<ConfigBuilderWindows>()
                    .open
                    .insert(key, handle.into());
            });
        }
    })
    .detach();
}

pub struct ConfigBuilder {
    pub(crate) draft: ConfigDraft,
    pub(crate) problems: Vec<Problem>,

    /// Text inputs by field id — see [`field_id`] for the encoding.
    inputs: HashMap<SharedString, Entity<InputState>>,
    /// Dropdowns, keyed the same way.
    selects: HashMap<SharedString, Entity<SelectState<Vec<DetailSelectItem>>>>,

    pub(crate) search: Entity<InputState>,
    pub(crate) yaml_editor: Entity<EditorState>,
    pub(crate) yaml_text: String,
    pub(crate) search_open: bool,
    pub(crate) yaml_open: bool,
    pub(crate) saved_at: Option<SharedString>,
    /// Load or save failures — shown in the footer rather than swallowed.
    pub(crate) notice: Option<SharedString>,

    /// Secondary-config graph is refreshed by `chain` at most every two seconds, rather than
    /// recursively parsing every linked YAML on every render.
    pub(crate) chain_nodes: Vec<SecondaryConfigNode>,
    pub(crate) collapsed_chain: HashSet<PathBuf>,
    pub(crate) last_chain_scan: Option<Instant>,

    _subscriptions: Vec<Subscription>,
}

/// Field id for a scalar setting, one list item, or one cell of a row.
///
/// `rollback_dir` · `shutdown#0` · `csv_value_templates#0#k` · `media_types#1#l#2`.
/// Setting keys are `[a-z0-9_]` upstream, so `#` cannot collide with one.
pub(crate) fn field_id(parts: &[&str]) -> SharedString {
    parts.join("#").into()
}

/// Open a native file/folder picker and copy the selected path into a builder input.
/// Kept here so the config builder owns all of its window and dialog behavior.
pub(crate) fn get_file<T: 'static>(
    window: &mut Window,
    cx: &mut Context<T>,
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
            _ = cx.update(|window, cx| {
                input.update(cx, |state, cx| {
                    state.set_value(path.to_string_lossy().to_string(), window, cx);
                });
            });
        }
    })
    .detach();
}

impl ConfigBuilder {
    pub fn new(path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut notice = None;
        let draft = match &path {
            Some(p) => ConfigDraft::load(p).unwrap_or_else(|e| {
                notice = Some(SharedString::from(format!(
                    "Couldn't open {}: {e}",
                    p.display()
                )));
                ConfigDraft::default()
            }),
            None => ConfigDraft::default(),
        };

        let search = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Search settings to add — try \"log\", \"rollback\", \"media\"")
        });
        let yaml_text = draft.to_yaml();
        let yaml_editor =
            cx.new(|cx| EditorState::new(window, cx).default_value(yaml_text.clone()));
        let _subscriptions = vec![cx.subscribe(&search, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.search_open = !this.search.read(cx).value().trim().is_empty();
                cx.notify();
            }
        })];

        let problems = validate(&draft);
        Self {
            draft,
            problems,
            inputs: HashMap::new(),
            selects: HashMap::new(),
            search,
            yaml_editor,
            yaml_text,
            search_open: false,
            // Settings → Config builder → "Show the YAML panel". Defaults on: the preview is the
            // thing that makes the editors legible to someone who knows the YAML already.
            yaml_open: AppSettings::get(cx)
                .values
                .get("builder_show_yaml")
                .map(|v| v.bool())
                .unwrap_or(true),
            saved_at: None,
            notice,
            chain_nodes: Vec::new(),
            collapsed_chain: HashSet::new(),
            last_chain_scan: None,
            _subscriptions,
        }
    }

    // --- widget plumbing -------------------------------------------------------------

    /// A text input for `id`, created and seeded on first use. Its change handler writes back
    /// into the draft, so `commit` is the only path a value takes from screen to config.
    pub(crate) fn input(
        &mut self,
        id: SharedString,
        setting_key: &str,
        seed: &str,
        placeholder: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        if let Some(existing) = self.inputs.get(&id) {
            return existing.clone();
        }
        let seed = seed.to_string();
        let placeholder = placeholder.to_string();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(placeholder)
                .default_value(seed)
        });
        let key = setting_key.to_string();
        self._subscriptions.push(
            cx.subscribe(&input, move |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.commit(&key, cx);
                }
            }),
        );
        self.inputs.insert(id, input.clone());
        input
    }

    /// A dropdown for `id`, seeded with `choices` and the current selection.
    pub(crate) fn select(
        &mut self,
        id: SharedString,
        setting_key: &str,
        choices: Vec<DetailSelectItem>,
        selected: Option<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<SelectState<Vec<DetailSelectItem>>> {
        if let Some(existing) = self.selects.get(&id) {
            return existing.clone();
        }
        let state = cx.new(|cx| SelectState::new(choices, None, window, cx));
        if let Some(value) = selected {
            state.update(cx, |s, cx| s.set_selected_value(&value, window, cx));
        }
        let key = setting_key.to_string();
        self._subscriptions.push(cx.subscribe(
            &state,
            move |this, _, _: &SelectEvent<Vec<DetailSelectItem>>, cx| {
                this.commit(&key, cx);
            },
        ));
        self.selects.insert(id, state.clone());
        state
    }

    pub(crate) fn input_value(&self, id: &SharedString, cx: &App) -> String {
        self.inputs
            .get(id)
            .map(|i| i.read(cx).value().to_string())
            .unwrap_or_default()
    }

    pub(crate) fn select_value(&self, id: &SharedString, cx: &App) -> Option<SharedString> {
        self.selects
            .get(id)
            .and_then(|s| s.read(cx).selected_value().cloned())
    }

    /// Forget every widget belonging to `key`, so the next render rebuilds them from the draft.
    /// Needed whenever rows are added or removed and the surviving rows shift position.
    pub(crate) fn forget_widgets(&mut self, key: &str) {
        let prefix = format!("{key}#");
        self.inputs
            .retain(|id, _| id.as_ref() != key && !id.starts_with(&prefix));
        self.selects
            .retain(|id, _| id.as_ref() != key && !id.starts_with(&prefix));
    }

    // --- draft mutation --------------------------------------------------------------

    /// Rebuild `key`'s value from its widgets and revalidate. The one place screen state
    /// becomes config state.
    pub(crate) fn commit(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(def) = catalog::find(key) else {
            return;
        };
        if let Some(value) = self.read_widgets(def, cx) {
            self.draft.values.insert(key.to_string(), value);
        }
        self.revalidate(cx);
    }

    pub(crate) fn revalidate(&mut self, cx: &mut Context<Self>) {
        self.problems = validate(&self.draft);
        cx.notify();
    }

    pub(crate) fn add_setting(&mut self, def: &SettingDef, cx: &mut Context<Self>) {
        if self.draft.values.contains_key(&def.key) {
            return;
        }
        // Seed with Workbench's own default so the row shows something real; a setting with no
        // upstream default starts empty for its shape.
        let seed = if def.default.is_null() {
            editors::empty_value(def.shape)
        } else {
            def.default.clone()
        };
        self.draft.values.insert(def.key.clone(), seed);
        self.forget_widgets(&def.key);
        self.search_open = false;
        self.revalidate(cx);
    }

    pub(crate) fn remove_setting(&mut self, key: &str, cx: &mut Context<Self>) {
        self.draft.values.shift_remove(key);
        self.forget_widgets(key);
        self.revalidate(cx);
    }

    pub(crate) fn problems_for<'a>(&'a self, key: &str) -> impl Iterator<Item = &'a Problem> {
        let key = key.to_string();
        self.problems
            .iter()
            .filter(move |p| p.key.as_deref() == Some(key.as_str()))
    }

    pub(crate) fn count(&self, severity: Severity) -> usize {
        self.problems
            .iter()
            .filter(|p| p.severity == severity)
            .count()
    }

    // --- save ------------------------------------------------------------------------

    fn target_path(&self, cx: &App) -> Option<PathBuf> {
        if let Some(path) = &self.draft.path {
            return Some(path.clone());
        }
        // New configs land in the folder Settings names, falling back to the workbench install.
        let dir = AppSettings::get(cx)
            .values
            .get("config_library_dir")
            .map(|v| v.text().to_string())
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                AppSettings::get(cx)
                    .values
                    .get("workbench_path")
                    .map(|v| format!("{}/g/config", v.text()))
            })?;
        let stem = if self.draft.label.trim().is_empty() {
            "new_config".to_string()
        } else {
            self.draft.label.trim().replace(' ', "_").to_lowercase()
        };
        Some(PathBuf::from(dir).join(format!("{stem}.yml")))
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.count(Severity::Error) > 0 {
            self.notice = Some("Fix the problems below before saving.".into());
            cx.notify();
            return;
        }
        let Some(path) = self.target_path(cx) else {
            self.notice =
                Some("Set a config folder in Settings before saving a new config.".into());
            cx.notify();
            return;
        };
        if let Err(e) = self.draft.save(&path) {
            self.notice = Some(format!("Couldn't save {}: {e}", path.display()).into());
            cx.notify();
            return;
        }

        let label = if self.draft.label.trim().is_empty() {
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Untitled".into())
        } else {
            self.draft.label.clone()
        };
        let task = self
            .draft
            .values
            .get("task")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let file_path: SharedString = path.to_string_lossy().to_string().into();

        // Add or update the library entry, matched on the file it points at.
        AppSettings::update(cx, |s| {
            let entry = TaskConfig {
                label: label.clone().into(),
                task_name: task.clone().into(),
                file_path: file_path.clone(),
            };
            match s.task_configs.iter_mut().find(|c| c.file_path == file_path) {
                Some(existing) => *existing = entry,
                None => s.task_configs.push(entry),
            }
        });

        let was_new = self.draft.path.is_none();
        self.draft.path = Some(path.clone());
        if was_new {
            let windows = cx.global_mut::<ConfigBuilderWindows>();
            windows.open.remove(&None);
            windows.open.insert(Some(path), window.window_handle());
        }
        self.draft.label = label;
        self.saved_at = Some("Saved".into());
        self.notice = None;
        cx.notify();
    }
}

impl Render for ConfigBuilder {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let errors = self.count(Severity::Error);
        let warnings = self.count(Severity::Warn);
        let title = if self.draft.label.trim().is_empty() {
            "New config".to_string()
        } else {
            self.draft.label.clone()
        };

        v_flex()
            .size_full()
            .child(
                TitleBar::new().child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(Label::new("Config Builder").font_semibold())
                        .child(
                            Label::new(title.clone())
                                .text_sm()
                                .text_color(cx.theme().muted_foreground),
                        )
                        .children(self.saved_at.clone().map(|s| {
                            Label::new(s)
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                        })),
                ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .w_full()
                    .overflow_hidden()
                    .child(
                        v_flex()
                            .flex_1()
                            .h_full()
                            .p_4()
                            .gap_3()
                            .overflow_y_scrollbar()
                            .child(self.render_locked_band(cx))
                            .child(self.render_search(window, cx))
                            .children(self.render_settings(window, cx))
                            .child(self.render_chain(window, cx)),
                    )
                    .children(self.yaml_open.then(|| self.render_yaml_panel(window, cx))),
            )
            .child(self.render_footer(errors, warnings, cx))
    }
}

impl ConfigBuilder {
    /// The three settings the app writes at run time, so nobody thinks they are missing.
    fn render_locked_band(&self, cx: &App) -> impl IntoElement {
        h_flex()
            .w_full()
            .gap_2()
            .px_2()
            .py_1()
            .items_center()
            .flex_wrap()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().colors.border)
            .bg(cx.theme().colors.secondary)
            .child(
                Label::new("At run time")
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground),
            )
            .children(catalog::locked().enumerate().flat_map(|(index, def)| {
                let separator = (index > 0).then(|| {
                    Label::new("·")
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                });
                separator
                    .into_iter()
                    .chain(std::iter::once(Label::new(def.key.clone()).text_xs()))
            }))
            .child(
                Label::new("filled by the app")
                    .text_xs()
                    .text_color(cx.theme().muted_foreground),
            )
    }

    fn render_footer(
        &mut self,
        errors: usize,
        warnings: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let summary = match (errors, warnings) {
            (0, 0) => "No problems".to_string(),
            (0, w) => format!("{w} thing{} to know", plural(w)),
            (e, 0) => format!("{e} problem{} to fix", plural(e)),
            (e, w) => format!(
                "{e} problem{} to fix · {w} thing{} to know",
                plural(e),
                plural(w)
            ),
        };
        let color = if errors > 0 {
            cx.theme().colors.danger
        } else {
            cx.theme().muted_foreground
        };

        h_flex()
            .w_full()
            .p_2()
            .gap_2()
            .items_center()
            .border_t_1()
            .border_color(cx.theme().colors.border)
            .child(Label::new(summary).text_sm().text_color(color))
            .children(
                self.notice
                    .clone()
                    .map(|n| Label::new(n).text_sm().text_color(cx.theme().colors.danger)),
            )
            .child(div().flex_1())
            .child(
                app_button("toggle-yaml")
                    .ghost()
                    .small()
                    .label(if self.yaml_open {
                        "Hide YAML"
                    } else {
                        "Show YAML"
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.yaml_open = !this.yaml_open;
                        this.resize_for_yaml(window);
                        cx.notify();
                    })),
            )
            .child(
                app_button("discard-draft")
                    .outline()
                    .small()
                    .label("Discard draft")
                    .disabled(self.draft.values.is_empty())
                    .on_click(cx.listener(|this, _, _, cx| {
                        let keys: Vec<String> = this.draft.values.keys().cloned().collect();
                        for key in keys {
                            this.forget_widgets(&key);
                        }
                        this.draft.values.clear();
                        this.saved_at = None;
                        this.revalidate(cx);
                    })),
            )
            .child(
                app_button("save-config")
                    .primary()
                    .small()
                    .label("Save to library")
                    .icon(IconName::Check)
                    .disabled(errors > 0)
                    .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
            )
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

impl ConfigBuilder {
    /// Grow or shrink the window by the panel's width when the panel is toggled, keeping the
    /// editor column the size it was. Clamped to the display so a wide window on a small screen
    /// does not push its own controls off the edge.
    fn resize_for_yaml(&self, window: &mut Window) {
        let current = window.bounds().size;
        let target = if self.yaml_open {
            current.width + YAML_PANEL_WIDTH
        } else {
            current.width - YAML_PANEL_WIDTH
        };
        window.resize(size(target.max(px(520.)), current.height));
    }
}
