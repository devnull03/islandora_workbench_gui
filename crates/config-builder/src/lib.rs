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
use std::sync::LazyLock;
use std::time::Instant;

use gpui::*;
use gpui_component::{
    ActiveTheme, Disableable, IconName, Root, Sizable, StyledExt, TitleBar,
    button::ButtonVariants,
    h_flex,
    input::EditorState,
    input::{Input, InputEvent, InputState},
    label::Label,
    scroll::ScrollableElement,
    select::{SelectEvent, SelectState},
    v_flex,
};
use regex::Regex;
use serde_yaml::Value;
use settings::{AppSettings, TaskConfig};
use workbench_integration::config::{
    ConfigDraft,
    catalog::{self, SettingDef},
    chain::SecondaryConfigNode,
    validate::{Problem, Severity, validate},
};

use ui::AppFont as _;
use ui::tokens::{GAP_2XL, GAP_LG, GAP_MD, GAP_SM, GAP_XL, GAP_XS, MIN_WINDOW_W, PAD_PAGE};
use ui::{
    APP_CONTROL_SIZE, DetailSelectItem, LockedBand, ProblemSummary, app_button, ghost_button,
};

actions!(config_builder, [OpenConfigBuilder]);

/// What a `Shape::Integer` field accepts while it is being typed into. Empty is allowed on
/// purpose (§04: empty means "use the default", not zero), and so is a leading minus, because
/// a few Workbench settings are offsets.
///
/// Compiled once — a `Regex::new` per row would be one compilation per setting per render.
static INTEGER_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^-?\d*$").expect("a literal that is checked by the tests below"));

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
pub(crate) const YAML_PANEL_WIDTH: Pixels = px(400.);
const FALLBACK_EDITOR_WIDTH: Pixels = px(600.);
const FALLBACK_EDITOR_HEIGHT: Pixels = px(800.);

/// Open the builder on `path`, or on a blank draft when `path` is `None`.
pub fn open_config_builder(path: Option<PathBuf>, cx: &mut App) {
    open_config_builder_with_parent(path, None, cx);
}

/// Open a new builder draft that will be linked under `parent` after its first successful save.
pub fn open_child_config_builder(parent: PathBuf, cx: &mut App) {
    open_config_builder_with_parent(None, Some(parent), cx);
}

fn open_config_builder_with_parent(path: Option<PathBuf>, parent: Option<PathBuf>, cx: &mut App) {
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
                && px(bounds.width) >= MIN_WINDOW_W
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
        window_min_size: Some(Size::new(MIN_WINDOW_W, px(420.0))),
        ..Default::default()
    };

    cx.spawn(async move |cx| {
        let key = path.clone();
        let opened = cx.open_window(options, |window, cx| {
            let view = cx.new(|cx| ConfigBuilder::new(path, parent, window, cx));
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
    /// Existing config that owns this draft when it was opened via “Add under”.
    pub(crate) parent_path: Option<PathBuf>,
    pub(crate) problems: Vec<Problem>,

    /// Text inputs by field id — see [`field_id`] for the encoding.
    inputs: HashMap<SharedString, Entity<InputState>>,
    /// Dropdowns, keyed the same way.
    selects: HashMap<SharedString, Entity<SelectState<Vec<DetailSelectItem>>>>,

    /// What the config is for, in the author's words. Not a Workbench setting — it lives in
    /// the config library beside the label, because the YAML schema has nowhere to put it.
    pub(crate) description: String,

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

fn link_saved_child(parent: &std::path::Path, child: &std::path::Path) -> Result<(), String> {
    let mut draft = ConfigDraft::load(parent).map_err(|error| error.to_string())?;
    let reference = child
        .strip_prefix(parent.parent().unwrap_or_else(|| std::path::Path::new("")))
        .unwrap_or(child)
        .to_path_buf();
    let existing = draft.secondary_tasks();
    if !existing.iter().any(|item| item == &reference) {
        let mut tasks = existing;
        tasks.push(reference);
        draft.set_secondary_tasks(&tasks);
        draft.save(parent).map_err(|error| error.to_string())?;
    }
    Ok(())
}

impl ConfigBuilder {
    pub fn new(
        path: Option<PathBuf>,
        parent_path: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
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

        // The library is the only place the description exists, so a draft opened from a file
        // has to go and find it. A file the library has never seen simply has none.
        let description = path
            .as_ref()
            .and_then(|p| {
                let file_path = p.to_string_lossy().to_string();
                AppSettings::get(cx)
                    .task_configs
                    .iter()
                    .find(|c| c.file_path == file_path)
                    .map(|c| c.description.to_string())
            })
            .unwrap_or_default();

        let problems = validate(&draft);
        Self {
            draft,
            description,
            parent_path,
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

    /// The config's name, edited in place in the header (`1a`).
    ///
    /// Cached under a reserved field id like every other input. `#` cannot appear in a Workbench
    /// setting key, so `#label` cannot collide with one.
    fn title_input(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<InputState> {
        let id: SharedString = "#label".into();
        if let Some(existing) = self.inputs.get(&id) {
            return existing.clone();
        }
        let seed = self.draft.label.clone();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Untitled config")
                .default_value(seed)
        });
        self._subscriptions
            .push(cx.subscribe(&input, |this, state, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.draft.label = state.read(cx).value().to_string();
                    cx.notify();
                }
            }));
        self.inputs.insert(id, input.clone());
        input
    }

    /// The line under the name: what this config is for.
    fn description_input(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        let id: SharedString = "#description".into();
        if let Some(existing) = self.inputs.get(&id) {
            return existing.clone();
        }
        let seed = self.description.clone();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Describe what this config is for — your future self will thank you")
                .default_value(seed)
        });
        self._subscriptions
            .push(cx.subscribe(&input, |this, state, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.description = state.read(cx).value().to_string();
                }
            }));
        self.inputs.insert(id, input.clone());
        input
    }

    /// The name to show for this draft: what the user typed, else the file's stem, else a
    /// placeholder. Never an empty string — the title bar would collapse around it.
    pub(crate) fn display_label(&self) -> String {
        if !self.draft.label.trim().is_empty() {
            return self.draft.label.clone();
        }
        match &self.draft.path {
            Some(path) => stem_of(path),
            None => "New config".to_string(),
        }
    }
    /// The add slot of a chip list.
    ///
    /// Unlike every other input here it does **not** commit on change: a per-keystroke commit
    /// would append one chip per character typed. It commits on Enter and on blur (§05) and
    /// clears itself, so the slot is always empty and ready for the next value.
    pub(crate) fn chip_input(
        &mut self,
        id: SharedString,
        setting_key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        if let Some(existing) = self.inputs.get(&id) {
            return existing.clone();
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("+ Add"));
        let key = setting_key.to_string();
        self._subscriptions.push(cx.subscribe_in(
            &input,
            window,
            move |this, state, event: &InputEvent, window, cx| {
                if !matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                    return;
                }
                let text = state.read(cx).value().to_string();
                if text.trim().is_empty() {
                    return;
                }
                this.push_chip(&key, &text, cx);
                state.update(cx, |state, cx| state.set_value("", window, cx));
            },
        ));
        self.inputs.insert(id, input.clone());
        input
    }
    /// A numeric input for `id`. Same cache and same change handler as [`Self::input`]; the
    /// difference is the state, which carries a digits-only pattern and a floor of zero.
    ///
    /// Those two make `NumberInput` step the value internally, so there is no `Step` event to
    /// subscribe to and no second copy of the number to keep in agreement with the draft. §04:
    /// an empty field is legal and means "use the default" — it is not zero, which is why the
    /// pattern permits an empty string.
    pub(crate) fn number_input(
        &mut self,
        id: SharedString,
        setting_key: &str,
        seed: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        if let Some(existing) = self.inputs.get(&id) {
            return existing.clone();
        }
        let seed = seed.to_string();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .pattern(INTEGER_PATTERN.clone())
                .default_value(seed)
                .min(0.)
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
        let description = self.description.clone();

        // Add or update the library entry, matched on the file it points at.
        AppSettings::update(cx, |s| {
            let entry = TaskConfig {
                label: label.clone().into(),
                task_name: task.clone().into(),
                file_path: file_path.clone(),
                description: description.clone().into(),
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
            windows
                .open
                .insert(Some(path.clone()), window.window_handle());
        }
        let mut link_notice = None;
        if was_new
            && let Some(parent) = self.parent_path.clone()
            && let Err(error) = link_saved_child(&parent, &path)
        {
            link_notice = Some(format!("Saved, but couldn't link child: {error}"));
        }
        self.draft.label = label;
        self.saved_at = Some("Saved".into());
        self.notice = link_notice.map(Into::into);
        cx.notify();
    }
}

impl Render for ConfigBuilder {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let errors = self.count(Severity::Error);
        let warnings = self.count(Severity::Warn);

        v_flex()
            .size_full()
            .app_font(cx)
            .child(self.render_title_bar(cx))
            .child(
                h_flex()
                    .flex_1()
                    .w_full()
                    .overflow_hidden()
                    .child(
                        v_flex()
                            .flex_1()
                            // A flex item's basis is its content, so without a zero minimum a
                            // long key or a wide results row makes this column — and with it
                            // the window — grow. Everything inside must fit the column, never
                            // the other way round.
                            .min_w(px(0.))
                            .h_full()
                            .p(PAD_PAGE)
                            .gap(GAP_2XL)
                            .overflow_y_scrollbar()
                            .child(self.render_header(window, cx))
                            .child(self.render_locked_band(cx))
                            .child(self.render_search(window, cx))
                            .child(v_flex().w_full().children(self.render_settings(window, cx)))
                            .child(self.render_chain(window, cx)),
                    )
                    .children(self.yaml_open.then(|| self.render_yaml_panel(window, cx))),
            )
            .child(self.render_footer(errors, warnings, cx))
    }
}

impl ConfigBuilder {
    /// Title, then the page crumb, then — pushed to the right, immediately before the window
    /// controls — whatever transient thing the window has to say (§02).
    ///
    /// A child window says so in its crumb rather than in a separate line: `Main batch items /
    /// Child pages` is `1d`'s breadcrumb, and it is the only place the relationship is stated
    /// once the child has been floated away from its parent.
    fn render_title_bar(&self, cx: &Context<Self>) -> impl IntoElement {
        let muted = cx.theme().colors.muted_foreground;
        let title = self.display_label();

        TitleBar::new()
            .child(
                h_flex()
                    .flex_1()
                    .gap(GAP_MD)
                    .items_center()
                    .child(Label::new("Config Builder").text_xs())
                    .children(self.parent_path.as_ref().map(|parent| {
                        h_flex()
                            .gap(GAP_SM)
                            .items_center()
                            .child(Label::new(stem_of(parent)).text_xs().text_color(muted))
                            .child(Label::new("/").text_xs().text_color(muted))
                    }))
                    .child(Label::new(title).text_xs().text_color(muted)),
            )
            .child(
                h_flex().justify_end().items_center().pr_4().children(
                    self.saved_at
                        .clone()
                        .map(|saved| Label::new(saved).text_xs().text_color(muted)),
                ),
            )
    }

    /// The config's own name and purpose, editable in place (`1a`).
    ///
    /// The description has nowhere to live in Workbench's schema, so it is stored beside the
    /// label in the config library — see `settings::TaskConfig`. Both are borderless inputs
    /// rather than a field with a label: a title that looks like a form field reads as one more
    /// setting, and this is the name of the thing the settings belong to.
    fn render_header(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = self.title_input(window, cx);
        let description = self.description_input(window, cx);

        v_flex()
            .w_full()
            .gap(GAP_XS)
            .child(
                Input::new(&title)
                    .appearance(false)
                    .with_size(APP_CONTROL_SIZE)
                    .w_full()
                    .text_lg()
                    .font_semibold(),
            )
            .child(
                Input::new(&description)
                    .appearance(false)
                    .with_size(APP_CONTROL_SIZE)
                    .w_full()
                    .text_color(cx.theme().colors.muted_foreground),
            )
    }

    /// The three settings the app writes at run time, so nobody thinks they are missing (§08).
    fn render_locked_band(&self, _cx: &App) -> impl IntoElement {
        let mut band = LockedBand::new(
            "Supplied by the app at run time",
            "you don't set these here",
        );
        for def in catalog::locked() {
            band = band.entry(def.key.clone(), locked_source(&def.key));
        }
        band
    }

    fn render_footer(
        &mut self,
        errors: usize,
        warnings: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        h_flex()
            .w_full()
            .px(GAP_XL)
            .py(GAP_LG)
            .gap(GAP_MD)
            .items_center()
            .border_t_1()
            .border_color(cx.theme().colors.border)
            .bg(cx.theme().colors.title_bar)
            .child(ProblemSummary::new(errors, warnings))
            .children(
                self.notice
                    .clone()
                    .map(|n| Label::new(n).text_sm().text_color(cx.theme().colors.danger)),
            )
            .child(div().flex_1())
            .child(
                ghost_button("toggle-yaml")
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
                ghost_button("discard-draft")
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
                    .label("Save to library")
                    .icon(IconName::Check)
                    // §06: disabled by errors only. A warning is something to know, not a
                    // veto — blocking on one teaches people to write configs that say less.
                    .disabled(errors > 0)
                    .tooltip(if errors > 0 {
                        "Fix the problems above first"
                    } else {
                        "Write the YAML and add it to the config library"
                    })
                    .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
            )
    }
}

/// Where a locked setting's value comes from, in the words `1a` uses.
///
/// A `match` rather than catalogue data: three keys, and the answer is about how *this app* runs
/// Workbench, which the generated catalogue has no business knowing.
fn locked_source(key: &str) -> &'static str {
    match key {
        "host" => "Ingest server",
        "credentials_file_path" => "App settings",
        "input_csv" => "Processed sheet",
        _ => "The app",
    }
}

fn stem_of(path: &std::path::Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
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
        window.resize(size(target.max(MIN_WINDOW_W), current.height));
    }
}

#[cfg(test)]
mod tests {
    // Not `use super::*`: this module glob-imports `gpui`, whose own `test` macro would
    // shadow the one this needs.
    use super::INTEGER_PATTERN;

    /// The pattern is what stands between a typo and a YAML integer field holding a word.
    /// Empty has to pass — §04 says an empty numeric field means "use the default", not zero —
    /// and a lone minus has to pass too, or you cannot type a negative number at all.
    #[test]
    fn the_integer_pattern_accepts_a_number_being_typed_and_nothing_else() {
        for ok in ["", "0", "3", "255", "-", "-1"] {
            assert!(INTEGER_PATTERN.is_match(ok), "{ok:?} should be accepted");
        }
        for bad in ["abc", "1.5", "1e3", "1 ", "--1", "1-"] {
            assert!(!INTEGER_PATTERN.is_match(bad), "{bad:?} should be rejected");
        }
    }
}
