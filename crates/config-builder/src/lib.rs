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
mod dock;
mod editors;
mod search;
mod yaml_panel;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Instant;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, Disableable, IconName, Root, Selectable as _, Sizable, StyledExt, TitleBar,
    button::ButtonVariants,
    h_flex,
    input::EditorState,
    input::{Input, InputEvent, InputState, TextareaState},
    label::Label,
    scroll::ScrollableElement,
    select::{SearchableVec, SelectEvent, SelectState},
    status_bar::StatusBar as StatusBarElement,
    v_flex,
};
use regex::Regex;
use serde_yaml::Value;
use settings::{AppSettings, TaskConfig};
use workbench_integration::config::{
    ConfigDraft,
    catalog::{self, SettingDef},
    chain::SecondaryConfigNode,
    validate::{Problem, Severity, ValidationContext, validate},
};

use ui::AppFont as _;
use ui::tokens::{GAP_2XL, GAP_MD, GAP_SM, GAP_XS, PAD_PAGE};
use ui::{
    APP_CONTROL_SIZE, DetailSelectItem, LockedBand, ProblemSummary, ghost_button, status_bar_button,
};

actions!(config_builder, [OpenConfigBuilder]);

pub use dock::{
    BUILDER_OPEN_DOCKED_KEY, BuilderDock, BuilderPane, DOCK_DEFAULT_W, ToggleBuilderDock,
    clamp_dock_width, install_builder_dock,
};
pub(crate) use dock::{BuilderHost, BuilderSnapshot};

/// What a `Shape::Integer` field accepts while it is being typed into. Empty is allowed on
/// purpose (§04: empty means "use the default", not zero), and so is a leading minus, because
/// a few Workbench settings are offsets.
///
/// Compiled once — a `Regex::new` per row would be one compilation per setting per render.
static INTEGER_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^-?\d*$").expect("a literal that is checked by the tests below"));

/// What a `Shape::Decimal` field accepts while typed into: the integer pattern plus one point.
static DECIMAL_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^-?\d*\.?\d*$").expect("a literal that is checked by the tests below")
});

/// Settings → Config builder switches (default on) that gate validation.
pub const BUILDER_CHECK_PATHS_KEY: &str = "builder_check_paths";
pub const BUILDER_WARN_CLASHES_KEY: &str = "builder_warn_clashes";

/// Component Spec §05: from this many options a dropdown gets a search field.
const SEARCHABLE_SELECT_MIN: usize = 12;

/// Open windows, keyed by the config they are editing (`None` for an unsaved new config), so a
/// second Edit on the same file focuses the window that already has it rather than opening a
/// rival copy with a divergent draft.
#[derive(Default)]
pub struct ConfigBuilderWindows {
    open: Vec<OpenBuilder>,
}

impl Global for ConfigBuilderWindows {}

/// One registered builder window, and the view in it — the view is how a chain edit made in
/// another window reaches this window's draft instead of the file under it (plan 1.9).
#[derive(Clone)]
struct OpenBuilder {
    /// The file, as it was opened and as the file system names it; `None` for an unsaved draft.
    /// Each unsaved draft is its own window.
    path: Option<(PathBuf, PathBuf)>,
    window: AnyWindowHandle,
    view: WeakEntity<ConfigBuilder>,
}

impl ConfigBuilderWindows {
    /// Every config with a live builder window, by both the path it was opened with and the
    /// one the file system resolves it to. Plain paths, so a per-frame lookup touches no disk.
    pub(crate) fn open_paths(&self) -> HashSet<PathBuf> {
        self.live()
            .filter_map(|open| open.path.clone())
            .flat_map(|(opened, identity)| [opened, identity])
            .collect()
    }

    /// The live builder editing `path`, if one is open. Matched by file identity, not by the
    /// spelling the window was opened with.
    fn view_for(&self, path: &std::path::Path) -> Option<Entity<ConfigBuilder>> {
        self.find(path).and_then(|open| open.view.upgrade())
    }

    /// The unsaved drafts and file drafts open in builders other than `except`.
    pub(crate) fn drafts(&self, except: EntityId, cx: &App) -> Vec<ConfigDraft> {
        self.live()
            .filter_map(|open| open.view.upgrade())
            .filter(|view| view.entity_id() != except)
            .map(|view| view.read(cx).draft.clone())
            .filter(|draft| draft.path.is_some())
            .collect()
    }

    /// How many open builders hold unsaved edits, and a window showing one of them — for Quit,
    /// which closes every window without asking each one.
    pub fn unsaved(&self, cx: &App) -> (usize, Option<AnyWindowHandle>) {
        let dirty: Vec<&OpenBuilder> = self
            .live()
            .filter(|open| {
                open.view
                    .upgrade()
                    .is_some_and(|view| view.read(cx).chrome.is_dirty())
            })
            .collect();
        (dirty.len(), dirty.first().map(|open| open.window))
    }

    /// Mark every open builder saved-or-discarded, so closing their windows does not ask again.
    pub fn forget_unsaved(cx: &mut App) {
        let views: Vec<_> = cx
            .global::<Self>()
            .live()
            .filter_map(|open| open.view.upgrade())
            .collect();
        for view in views {
            view.update(cx, |builder, _| {
                builder.chrome.save = BuilderTitleSaveState::Clean
            });
        }
    }

    /// The window `view` is registered in.
    pub(crate) fn window_of(&self, view: &WeakEntity<ConfigBuilder>) -> Option<AnyWindowHandle> {
        self.live()
            .find(|open| &open.view == view)
            .map(|open| open.window)
    }

    fn live(&self) -> impl Iterator<Item = &OpenBuilder> {
        self.open
            .iter()
            .filter(|open| open.view.upgrade().is_some())
    }

    fn find(&self, path: &std::path::Path) -> Option<&OpenBuilder> {
        let identity = workbench_integration::config::chain::identity(path);
        self.live().find(|open| {
            open.path
                .as_ref()
                .is_some_and(|(opened, known)| *known == identity || opened == path)
        })
    }

    /// Record `view`'s window under `path`, replacing whatever entry the view had.
    fn register(
        &mut self,
        path: Option<PathBuf>,
        window: AnyWindowHandle,
        view: WeakEntity<ConfigBuilder>,
    ) {
        self.open
            .retain(|open| open.view.upgrade().is_some() && open.view != view);
        let path = path.map(|path| {
            let identity = workbench_integration::config::chain::identity(&path);
            (path, identity)
        });
        self.open.push(OpenBuilder { path, window, view });
    }
}

/// Width of the read-only YAML preview, and of the editor column beside it. The window is sized
/// as the sum, and grows and shrinks by the panel's width when it is shown or hidden — the panel
/// appearing must not squeeze the editors it exists to explain.
pub(crate) const YAML_PANEL_WIDTH: Pixels = px(400.);
/// Component Spec §02: the builder opens 800×760 and goes no smaller than 640×520.
const DEFAULT_EDITOR_WIDTH: Pixels = px(800.);
const DEFAULT_EDITOR_HEIGHT: Pixels = px(760.);
const MIN_BUILDER_WIDTH: Pixels = px(640.);
const MIN_BUILDER_HEIGHT: Pixels = px(520.);
/// Settings key holding the editor side's last size, `"<width>x<height>"`.
const BUILDER_SIZE_KEY: &str = "builder_editor_size";
/// How far each chain level's window is offset from its parent's.
const CASCADE_STEP: Pixels = px(24.);

/// Open the builder on `path`, or on a blank draft when `path` is `None`.
pub fn open_config_builder(path: Option<PathBuf>, cx: &mut App) {
    open_config_builder_with_ancestors(path, Vec::new(), cx);
}

/// Open a new builder draft that will be linked under `parent` after its first successful save.
pub fn open_child_config_builder(parent: PathBuf, cx: &mut App) {
    open_config_builder_with_ancestors(None, vec![parent], cx);
}

pub(crate) fn open_config_builder_in_chain(
    path: Option<PathBuf>,
    ancestors: Vec<PathBuf>,
    cx: &mut App,
) {
    open_config_builder_with_ancestors(path, ancestors, cx);
}

fn open_config_builder_with_ancestors(
    path: Option<PathBuf>,
    ancestors: Vec<PathBuf>,
    cx: &mut App,
) {
    // Deferred: Edit and New are clicked inside the main window's update, and that window
    // cannot be updated again from within it — so neither focusing it (a docked builder) nor
    // docking into it would work.
    cx.defer(move |cx| open_now(path, ancestors, cx));
}

fn open_now(path: Option<PathBuf>, ancestors: Vec<PathBuf>, cx: &mut App) {
    if let Some(handle) = path.as_deref().and_then(|path| {
        cx.global::<ConfigBuilderWindows>()
            .find(path)
            .map(|open| open.window)
    }) && handle
        .update(cx, |_, window, _| window.activate_window())
        .is_ok()
    {
        return;
    }

    // A root config opens in the main window's dock when Settings says so and the dock is
    // free; a chain child always gets its own window (plan 5.7).
    if ancestors.is_empty() && dock::open_docked(path.clone(), cx) {
        return;
    }
    let depth = ancestors.len();
    let key = path.clone();
    let with_yaml = AppSettings::bool_or("builder_show_yaml", true, cx);
    open_builder_window(
        key,
        depth,
        with_yaml,
        move |window, cx| ConfigBuilder::new(path, ancestors, window, cx),
        cx,
    )
    .detach();
}

/// Open a builder window around the view `build` makes, register it under `key`, and hand the
/// view back once the window exists.
pub(crate) fn open_builder_window(
    key: Option<PathBuf>,
    depth: usize,
    with_yaml: bool,
    build: impl FnOnce(&mut Window, &mut Context<ConfigBuilder>) -> ConfigBuilder + 'static,
    cx: &mut App,
) -> Task<Option<Entity<ConfigBuilder>>> {
    // The starting width has to agree with whether the YAML panel is showing (`with_yaml`),
    // or the window opens the wrong size and the first toggle corrects it with a visible jump.
    // The editable side opens at the size the user last left a builder at, else the spec's
    // 800×760 (§02). YAML is an additional right-hand surface, never width stolen from the
    // form the user is trying to edit.
    let editor_size = AppSettings::get(cx)
        .values
        .get(BUILDER_SIZE_KEY)
        .and_then(|v| parse_size(&v.text()))
        .unwrap_or_else(|| size(DEFAULT_EDITOR_WIDTH, DEFAULT_EDITOR_HEIGHT));
    let width = if with_yaml {
        editor_size.width + YAML_PANEL_WIDTH
    } else {
        editor_size.width
    };
    let mut bounds = Bounds::centered(None, size(width, editor_size.height), cx);
    // `2a`: a child window cascades from its parent by one step per level so the chain reads
    // as a stack; past level 3 the offsets would walk off a laptop screen, so it centres.
    if (1..=3).contains(&depth) {
        bounds.origin += point(CASCADE_STEP * depth as f32, CASCADE_STEP * depth as f32);
    }
    let options = WindowOptions {
        titlebar: Some(TitleBar::title_bar_options()),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(Size::new(MIN_BUILDER_WIDTH, MIN_BUILDER_HEIGHT)),
        ..Default::default()
    };

    cx.spawn(async move |cx| {
        let mut view = None;
        let opened = cx.open_window(options, |window, cx| {
            let builder = cx.new(|cx| build(window, cx));
            let weak = builder.downgrade();
            window.on_window_should_close(cx, move |window, cx| {
                weak.upgrade()
                    .is_none_or(|builder| may_close_builder(&builder, window, cx))
            });
            view = Some(builder.clone());
            cx.new(|cx| Root::new(builder, window, cx))
        });
        let (Ok(handle), Some(view)) = (opened, view) else {
            return None;
        };
        cx.update(|cx| {
            cx.global_mut::<ConfigBuilderWindows>()
                .register(key, handle.into(), view.downgrade());
        });
        Some(view)
    })
}

pub struct ConfigBuilder {
    /// The profile active when this builder opened: its library gets the entry on save.
    pub(crate) profile: String,
    /// Where this builder is shown: its own window, or the main window's right dock.
    pub(crate) host: BuilderHost,
    /// While a floating builder sits over the main window's right edge: when it got there, so
    /// it docks once it has been still there for a moment (`dock::window_moved`).
    pub(crate) snap: Option<Instant>,
    /// This builder's window bounds at the last bounds event, to tell a move from a resize.
    pub(crate) last_bounds: Option<Bounds<Pixels>>,
    /// The pending "still over the edge?" check; replacing it cancels the last one.
    pub(crate) snap_task: Option<Task<()>>,
    /// Focused when the builder opens, so its shortcuts work before anything is clicked.
    focus_handle: FocusHandle,
    pub(crate) draft: ConfigDraft,
    /// Existing config that owns this draft when it was opened via “Add under”.
    pub(crate) parent_path: Option<PathBuf>,
    /// Root-to-parent path used for chained-window breadcrumbs and level numbering.
    pub(crate) ancestors: Vec<PathBuf>,
    pub(crate) problems: Vec<Problem>,

    /// Text inputs by field id — see [`field_id`] for the encoding.
    inputs: HashMap<SharedString, Entity<InputState>>,
    /// Dropdowns, keyed the same way.
    selects: HashMap<SharedString, Entity<SelectState<SearchableVec<DetailSelectItem>>>>,
    /// Multi-line YAML editors (`Shape::Yaml`), keyed the same way.
    textareas: HashMap<SharedString, Entity<TextareaState>>,
    /// What a widget could not turn into a value, by setting key: YAML that does not parse, a
    /// word typed into a list of numbers. The draft keeps the last good value; this is what the
    /// row says instead, and it blocks Save.
    pub(crate) widget_errors: HashMap<String, String>,
    /// Text and URL settings whose value was a number when the config was loaded, so they are
    /// written back as numbers (see `editors::parse_text_keeping_numbers`).
    loaded_numbers: HashSet<String>,

    /// What the config is for, in the author's words. Not a Workbench setting — it lives in
    /// the config library beside the label, because the YAML schema has nowhere to put it.
    pub(crate) description: String,

    pub(crate) search: Entity<InputState>,
    /// Stable catalogue key selected in the add-setting palette. A key survives filtering and
    /// regrouping; a row index would point at a different setting after either operation.
    pub(crate) search_selected: Option<SharedString>,
    pub(crate) search_scroll: ScrollHandle,
    pub(crate) yaml_editor: Entity<EditorState>,
    pub(crate) yaml_text: String,
    /// Bumped on every change to the draft or its problems; the YAML preview and its markers
    /// are rebuilt only when it moves.
    pub(crate) draft_revision: u64,
    pub(crate) yaml_revision: Option<u64>,
    pub(crate) yaml_markers: Vec<(usize, Severity)>,
    pub(crate) search_open: bool,
    pub(crate) chrome: BuilderChromeState,

    /// Secondary-config graph is refreshed by `chain` at most every two seconds, rather than
    /// recursively parsing every linked YAML on every render.
    pub(crate) chain_nodes: Vec<SecondaryConfigNode>,
    /// Why the last chain edit (link, relink, unlink) could not be made, shown in the chain
    /// panel beside the attempt (`2a`).
    pub(crate) chain_error: Option<SharedString>,
    pub(crate) collapsed_chain: HashSet<PathBuf>,
    pub(crate) last_chain_scan: Option<Instant>,
    /// Text a widget held but could not turn into a value when the builder moved host
    /// (`dock`), by widget id: the rebuilt widget starts with it rather than the last good value.
    carried_text: HashMap<SharedString, String>,
    /// Each widget's change handler, by widget id, dropped with the widget in `forget_widgets`.
    widget_subscriptions: HashMap<SharedString, Subscription>,
    /// The chain scan in flight, if any (see `scan_chain`).
    chain_scan: Option<Task<()>>,
    /// What validation read from Settings last time, so a settings change that alters the
    /// answer (another profile, a new Workbench folder, a builder switch) revalidates.
    validation_inputs: ValidationInputs,
    /// The pending write of this window's size; replaced on every resize so a drag is saved
    /// once, when it settles.
    size_save: Option<Task<()>>,

    _subscriptions: Vec<Subscription>,
}

#[derive(Clone)]
enum BuilderTitleNavigation {
    Root {
        title: SharedString,
    },
    Chained {
        crumbs: Vec<SharedString>,
        title: SharedString,
        depth: usize,
    },
}

#[derive(Clone, Default)]
enum BuilderTitleSaveState {
    #[default]
    Clean,
    Dirty,
    /// Saved at this local time, `HH:MM`.
    Saved(SharedString),
    Error(SharedString),
}

struct BuilderTitleChromeState {
    navigation: BuilderTitleNavigation,
    save: BuilderTitleSaveState,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum BuilderYamlState {
    #[default]
    Hidden,
    Visible,
}

#[derive(Clone)]
pub(crate) enum BuilderChromeEvent {
    Edited,
    ToggleYaml,
    Saved {
        /// Local time of the save, `HH:MM`.
        at: SharedString,
        notice: Option<SharedString>,
    },
    Failed {
        title: SharedString,
        detail: SharedString,
    },
}

#[derive(Clone, Default)]
pub(crate) struct BuilderChromeState {
    save: BuilderTitleSaveState,
    yaml: BuilderYamlState,
    /// Load, save, and chain failures are shown in the status bar.
    notice: Option<SharedString>,
}

impl BuilderChromeState {
    pub(crate) fn transition(&mut self, event: BuilderChromeEvent) {
        match event {
            BuilderChromeEvent::Edited => {
                self.save = BuilderTitleSaveState::Dirty;
                self.notice = None;
            }
            BuilderChromeEvent::ToggleYaml => {
                self.yaml = match self.yaml {
                    BuilderYamlState::Hidden => BuilderYamlState::Visible,
                    BuilderYamlState::Visible => BuilderYamlState::Hidden,
                };
            }
            BuilderChromeEvent::Saved { at, notice } => {
                self.save = BuilderTitleSaveState::Saved(at);
                self.notice = notice;
            }
            BuilderChromeEvent::Failed { title, detail } => {
                self.save = BuilderTitleSaveState::Error(title);
                self.notice = Some(detail);
            }
        }
    }

    /// Possibly different from the file: edited since it was opened, saved or discarded, or
    /// a save that failed.
    pub(crate) fn is_dirty(&self) -> bool {
        matches!(
            self.save,
            BuilderTitleSaveState::Dirty | BuilderTitleSaveState::Error(_)
        )
    }

    pub(crate) fn yaml_visible(&self) -> bool {
        self.yaml == BuilderYamlState::Visible
    }
}

/// Field id for a scalar setting, one list item, or one cell of a row.
///
/// `rollback_dir` · `shutdown#0` · `csv_value_templates#0#k` · `media_types#1#l#2`.
/// Setting keys are `[a-z0-9_]` upstream, so `#` cannot collide with one.
pub(crate) fn field_id(parts: &[&str]) -> SharedString {
    parts.join("#").into()
}

/// The Workbench install directory from Settings: the base relative `secondary_tasks` entries
/// resolve against, because the app runs Workbench from there (plan 1.4, P0-4).
pub(crate) fn workbench_dir(cx: &App) -> Option<PathBuf> {
    AppSettings::get(cx)
        .values
        .get("workbench_path")
        .map(|v| v.text().trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

/// Keys whose value is a number, for `loaded_numbers`.
fn numeric_keys(draft: &ConfigDraft) -> HashSet<String> {
    draft
        .values
        .iter()
        .filter(|(_, value)| value.is_number())
        .map(|(key, _)| key.clone())
        .collect()
}

/// `"800x760"` as a size, refusing anything below the minimum (a corrupt or tiny value must not
/// open an unusable window).
fn parse_size(text: &str) -> Option<Size<Pixels>> {
    let (w, h) = text.split_once('x')?;
    let (w, h) = (w.trim().parse::<f32>().ok()?, h.trim().parse::<f32>().ok()?);
    (w.is_finite() && h.is_finite() && px(w) >= MIN_BUILDER_WIDTH && px(h) >= MIN_BUILDER_HEIGHT)
        .then(|| size(px(w), px(h)))
}

/// A YAML editor's text as a value: empty is unset, anything else must parse.
fn parse_yaml_value(text: &str) -> Result<Value, String> {
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_yaml::from_str(text).map_err(|e| e.to_string())
}

/// How long a resize must settle before the builder size is written to Settings.
const SIZE_SAVE_DELAY: std::time::Duration = std::time::Duration::from_millis(400);

/// The settings validation depends on.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ValidationInputs {
    workbench_dir: Option<PathBuf>,
    check_paths: bool,
    warn_clashes: bool,
}

impl ValidationInputs {
    fn read(cx: &App) -> Self {
        Self {
            workbench_dir: workbench_dir(cx),
            check_paths: AppSettings::bool_or(BUILDER_CHECK_PATHS_KEY, true, cx),
            warn_clashes: AppSettings::bool_or(BUILDER_WARN_CLASHES_KEY, true, cx),
        }
    }
}

/// The draft's problems plus what its widgets could not turn into values, widget errors first
/// and sorted, so the footer and the "first blocker" are the same on every render.
fn problems_for(
    draft: &ConfigDraft,
    widget_errors: &HashMap<String, String>,
    cx: &App,
) -> Vec<Problem> {
    let mut problems = validate_draft(draft, cx);
    let mut errors: Vec<_> = widget_errors.iter().collect();
    errors.sort();
    let widget_problems = errors.into_iter().map(|(key, error)| Problem {
        key: Some(key.clone()),
        severity: Severity::Error,
        message: error.clone(),
    });
    problems.splice(0..0, widget_problems);
    problems
}

/// Validate with what the builder knows about this machine.
fn validate_draft(draft: &ConfigDraft, cx: &App) -> Vec<Problem> {
    let inputs = ValidationInputs::read(cx);
    validate(
        draft,
        &ValidationContext {
            workbench_dir: inputs.workbench_dir.as_deref(),
            skip_path_checks: !inputs.check_paths,
            skip_clash_warnings: !inputs.warn_clashes,
        },
    )
}

/// Apply a `secondary_tasks` edit to a config that no builder window has open: load, edit,
/// and save only if the edit changed something.
fn edit_config_file(
    path: &std::path::Path,
    edit: impl FnOnce(&mut ConfigDraft) -> bool,
) -> Result<(), String> {
    let mut draft = ConfigDraft::load(path)
        .map_err(|error| format!("Couldn't open {}: {error}", path.display()))?;
    // Only `secondary_tasks` changes, so only its lines are rewritten: a file no window has
    // open keeps its comments.
    if edit(&mut draft) {
        draft
            .save_key(path, "secondary_tasks")
            .map_err(|error| format!("Couldn't save {}: {error}", path.display()))?;
    }
    Ok(())
}

impl ConfigBuilder {
    /// The one path a chain edit takes (plan 1.9, P0-9). `edit` changes the `secondary_tasks`
    /// of the config at `owner` and reports whether it changed anything. It is applied to:
    ///
    /// 1. this window's own draft, when `owner` is the config open here — including a
    ///    top-level node of a saved root, which used to be edited on disk behind the draft;
    /// 2. the draft of another open builder window on `owner`, which is marked edited, so its
    ///    next Save keeps the change instead of overwriting it with a stale draft;
    /// 3. otherwise the file itself.
    pub(crate) fn apply_chain_edit(
        &mut self,
        owner: &std::path::Path,
        edit: impl FnOnce(&mut ConfigDraft, Option<&std::path::Path>) -> bool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let base = workbench_dir(cx);
        let base = base.as_deref();
        self.last_chain_scan = None;
        if self
            .draft
            .path
            .as_deref()
            .is_some_and(|path| workbench_integration::config::chain::same_path(path, owner))
        {
            if edit(&mut self.draft, base) {
                self.forget_widgets("secondary_tasks");
                self.revalidate(cx);
            }
            return Ok(());
        }
        if let Some(view) = cx.global::<ConfigBuilderWindows>().view_for(owner) {
            view.update(cx, |builder, cx| {
                if edit(&mut builder.draft, base) {
                    builder.forget_widgets("secondary_tasks");
                    builder.last_chain_scan = None;
                    builder.revalidate(cx);
                }
            });
            return Ok(());
        }
        let result = edit_config_file(owner, |draft| edit(draft, base));
        ConfigFilesChanged::bump(cx);
        result
    }
}

/// Bumped whenever the builder writes a config file, so views that summarise configs (the main
/// window's step 2) know to read them again instead of checking the disk every frame.
#[derive(Default)]
pub struct ConfigFilesChanged(pub u64);

impl Global for ConfigFilesChanged {}

impl ConfigFilesChanged {
    pub fn generation(cx: &App) -> u64 {
        cx.try_global::<Self>().map_or(0, |changed| changed.0)
    }

    /// Something wrote a config file.
    pub fn bump(cx: &mut App) {
        cx.default_global::<Self>().0 += 1;
    }
}

/// A file name for a label: lowercase `[a-z0-9_-]`, runs of anything else collapsed to one
/// `_`. The label is free text in the header, so it can hold `../`, `:`, or nothing at all;
/// none of that may reach the file system (plan 1.8, P0-8).
fn slug(label: &str) -> String {
    let mut slug = String::new();
    for c in label.trim().chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            slug.push(c);
        } else if !slug.ends_with('_') {
            slug.push('_');
        }
    }
    let slug = slug.trim_matches(['_', '-']);
    // Windows reserves these names in every folder (`con.yml` is the console), and a library
    // folder may be shared with a Windows machine whatever this one runs.
    let reserved = matches!(slug, "con" | "prn" | "aux" | "nul")
        || (slug.len() == 4
            && (slug.starts_with("com") || slug.starts_with("lpt"))
            && slug.as_bytes()[3].is_ascii_digit());
    if slug.is_empty() {
        "new_config".to_string()
    } else if reserved {
        format!("{slug}_config")
    } else {
        slug.to_string()
    }
}

/// Where a never-saved draft is written: `<dir>/<slug>.yml`, or `<slug>-2.yml`, `-3`, … when
/// that file already exists. A new draft must never overwrite a config it did not load — two
/// drafts labelled the same used to replace each other silently.
fn new_config_path(dir: &std::path::Path, label: &str) -> PathBuf {
    let stem = slug(label);
    let mut candidate = dir.join(format!("{stem}.yml"));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{stem}-{n}.yml"));
        n += 1;
    }
    candidate
}

/// The config library's description for `path`, if the library knows the file.
fn library_description(path: &std::path::Path, cx: &App) -> Option<String> {
    let file_path = path.to_string_lossy().to_string();
    AppSettings::get(cx)
        .task_configs
        .iter()
        .find(|c| c.file_path == file_path)
        .map(|c| c.description.to_string())
}

/// What "Discard draft" returns to (plan 1.7, P0-7): the saved file, re-read from disk, or —
/// for a config never saved — a fresh draft with only its required settings. Never an empty
/// form: the old Discard cleared every value, `task` included, and on a saved config that
/// looked like the file had been emptied.
fn discarded_draft(current: &ConfigDraft) -> Result<ConfigDraft, String> {
    let mut draft = match &current.path {
        Some(path) => ConfigDraft::load(path)
            .map_err(|e| format!("Couldn't re-read {}: {e}", path.display()))?,
        None => ConfigDraft::default(),
    };
    ensure_required_settings(&mut draft);
    Ok(draft)
}

fn ensure_required_settings(draft: &mut ConfigDraft) {
    for def in catalog::catalog()
        .iter()
        .filter(|def| def.required && !def.locked)
        .rev()
    {
        if !draft.values.contains_key(&def.key) {
            let value = if def.default.is_null() {
                editors::empty_value(def.shape)
            } else {
                def.default.clone()
            };
            draft.values.shift_insert(0, def.key.clone(), value);
        }
    }
}

impl ConfigBuilder {
    fn title_chrome_state(&self) -> BuilderTitleChromeState {
        let title: SharedString = self.display_label().into();
        let navigation = if self.ancestors.is_empty() {
            BuilderTitleNavigation::Root { title }
        } else {
            BuilderTitleNavigation::Chained {
                crumbs: self
                    .ancestors
                    .iter()
                    .map(|path| SharedString::from(stem_of(path)))
                    .collect(),
                title,
                depth: self.ancestors.len() + 1,
            }
        };
        BuilderTitleChromeState {
            navigation,
            save: self.chrome.save.clone(),
        }
    }

    pub fn new(
        path: Option<PathBuf>,
        ancestors: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut notice = None;
        let mut draft = match &path {
            Some(p) => ConfigDraft::load(p).unwrap_or_else(|e| {
                notice = Some(SharedString::from(format!(
                    "Couldn't open {}: {e}",
                    p.display()
                )));
                ConfigDraft::default()
            }),
            None => ConfigDraft::default(),
        };

        // Required settings are part of the form's structure, not optional palette choices.
        // Insert missing ones at the start so `task` is always the first editable row, including
        // for a new draft and for an invalid hand-written config that omitted it.
        ensure_required_settings(&mut draft);

        // The library is the only place the description exists, so a draft opened from a file
        // has to go and find it. A file the library has never seen simply has none.
        let description = path
            .as_ref()
            .and_then(|p| library_description(p, cx))
            .unwrap_or_default();
        let loaded_numbers = numeric_keys(&draft);
        let snapshot = BuilderSnapshot {
            widget_errors: HashMap::new(),
            carried_text: HashMap::new(),
            profile: AppSettings::get(cx).active_profile.clone(),
            draft,
            description,
            ancestors,
            loaded_numbers,
            collapsed_chain: HashSet::new(),
            // Settings → Config builder → "Show the YAML panel". Defaults on: the preview is
            // the thing that makes the editors legible to someone who knows the YAML already.
            chrome: BuilderChromeState {
                save: if notice.is_some() {
                    BuilderTitleSaveState::Error("Load failed".into())
                } else {
                    BuilderTitleSaveState::Clean
                },
                yaml: if AppSettings::bool_or("builder_show_yaml", true, cx) {
                    BuilderYamlState::Visible
                } else {
                    BuilderYamlState::Hidden
                },
                notice,
            },
        };
        Self::restore(snapshot, BuilderHost::Window, window, cx)
    }

    /// Everything a builder holds that is not a widget: enough to rebuild it in another host
    /// (`dock`). The widgets are rebuilt from the draft, which is the source of truth anyway.
    pub(crate) fn snapshot(&self, cx: &App) -> BuilderSnapshot {
        // A refused value lives only in its widget; keep the text of every widget whose
        // setting has one, so the rebuilt widget shows what was typed, error and all.
        let refused = |id: &SharedString| {
            let key = id.split('#').next().unwrap_or_default();
            self.widget_errors.contains_key(key)
        };
        let mut carried_text: HashMap<SharedString, String> = self
            .inputs
            .iter()
            .filter(|(id, _)| refused(id))
            .map(|(id, input)| (id.clone(), input.read(cx).value().to_string()))
            .collect();
        carried_text.extend(
            self.textareas
                .iter()
                .filter(|(id, _)| refused(id))
                .map(|(id, area)| (id.clone(), area.read(cx).value().to_string())),
        );
        BuilderSnapshot {
            widget_errors: self.widget_errors.clone(),
            carried_text,
            profile: self.profile.clone(),
            draft: self.draft.clone(),
            description: self.description.clone(),
            ancestors: self.ancestors.clone(),
            loaded_numbers: self.loaded_numbers.clone(),
            collapsed_chain: self.collapsed_chain.clone(),
            chrome: self.chrome.clone(),
        }
    }

    /// Build the view around `snapshot`, hosted in its own window or in the main window's dock.
    pub(crate) fn restore(
        snapshot: BuilderSnapshot,
        host: BuilderHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let BuilderSnapshot {
            widget_errors,
            carried_text,
            profile,
            draft,
            description,
            ancestors,
            loaded_numbers,
            collapsed_chain,
            chrome,
        } = snapshot;

        let search = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Search settings to add — try \"log\", \"rollback\", \"media\"")
        });
        let yaml_text = draft.to_yaml();
        let yaml_editor =
            cx.new(|cx| EditorState::new(window, cx).default_value(yaml_text.clone()));
        // Remember the editor side's size (the window less the YAML panel) for the next builder.
        // Only a change is written, so a drag is a handful of writes, not one per frame.
        let size_sub = cx.observe_window_bounds(window, |this, window, cx| {
            // Docked, the window is the main window: its size is not the builder's.
            if this.host == BuilderHost::Docked {
                return;
            }
            dock::window_moved(this, window, cx);
            // A maximized or full-screen size is the screen's, not a size the user chose.
            if window.is_maximized() || window.is_fullscreen() {
                return;
            }
            let bounds = window.bounds().size;
            let editor_width = if this.chrome.yaml_visible() {
                bounds.width - YAML_PANEL_WIDTH
            } else {
                bounds.width
            };
            let text = format!(
                "{}x{}",
                f32::from(editor_width).round(),
                f32::from(bounds.height).round()
            );
            if parse_size(&text).is_none() {
                return;
            }
            this.size_save = Some(cx.spawn(async move |_, cx| {
                cx.background_executor().timer(SIZE_SAVE_DELAY).await;
                cx.update(|cx| {
                    let stored = AppSettings::get(cx)
                        .values
                        .get(BUILDER_SIZE_KEY)
                        .map(|v| v.text().to_string());
                    if stored.as_deref() != Some(text.as_str()) {
                        AppSettings::set_text(BUILDER_SIZE_KEY, text.into(), cx);
                    }
                });
            }));
        });
        let settings_sub = cx.observe_global::<AppSettings>(|this, cx| {
            let inputs = ValidationInputs::read(cx);
            if inputs != this.validation_inputs {
                this.validation_inputs = inputs;
                this.last_chain_scan = None;
                this.refresh_problems(cx);
            }
        });
        let _subscriptions = vec![
            size_sub,
            settings_sub,
            cx.subscribe(&search, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change | InputEvent::Focus) {
                    let query = this.search.read(cx).value().trim().to_string();
                    this.search_open = !query.is_empty();
                    if matches!(event, InputEvent::Change) {
                        this.search_selected = None;
                    }
                    cx.notify();
                }
            }),
        ];

        let problems = problems_for(&draft, &widget_errors, cx);
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            profile,
            host,
            draft,
            description,
            parent_path: ancestors.last().cloned(),
            snap: None,
            last_bounds: None,
            snap_task: None,
            focus_handle,
            ancestors,
            problems,
            inputs: HashMap::new(),
            selects: HashMap::new(),
            textareas: HashMap::new(),
            widget_errors,
            carried_text,
            loaded_numbers,
            search,
            search_selected: None,
            search_scroll: ScrollHandle::new(),
            yaml_editor,
            yaml_text,
            draft_revision: 0,
            yaml_revision: None,
            yaml_markers: Vec::new(),
            search_open: false,
            chrome,
            chain_nodes: Vec::new(),
            chain_error: None,
            collapsed_chain,
            last_chain_scan: None,
            widget_subscriptions: HashMap::new(),
            chain_scan: None,
            validation_inputs: ValidationInputs::read(cx),
            size_save: None,
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
        let seed = self
            .carried_text
            .remove(&id)
            .unwrap_or_else(|| seed.to_string());
        let placeholder = placeholder.to_string();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(placeholder)
                .default_value(seed)
        });
        let key = setting_key.to_string();
        self.widget_subscriptions.insert(
            id.clone(),
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
                    this.chrome.transition(BuilderChromeEvent::Edited);
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
                    this.chrome.transition(BuilderChromeEvent::Edited);
                    cx.notify();
                }
            }));
        self.inputs.insert(id, input.clone());
        input
    }

    /// The name to show for this draft: what the user typed, else the file's stem, else a
    /// placeholder. Never an empty string — the title bar would collapse around it.
    pub(crate) fn display_label(&self) -> String {
        ui_title(&self.draft)
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
        let carried = self.carried_text.remove(&id).unwrap_or_default();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("+ Add")
                .default_value(carried)
        });
        let key = setting_key.to_string();
        self.widget_subscriptions.insert(
            id.clone(),
            cx.subscribe_in(
                &input,
                window,
                move |this, state, event: &InputEvent, window, cx| {
                    let text = state.read(cx).value().to_string();
                    // Emptying the field drops a refused value, and its message with it.
                    if matches!(event, InputEvent::Change) && text.trim().is_empty() {
                        if this.widget_errors.remove(&key).is_some() {
                            this.revalidate(cx);
                        }
                        return;
                    }
                    // §05: a comma commits too, so a pasted `tif, jp2, png` becomes three chips.
                    // Enter and blur commit whatever is left.
                    let (complete, rest) = match event {
                        InputEvent::Change if text.contains(',') => {
                            let (done, rest) = text.rsplit_once(',').unwrap_or((&text, ""));
                            (done.to_string(), rest.to_string())
                        }
                        InputEvent::PressEnter { .. } | InputEvent::Blur => (text, String::new()),
                        _ => return,
                    };
                    if complete.trim().is_empty() {
                        return;
                    }
                    let refused = this.push_chips(&key, complete.split(','), cx);
                    // Refused values stay in the field to be corrected, not silently dropped, and
                    // so does anything typed after the last comma.
                    let mut keep = refused.join(", ");
                    if !rest.trim().is_empty() {
                        if !keep.is_empty() {
                            keep.push_str(", ");
                        }
                        keep.push_str(rest.trim_start());
                    }
                    state.update(cx, |state, cx| state.set_value(keep, window, cx));
                },
            ),
        );
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
        placeholder: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        if let Some(existing) = self.inputs.get(&id) {
            return existing.clone();
        }
        let seed = self
            .carried_text
            .remove(&id)
            .unwrap_or_else(|| seed.to_string());
        let placeholder = placeholder.to_string();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(placeholder)
                .pattern(INTEGER_PATTERN.clone())
                .default_value(seed)
                .min(0.)
        });
        let key = setting_key.to_string();
        self.widget_subscriptions.insert(
            id.clone(),
            cx.subscribe(&input, move |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.commit(&key, cx);
                }
            }),
        );
        self.inputs.insert(id, input.clone());
        input
    }
    /// A decimal input for `id`: the number input's cache and handler with a pattern that
    /// admits a fraction. No stepper — a step of 1 means nothing for a threshold of 2.5.
    pub(crate) fn decimal_input(
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
        let seed = self
            .carried_text
            .remove(&id)
            .unwrap_or_else(|| seed.to_string());
        let placeholder = placeholder.to_string();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(placeholder)
                .pattern(DECIMAL_PATTERN.clone())
                .default_value(seed)
        });
        let key = setting_key.to_string();
        self.widget_subscriptions.insert(
            id.clone(),
            cx.subscribe(&input, move |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.commit(&key, cx);
                }
            }),
        );
        self.inputs.insert(id, input.clone());
        input
    }

    /// A multi-line YAML editor for `id`. Each change is parsed: good YAML replaces the value
    /// (empty means unset), bad YAML is recorded in `widget_errors` and leaves the draft alone.
    pub(crate) fn yaml_input(
        &mut self,
        id: SharedString,
        setting_key: &str,
        seed: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextareaState> {
        if let Some(existing) = self.textareas.get(&id) {
            return existing.clone();
        }
        let seed = self
            .carried_text
            .remove(&id)
            .unwrap_or_else(|| seed.to_string());
        let state = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(3, 16)
                .default_value(seed)
        });
        let key = setting_key.to_string();
        self.widget_subscriptions.insert(
            id.clone(),
            cx.subscribe(&state, move |this, state, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let text = state.read(cx).value().to_string();
                    match parse_yaml_value(&text) {
                        Ok(value) => {
                            this.widget_errors.remove(&key);
                            this.draft.values.insert(key.clone(), value);
                        }
                        Err(error) => {
                            this.widget_errors
                                .insert(key.clone(), format!("Not valid YAML: {error}"));
                        }
                    }
                    this.revalidate(cx);
                }
            }),
        );
        self.textareas.insert(id, state.clone());
        state
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
    ) -> Entity<SelectState<SearchableVec<DetailSelectItem>>> {
        if let Some(existing) = self.selects.get(&id) {
            return existing.clone();
        }
        // §05: a dropdown of twelve or more values gets a search field — `task` has eighteen.
        let searchable = choices.len() >= SEARCHABLE_SELECT_MIN;
        let state = cx.new(|cx| {
            SelectState::new(SearchableVec::new(choices), None, window, cx).searchable(searchable)
        });
        if let Some(value) = selected {
            state.update(cx, |s, cx| s.set_selected_value(&value, window, cx));
        }
        let key = setting_key.to_string();
        self.widget_subscriptions.insert(
            id.clone(),
            cx.subscribe(
                &state,
                move |this, _, _: &SelectEvent<SearchableVec<DetailSelectItem>>, cx| {
                    this.commit(&key, cx);
                },
            ),
        );
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
        self.textareas
            .retain(|id, _| id.as_ref() != key && !id.starts_with(&prefix));
        self.widget_subscriptions
            .retain(|id, _| id.as_ref() != key && !id.starts_with(&prefix));
        self.widget_errors.remove(key);
    }

    // --- draft mutation --------------------------------------------------------------

    /// Rebuild `key`'s value from its widgets and revalidate. The one place screen state
    /// becomes config state.
    pub(crate) fn commit(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(def) = catalog::find(key) else {
            return;
        };
        match self.read_widgets(def, cx) {
            Ok(Some(value)) => {
                self.widget_errors.remove(key);
                self.draft.values.insert(key.to_string(), value);
            }
            Ok(None) => {}
            Err(error) => {
                self.widget_errors.insert(key.to_string(), error);
            }
        }
        self.revalidate(cx);
    }

    /// The draft changed: recompute its problems and mark it edited.
    pub(crate) fn revalidate(&mut self, cx: &mut Context<Self>) {
        self.refresh_problems(cx);
        self.chrome.transition(BuilderChromeEvent::Edited);
    }

    /// Recompute the problems without calling the draft edited — for when something outside it
    /// changed (Settings, a profile switch).
    pub(crate) fn refresh_problems(&mut self, cx: &mut Context<Self>) {
        self.draft_revision += 1;
        self.problems = problems_for(&self.draft, &self.widget_errors, cx);
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

    /// Back to Workbench's default: the row stays, the key is no longer written (§03 "reset
    /// removes the override").
    pub(crate) fn reset_setting(&mut self, key: &str, cx: &mut Context<Self>) {
        self.draft.values.insert(key.to_string(), Value::Null);
        self.forget_widgets(key);
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
        Some(new_config_path(
            std::path::Path::new(&dir),
            &self.draft.label,
        ))
    }

    /// Save, asking first when the file on disk has comments the save would drop (plan Q8:
    /// the minimum option — warn, rather than round-trip comments).
    fn request_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self
            .draft
            .path
            .clone()
            .filter(|_| self.draft.source_has_comments)
        else {
            self.save(window, cx);
            return;
        };
        let this = cx.entity().downgrade();
        ui::confirm_dialog(
            window,
            cx,
            format!("Save over {}?", stem_of(&path)),
            "This file has comments. Saving rewrites it from the settings shown here: every \
             setting is kept, but comments and hand formatting are not.",
            "Save anyway",
            move |window, cx| {
                this.update(cx, |this, cx| this.save(window, cx)).ok();
            },
        );
    }

    /// Discard, after asking. A saved config goes back to its file; a new one starts again.
    fn request_discard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let body = if self.draft.path.is_some() {
            "Your changes since the last save are lost. The saved file and its links stay as \
             they are."
        } else {
            "Everything you have set in this new config is lost. Nothing has been written to \
             disk yet."
        };
        let this = cx.entity().downgrade();
        ui::confirm_dialog(
            window,
            cx,
            "Discard draft?",
            body,
            "Discard",
            move |_, cx| {
                this.update(cx, |this, cx| this.discard(cx)).ok();
            },
        );
    }

    fn discard(&mut self, cx: &mut Context<Self>) {
        match discarded_draft(&self.draft) {
            Ok(draft) => {
                self.draft = draft;
                self.description = self
                    .draft
                    .path
                    .as_ref()
                    .and_then(|p| library_description(p, cx))
                    .unwrap_or_default();
                // Every widget, including the name and description, is rebuilt from the draft.
                self.inputs.clear();
                self.selects.clear();
                self.textareas.clear();
                self.widget_errors.clear();
                self.loaded_numbers = numeric_keys(&self.draft);
                self.last_chain_scan = None;
                self.problems = validate_draft(&self.draft, cx);
                self.draft_revision += 1;
                self.chrome = BuilderChromeState {
                    yaml: self.chrome.yaml,
                    ..Default::default()
                };
            }
            Err(error) => self.chrome.transition(BuilderChromeEvent::Failed {
                title: "Discard failed".into(),
                detail: error.into(),
            }),
        }
        cx.notify();
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.count(Severity::Error) > 0 {
            self.chrome.transition(BuilderChromeEvent::Failed {
                title: "Save blocked".into(),
                detail: "Fix the problems below before saving.".into(),
            });
            cx.notify();
            return;
        }
        let Some(path) = self.target_path(cx) else {
            self.chrome.transition(BuilderChromeEvent::Failed {
                title: "Save failed".into(),
                detail: "Set a config folder in Settings before saving a new config.".into(),
            });
            cx.notify();
            return;
        };
        if let Err(e) = self.draft.save(&path) {
            self.chrome.transition(BuilderChromeEvent::Failed {
                title: "Save failed".into(),
                detail: format!("Couldn't save {}: {e}", path.display()).into(),
            });
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
        // In the profile the builder was opened under, even if another is active now.
        let profile = self.profile.clone();
        AppSettings::update(cx, |s| {
            let entry = TaskConfig {
                label: label.clone().into(),
                task_name: task.clone().into(),
                file_path: file_path.clone(),
                description: description.clone().into(),
            };
            s.upsert_library_entry(&profile, entry);
        });

        let was_new = self.draft.path.is_none();
        self.draft.mark_saved(&path);
        ConfigFilesChanged::bump(cx);
        if was_new {
            let view = cx.entity().downgrade();
            cx.global_mut::<ConfigBuilderWindows>().register(
                Some(path.clone()),
                window.window_handle(),
                view,
            );
        }
        let mut link_notice = None;
        if was_new && let Some(parent) = self.parent_path.clone() {
            let child = path.clone();
            if let Err(error) = self.apply_chain_edit(
                &parent,
                move |draft, base| draft.link_secondary_task(&child, base),
                cx,
            ) {
                link_notice = Some(format!("Saved, but couldn't link child: {error}"));
            }
        }
        self.draft.label = label;
        self.chrome.transition(BuilderChromeEvent::Saved {
            at: chrono::Local::now().format("%H:%M").to_string().into(),
            notice: link_notice.map(Into::into),
        });
        cx.notify();
    }
}

impl Render for ConfigBuilder {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let errors = self.count(Severity::Error);
        let warnings = self.count(Severity::Warn);
        let has_secondary_tasks = self.draft.values.contains_key("secondary_tasks");

        let docked = self.host == BuilderHost::Docked;
        let body = if docked { v_flex() } else { h_flex() };

        v_flex()
            .id("config-builder")
            .size_full()
            .app_font(cx)
            .key_context("ConfigBuilder")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &ToggleBuilderDock, _, cx| {
                this.toggle_dock(cx);
                cx.stop_propagation();
            }))
            .map(|root| {
                if docked {
                    root.child(self.render_docked_bar(cx))
                } else {
                    root.child(self.render_title_bar(cx))
                }
            })
            .child(
                body.flex_1()
                    .w_full()
                    .min_h(px(0.))
                    .overflow_hidden()
                    .child(
                        v_flex()
                            .flex_1()
                            // A flex item's basis is its content, so without a zero minimum a
                            // long key or a wide results row makes this column — and with it
                            // the window — grow. Everything inside must fit the column, never
                            // the other way round.
                            .min_w(px(0.))
                            .min_h(px(0.))
                            .when(!docked, |column| column.h_full())
                            .p(PAD_PAGE)
                            .gap(GAP_2XL)
                            .overflow_y_scrollbar()
                            .child(self.render_header(window, cx))
                            .child(self.render_locked_band(cx))
                            .child(self.render_search(window, cx))
                            .child(v_flex().w_full().children(self.render_settings(window, cx)))
                            .children(has_secondary_tasks.then(|| self.render_chain(window, cx))),
                    )
                    .children(
                        self.chrome
                            .yaml_visible()
                            .then(|| self.render_yaml_panel(window, cx)),
                    ),
            )
            .child(self.render_footer(errors, warnings, cx))
            // Docked, the main window's own dialog layer shows this builder's dialogs.
            .children(
                (!docked)
                    .then(|| Root::render_dialog_layer(window, cx))
                    .flatten(),
            )
    }
}

/// What the title row says: the crumb, the save state, and the chain level.
struct TitleParts {
    crumbs: Vec<AnyElement>,
    save: Option<(SharedString, Hsla)>,
    depth: Option<usize>,
}

impl ConfigBuilder {
    /// Docked, the dock's tab names the builder and the main window owns the title bar; this
    /// row keeps the crumb and save state, with Pop out beside them.
    fn render_docked_bar(&self, cx: &Context<Self>) -> impl IntoElement {
        let TitleParts { crumbs, save, .. } = self.title_parts(cx);
        h_flex()
            .w_full()
            .flex_none()
            .px_3()
            .py_1()
            .gap(GAP_SM)
            .items_center()
            .border_b_1()
            .border_color(cx.theme().colors.border)
            .child(
                h_flex()
                    .flex_1()
                    .min_w(px(0.))
                    .gap(GAP_SM)
                    .items_center()
                    .children(crumbs),
            )
            .children(save.map(|(label, color)| Label::new(label).text_xs().text_color(color)))
            .child(
                ghost_button("builder-pop-out")
                    .icon(IconName::ExternalLink)
                    .tooltip_with_action("Pop out", &ToggleBuilderDock, Some("ConfigBuilder"))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_dock(cx))),
            )
            .child(
                ghost_button("builder-close-docked")
                    .icon(IconName::Close)
                    .tooltip("Close the builder")
                    .on_click(cx.listener(|this, _, window, cx| this.close_docked(window, cx))),
            )
    }

    /// Title, then the page crumb, then — pushed to the right, immediately before the window
    /// controls — whatever transient thing the window has to say (§02).
    ///
    /// A child window says so in its crumb rather than in a separate line: `Main batch items /
    /// Child pages` is `1d`'s breadcrumb, and it is the only place the relationship is stated
    /// once the child has been floated away from its parent.
    fn title_parts(&self, cx: &Context<Self>) -> TitleParts {
        let muted = cx.theme().colors.muted_foreground;
        let state = self.title_chrome_state();
        let mut crumbs = Vec::new();
        let depth = match state.navigation {
            BuilderTitleNavigation::Root { title } => {
                crumbs.push(Label::new("Config Builder").text_xs().into_any_element());
                crumbs.push(
                    Label::new(title)
                        .text_xs()
                        .text_color(muted)
                        .into_any_element(),
                );
                None
            }
            BuilderTitleNavigation::Chained {
                crumbs: path,
                title,
                depth,
            } => {
                crumbs.push(
                    Label::new("↳")
                        .text_xs()
                        .font_semibold()
                        .text_color(cx.theme().colors.warning)
                        .into_any_element(),
                );
                for ancestor in path {
                    crumbs.push(
                        Label::new(ancestor)
                            .text_xs()
                            .text_color(muted)
                            .into_any_element(),
                    );
                    crumbs.push(
                        Label::new("/")
                            .text_xs()
                            .text_color(muted)
                            .into_any_element(),
                    );
                }
                crumbs.push(Label::new(title).text_xs().into_any_element());
                Some(depth)
            }
        };
        let save = match state.save {
            BuilderTitleSaveState::Clean => None,
            BuilderTitleSaveState::Dirty => Some(("Edited".into(), muted)),
            // §02: "Draft saved 12:04" — when, not just that.
            BuilderTitleSaveState::Saved(at) => Some((format!("Saved {at}").into(), muted)),
            BuilderTitleSaveState::Error(message) => Some((message, cx.theme().colors.danger)),
        };

        TitleParts {
            crumbs,
            save,
            depth,
        }
    }

    fn render_title_bar(&self, cx: &Context<Self>) -> impl IntoElement {
        let muted = cx.theme().colors.muted_foreground;
        let TitleParts {
            crumbs,
            save,
            depth,
        } = self.title_parts(cx);
        // Linux draws its own ✕ here, which would otherwise close without asking.
        let this = cx.entity().downgrade();
        TitleBar::new()
            .on_close_window(move |_, window, cx| {
                if this
                    .upgrade()
                    .is_none_or(|builder| may_close_builder(&builder, window, cx))
                {
                    window.remove_window();
                }
            })
            .child(
                h_flex()
                    .flex_1()
                    .gap(GAP_SM)
                    .items_center()
                    .children(crumbs),
            )
            .child(
                h_flex()
                    .justify_end()
                    .items_center()
                    .gap(GAP_MD)
                    .pr_4()
                    .children(
                        save.map(|(label, color)| Label::new(label).text_xs().text_color(color)),
                    )
                    .when_some(depth, |this, depth| {
                        this.child(
                            Label::new(format!("level {depth}"))
                                .text_xs()
                                .font_family(cx.theme().mono_font_family.clone())
                                .text_color(muted),
                        )
                    }),
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
    fn render_locked_band(&self, cx: &App) -> AnyElement {
        if let Some(parent) = self.parent_path.clone() {
            let parent_ancestors =
                self.ancestors[..self.ancestors.len().saturating_sub(1)].to_vec();
            return h_flex()
                .w_full()
                .items_center()
                .px(GAP_MD)
                .py(GAP_SM)
                .rounded(cx.theme().radius)
                .border_1()
                .border_color(cx.theme().colors.border)
                .bg(cx.theme().colors.secondary)
                .child(
                    Label::new("Inherits host and credentials_file_path from the root")
                        .text_xs()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(div().flex_1())
                .child(
                    ghost_button("open-parent-config")
                        .label("Up to parent")
                        .on_click(move |_, _, cx| {
                            open_config_builder_in_chain(
                                Some(parent.clone()),
                                parent_ancestors.clone(),
                                cx,
                            )
                        }),
                )
                .into_any_element();
        }

        let mut band = LockedBand::new(
            "Supplied by the app at run time",
            "you don't set these here",
        );
        for def in catalog::locked() {
            band = band.entry(
                def.key.clone(),
                locked_source(&def.key, self.draft.values.get(&def.key)),
            );
        }
        band.into_any_element()
    }

    fn render_footer(
        &mut self,
        errors: usize,
        warnings: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // The docked pane is half the width of a builder window: shorter labels, and the
        // problem summary gives way before the buttons do.
        let compact = self.host == BuilderHost::Docked;
        let left = h_flex()
            .flex_1()
            .min_w(px(0.))
            .overflow_hidden()
            .gap(GAP_MD)
            .items_center()
            .child(ProblemSummary::new(errors, warnings))
            .children(
                self.chrome
                    .notice
                    .clone()
                    .map(|n| Label::new(n).text_xs().text_color(cx.theme().colors.danger)),
            );

        let right = h_flex()
            .flex_none()
            .gap(GAP_SM)
            .items_center()
            .child(
                status_bar_button("toggle-yaml")
                    .label(match (self.chrome.yaml_visible(), compact) {
                        (true, false) => "Hide YAML",
                        (false, false) => "Show YAML",
                        (_, true) => "YAML",
                    })
                    .selected(compact && self.chrome.yaml_visible())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.chrome.transition(BuilderChromeEvent::ToggleYaml);
                        if this.host == BuilderHost::Window {
                            this.resize_for_yaml(window);
                        }
                        cx.notify();
                    })),
            )
            .child(
                status_bar_button("discard-draft")
                    .label(if compact { "Discard" } else { "Discard draft" })
                    .disabled(!self.chrome.is_dirty())
                    .tooltip(if self.draft.path.is_some() {
                        "Go back to the saved file"
                    } else {
                        "Start this config again"
                    })
                    .on_click(cx.listener(|this, _, window, cx| this.request_discard(window, cx))),
            )
            .child(
                status_bar_button("save-config")
                    .primary()
                    .label(if compact { "Save" } else { "Save to library" })
                    .icon(IconName::Check)
                    // §06: disabled by errors only. A warning is something to know, not a
                    // veto — blocking on one teaches people to write configs that say less.
                    .disabled(errors > 0)
                    .tooltip(save_tooltip(&self.problems))
                    .on_click(cx.listener(|this, _, window, cx| this.request_save(window, cx))),
            );

        // The builder footer and the main task/status bar are one piece of window chrome. Using
        // the same container and compact button skin keeps both outer and inner metrics aligned.
        StatusBarElement::new()
            .h(ui::tokens::STATUS_BAR_H)
            .px_3()
            .text_xs()
            .text_color(cx.theme().colors.foreground)
            .left(left)
            .right(right)
    }
}

/// §06: a disabled button says why. Save names the first thing blocking it.
fn save_tooltip(problems: &[Problem]) -> SharedString {
    match problems.iter().find(|p| p.severity == Severity::Error) {
        Some(problem) => match &problem.key {
            Some(key) => format!("Fix first — {key}: {}", problem.message).into(),
            None => format!("Fix first — {}", problem.message).into(),
        },
        None => "Write the YAML and add it to the config library".into(),
    }
}

/// Where a locked setting's value comes from, in the words `1a` uses.
///
/// A `match` rather than catalogue data: two keys, and the answer is about how *this app* runs
/// Workbench, which the generated catalogue has no business knowing.
///
/// A file loaded from disk may already set one of them. That is said here, in the band, rather
/// than as a validation warning with no row to show it on (plan 1.10, P0-11).
fn locked_source(key: &str, in_file: Option<&Value>) -> String {
    let set_in_file = in_file.is_some_and(|value| !value.is_null());
    match key {
        "host" if set_in_file => "set in file — replaced by the ingest server".into(),
        "host" => "Ingest server".into(),
        "credentials_file_path" if set_in_file => "set in file — replaced from app settings".into(),
        "credentials_file_path" => "App settings".into(),
        _ => "The app".into(),
    }
}

/// Whether a builder window may close now. With unsaved edits it asks first, and closes the
/// window itself if the user agrees.
/// Close `window` the way its close button would, if it is a builder window: asking first
/// when it has unsaved edits. Returns whether `window` was a builder.
pub fn close_builder_window(window: AnyWindowHandle, cx: &mut App) -> bool {
    let Some(view) = cx
        .global::<ConfigBuilderWindows>()
        .live()
        .find(|open| open.window == window)
        .and_then(|open| open.view.upgrade())
    else {
        return false;
    };
    let _ = window.update(cx, |_, window, cx| {
        if may_close_builder(&view, window, cx) {
            window.remove_window();
        }
    });
    true
}

fn may_close_builder(builder: &Entity<ConfigBuilder>, window: &mut Window, cx: &mut App) -> bool {
    if !builder.read(cx).chrome.is_dirty() {
        return true;
    }
    let title = ui_title(&builder.read(cx).draft);
    let builder = builder.clone();
    ui::confirm_dialog(
        window,
        cx,
        format!("Close {title} without saving?"),
        "Your edits since the last save are lost. The file on disk is not changed.",
        "Close without saving",
        move |window, cx| {
            builder.update(cx, |builder, _| {
                builder.chrome.save = BuilderTitleSaveState::Clean;
            });
            window.remove_window();
        },
    );
    false
}

/// A draft's name as the user sees it: its label, else its file's name, else "New config".
pub(crate) fn ui_title(draft: &ConfigDraft) -> String {
    if !draft.label.trim().is_empty() {
        return draft.label.clone();
    }
    match &draft.path {
        Some(path) => stem_of(path),
        None => "New config".to_string(),
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
        let target = if self.chrome.yaml_visible() {
            current.width + YAML_PANEL_WIDTH
        } else {
            current.width - YAML_PANEL_WIDTH
        };
        window.resize(size(target.max(MIN_BUILDER_WIDTH), current.height));
    }
}

#[cfg(test)]
mod tests {
    // Not `use super::*`: this module glob-imports `gpui`, whose own `test` macro would
    // shadow the one this needs.
    use super::{
        BuilderChromeEvent, BuilderChromeState, BuilderTitleSaveState, ConfigDraft,
        DECIMAL_PATTERN, INTEGER_PATTERN, discarded_draft, edit_config_file,
        ensure_required_settings, locked_source, new_config_path, parse_size, parse_yaml_value,
        save_tooltip, slug,
    };
    use serde_yaml::Value;
    use workbench_integration::config::validate::{Problem, Severity};

    #[test]
    fn the_save_tooltip_names_the_first_blocker() {
        let problems = vec![
            Problem {
                key: Some("adaptive_pause".into()),
                severity: Severity::Warn,
                message: "slow".into(),
            },
            Problem {
                key: Some("rollback_dir".into()),
                severity: Severity::Error,
                message: "A folder is needed.".into(),
            },
        ];
        assert_eq!(
            save_tooltip(&problems).as_ref(),
            "Fix first — rollback_dir: A folder is needed."
        );
        assert_eq!(
            save_tooltip(&problems[..1]).as_ref(),
            "Write the YAML and add it to the config library"
        );
    }

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

    #[test]
    fn stored_builder_sizes_are_parsed_and_floored() {
        let ok = parse_size("900x700").unwrap();
        assert_eq!((f32::from(ok.width), f32::from(ok.height)), (900., 700.));
        assert!(parse_size("300x200").is_none());
        assert!(parse_size("wide").is_none());
        assert!(parse_size("NaNx700").is_none());
    }

    #[test]
    fn the_decimal_pattern_admits_one_point() {
        for ok in ["", "2", "2.", "2.5", ".5", "-0.25"] {
            assert!(DECIMAL_PATTERN.is_match(ok), "{ok:?}");
        }
        for bad in ["2.5.1", "a", "1e3", "2,5"] {
            assert!(!DECIMAL_PATTERN.is_match(bad), "{bad:?}");
        }
    }

    #[test]
    fn yaml_editor_text_is_parsed_or_refused() {
        assert_eq!(parse_yaml_value("  "), Ok(Value::Null));
        assert!(parse_yaml_value("a: [1, 2]").unwrap().is_mapping());
        assert!(parse_yaml_value("a: [1, 2").is_err());
    }

    #[test]
    fn chrome_transitions_keep_title_and_status_in_sync() {
        let mut chrome = BuilderChromeState::default();
        chrome.transition(BuilderChromeEvent::Failed {
            title: "Save failed".into(),
            detail: "Disk is full".into(),
        });
        assert!(matches!(
            chrome.save,
            BuilderTitleSaveState::Error(ref title) if title.as_ref() == "Save failed"
        ));
        assert_eq!(chrome.notice.as_deref(), Some("Disk is full"));

        chrome.transition(BuilderChromeEvent::Edited);
        assert!(matches!(chrome.save, BuilderTitleSaveState::Dirty));
        assert!(chrome.notice.is_none());

        chrome.transition(BuilderChromeEvent::Saved {
            at: "12:04".into(),
            notice: None,
        });
        assert!(
            matches!(chrome.save, BuilderTitleSaveState::Saved(ref at) if at.as_ref() == "12:04")
        );
        assert!(chrome.notice.is_none());
    }

    #[test]
    fn yaml_visibility_is_a_reversible_chrome_transition() {
        let mut chrome = BuilderChromeState::default();
        assert!(!chrome.yaml_visible());
        chrome.transition(BuilderChromeEvent::ToggleYaml);
        assert!(chrome.yaml_visible());
        chrome.transition(BuilderChromeEvent::ToggleYaml);
        assert!(!chrome.yaml_visible());
    }

    #[test]
    fn required_settings_are_present_without_using_the_palette() {
        let mut draft = ConfigDraft::from_yaml("rollback_dir: ./rollback\n").unwrap();
        ensure_required_settings(&mut draft);

        assert_eq!(
            draft.values.first().map(|(key, _)| key.as_str()),
            Some("task")
        );
        assert_eq!(
            draft.values.get("task").and_then(|value| value.as_str()),
            Some("")
        );
    }

    /// P0-7: Discard returns a saved config to its file, and a new one to a required-only
    /// draft — never to an empty form.
    #[test]
    fn discard_reloads_the_file_or_starts_again() {
        let dir = std::env::temp_dir().join(format!("builder-discard-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("saved.yml");
        std::fs::write(&path, "task: update\nid_field: pid\n").unwrap();

        let mut edited = ConfigDraft::load(&path).unwrap();
        edited.values.clear();
        edited
            .values
            .insert("log_file_path".into(), Value::String("x.log".into()));
        let back = discarded_draft(&edited).unwrap();
        assert_eq!(back.path.as_deref(), Some(path.as_path()));
        assert_eq!(
            back.values.get("task").and_then(Value::as_str),
            Some("update")
        );
        assert_eq!(
            back.values.get("id_field").and_then(Value::as_str),
            Some("pid")
        );
        assert!(!back.values.contains_key("log_file_path"));

        let mut unsaved = ConfigDraft::from_yaml("task: create\nid_field: id\n").unwrap();
        unsaved.path = None;
        let fresh = discarded_draft(&unsaved).unwrap();
        assert!(fresh.is_blank());
        assert!(fresh.values.contains_key("task"));

        let _ = std::fs::remove_dir_all(dir);
    }

    /// P0-8: a label never reaches the file system unsanitised.
    #[test]
    fn labels_become_safe_file_names() {
        assert_eq!(slug("Main batch items"), "main_batch_items");
        assert_eq!(slug("../../etc/passwd"), "etc_passwd");
        assert_eq!(slug("a:b"), "a_b");
        assert_eq!(slug("  "), "new_config");
        assert_eq!(slug("Übersicht — pages"), "bersicht_pages");
        assert_eq!(slug("create-items_v2"), "create-items_v2");
        assert_eq!(slug("CON"), "con_config");
        assert_eq!(slug("com1"), "com1_config");
        assert_eq!(slug("console"), "console");
    }

    /// P0-8: two new drafts with the same label get two files.
    #[test]
    fn a_new_config_never_overwrites_an_existing_file() {
        let dir = std::env::temp_dir().join(format!("builder-slug-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = new_config_path(&dir, "Pages");
        assert_eq!(first, dir.join("pages.yml"));
        std::fs::write(&first, "task: create\n").unwrap();
        assert_eq!(new_config_path(&dir, "Pages"), dir.join("pages-2.yml"));
        std::fs::write(dir.join("pages-2.yml"), "task: create\n").unwrap();
        assert_eq!(new_config_path(&dir, "pages"), dir.join("pages-3.yml"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// P0-9's fall-back branch: with no window open on the owner, the file is edited — and
    /// left alone when the edit changes nothing.
    #[test]
    fn a_chain_edit_with_no_open_window_goes_to_the_file() {
        let dir = std::env::temp_dir().join(format!("builder-chain-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let child = dir.join("pages.yml");
        std::fs::write(&child, "task: create\n").unwrap();
        let parent = dir.join("root.yml");
        std::fs::write(
            &parent,
            format!("task: create\nsecondary_tasks: [{}]\n", child.display()),
        )
        .unwrap();

        edit_config_file(&parent, |draft| draft.unlink_secondary_task(&child, None)).unwrap();
        let reloaded = ConfigDraft::load(&parent).unwrap();
        assert!(reloaded.secondary_tasks().is_empty());

        let modified = std::fs::metadata(&parent).unwrap().modified().unwrap();
        edit_config_file(&parent, |_| false).unwrap();
        assert_eq!(
            std::fs::metadata(&parent).unwrap().modified().unwrap(),
            modified
        );
        assert!(edit_config_file(&dir.join("missing.yml"), |_| true).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// P0-11: a value already in the file is stated in the locked band.
    #[test]
    fn locked_band_says_when_the_file_already_sets_a_key() {
        let url = Value::String("https://x.org".into());
        assert_eq!(locked_source("host", None), "Ingest server");
        assert!(locked_source("host", Some(&url)).starts_with("set in file"));
    }
}
