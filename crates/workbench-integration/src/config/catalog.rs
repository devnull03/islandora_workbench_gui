//! The config-builder setting catalogue: every Workbench setting, its value shape, default,
//! description and grouping.
//!
//! The data is generated from Workbench's own `WorkbenchConfig.get_default_config()` by
//! `scripts/gen-config-catalog.py` into `config_catalog.json`, which is embedded here at
//! compile time. Nothing reads Python at runtime, and a stale catalogue shows up as a diff
//! rather than as a surprise in the field. See `docs/plans/stage-1-config-builder.md`.

use std::sync::LazyLock;

use serde::Deserialize;
use serde_yaml::Value;

/// The sixteen YAML value shapes a setting can take, and therefore the sixteen editors the
/// builder can render. Mirrors `SHAPES` in `scripts/gen-config-catalog.py`; adding one means
/// adding it in both places plus an arm in the `config-builder` crate's editors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    /// `nodes_only: true`
    Boolean,
    /// `max_node_title_length: 255`
    Integer,
    /// `user_agent: Islandora Workbench`
    String,
    /// A single character, e.g. `subdelimiter: "|"`
    Delimiter,
    /// Fixed choices, e.g. `update_mode: replace`
    Enum,
    /// Fixed choices that may also be unset, e.g. `fixity_algorithm: md5 # or null`
    NullableEnum,
    /// `input_csv: metadata.csv` — gets a Browse button
    FilePath,
    /// `media_use_tid: http://pcdm.org/use#OriginalFile`
    Url,
    /// `ignore_csv_columns: [machineName, fileTitle]`
    ListOfStrings,
    /// `http_retry_on_status_codes: [500, 502]`
    ListOfNumbers,
    /// Ordered pairs: `csv_field_templates: [{field_model: Image}]`
    ListOfOneKeyMaps,
    /// Keyed table: `media_fields: {image: field_media_image}`
    Map,
    /// `media_types: [{image: [png, gif, jpg]}]`
    MapOfLists,
    /// Hooks that run on this machine: `shutdown: [uv run python ./g/scripts/…]`
    CommandList,
    /// `rollback_config_filename_template: rollback_${config_filename}.yml`
    TemplateString,
    /// Points at another config file: `secondary_tasks: [pages.yml]`
    ConfigRef,
}

impl Shape {
    /// Short label shown beside a setting in the search palette (mockup `1a`).
    pub fn label(self) -> &'static str {
        match self {
            Shape::Boolean => "boolean",
            Shape::Integer => "integer",
            Shape::String => "string",
            Shape::Delimiter => "character",
            Shape::Enum => "enum",
            Shape::NullableEnum => "enum",
            Shape::FilePath => "path",
            Shape::Url => "URL",
            Shape::ListOfStrings | Shape::ListOfNumbers => "list",
            Shape::ListOfOneKeyMaps => "map list",
            Shape::Map => "map",
            Shape::MapOfLists => "map of lists",
            Shape::CommandList => "commands",
            Shape::TemplateString => "template",
            Shape::ConfigRef => "config",
        }
    }
}

/// Which picker a [`Shape::FilePath`] setting opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Browse {
    #[default]
    File,
    Dir,
}

/// One enum choice: the YAML value, and the label shown in the dropdown. The generator emits
/// either a bare string (value doubles as the label) or a `[value, label]` pair.
#[derive(Debug, Clone, Deserialize)]
#[serde(from = "ChoiceRepr")]
pub struct Choice {
    pub value: String,
    pub label: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ChoiceRepr {
    Bare(String),
    Labelled(String, String),
}

impl From<ChoiceRepr> for Choice {
    fn from(repr: ChoiceRepr) -> Self {
        match repr {
            ChoiceRepr::Bare(value) => Choice {
                label: value.clone(),
                value,
            },
            ChoiceRepr::Labelled(value, label) => Choice { value, label },
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SettingDef {
    pub key: String,
    pub shape: Shape,
    /// Workbench's own default, or `null` where it has none.
    #[serde(default)]
    pub default: Value,
    /// One sentence, or empty where nobody has written one yet.
    #[serde(default)]
    pub description: String,
    /// Section in the search palette.
    pub group: String,
    /// The builder refuses to save without it. Only `task` is.
    #[serde(default)]
    pub required: bool,
    /// Supplied by the app at run time (`host`, `credentials_file_path`, `input_csv`). Shown in
    /// the locked band of mockup `1a` and kept out of the addable list.
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub choices: Vec<Choice>,
    /// Suffix after an integer input, e.g. `seconds`.
    #[serde(default)]
    pub unit: Option<String>,
    /// Insertable tokens for a [`Shape::TemplateString`].
    #[serde(default)]
    pub tokens: Vec<String>,
    #[serde(default)]
    pub browse: Browse,
}

#[derive(Deserialize)]
struct Catalog {
    #[allow(dead_code)] // provenance; surfaced in the builder's Help, not used in logic
    workbench_ref: String,
    #[allow(dead_code)]
    generated: String,
    settings: Vec<SettingDef>,
}

static CATALOG: LazyLock<Catalog> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../config_catalog.json"))
        .expect("config_catalog.json is generated and checked in; a parse failure is a build bug")
});

/// Every known setting, in the order Workbench declares them.
pub fn catalog() -> &'static [SettingDef] {
    &CATALOG.settings
}

/// The Workbench commit the catalogue was generated from.
pub fn catalog_workbench_ref() -> &'static str {
    &CATALOG.workbench_ref
}

pub fn find(key: &str) -> Option<&'static SettingDef> {
    catalog().iter().find(|s| s.key == key)
}

/// The three settings the app writes at run time, shown as a locked band in the builder.
pub fn locked() -> impl Iterator<Item = &'static SettingDef> {
    catalog().iter().filter(|s| s.locked)
}

/// Settings a user may add to a config — everything the app does not supply itself.
pub fn addable() -> impl Iterator<Item = &'static SettingDef> {
    catalog().iter().filter(|s| !s.locked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_parses_and_covers_the_basics() {
        let all = catalog();
        assert!(
            all.len() > 100,
            "expected the full catalogue, got {}",
            all.len()
        );

        let task = find("task").expect("task must be in the catalogue");
        assert!(task.required, "task is the only required setting");
        assert_eq!(task.shape, Shape::Enum);
        assert!(task.choices.iter().any(|c| c.value == "create"));

        assert_eq!(
            locked().count(),
            3,
            "host, credentials_file_path, input_csv"
        );
        assert!(addable().all(|s| !s.locked));

        // Every shape the editors match on must actually occur, or an arm is dead code.
        for shape in [
            Shape::CommandList,
            Shape::MapOfLists,
            Shape::TemplateString,
            Shape::ConfigRef,
        ] {
            assert!(
                all.iter().any(|s| s.shape == shape),
                "no setting has shape {shape:?}"
            );
        }
    }
}
