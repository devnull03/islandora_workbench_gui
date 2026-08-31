//! Local validation for a [`ConfigDraft`] — the ✓ / ! / ✕ lines under each setting in the
//! builder (mockup `1b`).
//!
//! Everything here is local: shape conformance, required settings, whether a path resolves,
//! and a small table of cross-field rules. Nothing touches the network. Server-backed checks
//! (does this vocabulary exist, does this term URI resolve) are a later stage — see
//! `docs/plans/stage-1-config-builder.md`.

use std::path::Path;

use serde_yaml::Value;

use super::ConfigDraft;
use super::catalog::{self, Shape};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Something is true and worth confirming. Rendered as a ✓ line.
    Ok,
    /// The run will proceed but behave in a way worth knowing about. `! N things to know`.
    Warn,
    /// The config cannot be saved as it stands. `✕ N problems to fix`.
    Error,
}

#[derive(Debug, Clone)]
pub struct Problem {
    /// The setting this attaches to, or `None` for a whole-config problem.
    pub key: Option<String>,
    pub severity: Severity,
    pub message: String,
}

impl Problem {
    fn new(key: impl Into<String>, severity: Severity, message: impl Into<String>) -> Self {
        Self {
            key: Some(key.into()),
            severity,
            message: message.into(),
        }
    }
}

/// A cross-field rule: settings only make sense together, so some problems have no single
/// owner. Each rule reads the whole draft and returns at most one problem.
///
/// Deliberately a plain slice rather than a trait or a registry — it is expected to grow one
/// entry at a time, and a `fn` per rule is the smallest thing that reads clearly.
type Rule = fn(&ConfigDraft) -> Option<Problem>;

const RULES: &[Rule] = &[
    rollback_dir_needed,
    missing_files_are_only_logged,
    adaptive_pause_costs_time,
    secondary_task_cannot_be_self,
];

/// `timestamp_rollback` writes per-run rollback files, and it has nowhere to write them
/// without `rollback_dir`. Mockup `1b` calls this out as the one hard error.
fn rollback_dir_needed(draft: &ConfigDraft) -> Option<Problem> {
    if !as_bool(draft.values.get("timestamp_rollback")) {
        return None;
    }
    if is_non_empty_string(draft.values.get("rollback_dir")) {
        return None;
    }
    Some(Problem::new(
        "rollback_dir",
        Severity::Error,
        "A folder is needed because timestamp_rollback is on. Without it the run can't be undone.",
    ))
}

fn missing_files_are_only_logged(draft: &ConfigDraft) -> Option<Problem> {
    as_bool(draft.values.get("allow_missing_files")).then(|| {
        Problem::new(
            "allow_missing_files",
            Severity::Warn,
            "Missing files will be reported in the log but won't stop the run. \
             Check the log afterwards.",
        )
    })
}

/// A pause of a few seconds is invisible on ten rows and adds an hour on a thousand. The row
/// count is not known here, so the warning states the per-row cost instead of guessing.
fn adaptive_pause_costs_time(draft: &ConfigDraft) -> Option<Problem> {
    let secs = draft.values.get("adaptive_pause")?.as_i64()?;
    (secs >= 3).then(|| {
        Problem::new(
            "adaptive_pause",
            Severity::Warn,
            format!("{secs}s between requests adds {secs} seconds per row to the run."),
        )
    })
}

/// A config that runs itself never terminates. The full cycle check across a nested chain is
/// Stage 6; this catches the one-step case the flat list can produce.
fn secondary_task_cannot_be_self(draft: &ConfigDraft) -> Option<Problem> {
    let path = draft.path.as_deref()?;
    let name = path.file_name()?;
    draft
        .secondary_tasks()
        .iter()
        .any(|t| t.file_name() == Some(name))
        .then(|| {
            Problem::new(
                "secondary_tasks",
                Severity::Error,
                "A config can't run inside itself.",
            )
        })
}

/// Every problem in the draft, most severe first.
pub fn validate(draft: &ConfigDraft) -> Vec<Problem> {
    let mut problems = Vec::new();

    for def in catalog::catalog().iter().filter(|d| d.required) {
        if !draft.values.contains_key(&def.key) {
            problems.push(Problem::new(
                &def.key,
                Severity::Error,
                format!("{} is required.", def.key),
            ));
        }
    }

    for (key, value) in &draft.values {
        if ConfigDraft::is_app_supplied(key) {
            problems.push(Problem::new(
                key,
                Severity::Warn,
                "The app writes this at run time; the value here will be replaced.",
            ));
            continue;
        }
        let Some(def) = catalog::find(key) else {
            problems.push(Problem::new(
                key,
                Severity::Warn,
                "Not a setting Workbench recognises. Check the spelling.",
            ));
            continue;
        };
        problems.extend(check_shape(&def.key, def.shape, value));
        if !def.choices.is_empty()
            && let Some(found) = value.as_str()
            && !def.choices.iter().any(|c| c.value == found)
        {
            let allowed: Vec<&str> = def.choices.iter().map(|c| c.value.as_str()).collect();
            problems.push(Problem::new(
                key,
                Severity::Error,
                format!("\"{found}\" is not one of: {}.", allowed.join(", ")),
            ));
        }
        if def.shape == Shape::FilePath
            && let Some(raw) = value.as_str().filter(|s| !s.is_empty())
        {
            problems.push(check_path(key, raw));
        }
    }

    problems.extend(RULES.iter().filter_map(|rule| rule(draft)));
    problems.sort_by_key(|p| std::cmp::Reverse(p.severity));
    problems
}

/// Does the value match the shape the catalogue declares? Returns at most one problem —
/// a wrong shape is one mistake however many sub-values it has.
fn check_shape(key: &str, shape: Shape, value: &Value) -> Option<Problem> {
    let wrong = |want: &str| {
        Some(Problem::new(
            key,
            Severity::Error,
            format!("Expected {want}."),
        ))
    };

    match shape {
        Shape::Boolean => (!value.is_bool()).then(|| wrong("true or false")).flatten(),
        Shape::Integer => (!value.is_i64() && !value.is_u64())
            .then(|| wrong("a whole number"))
            .flatten(),
        Shape::Delimiter => match value.as_str() {
            Some(s) if s.chars().count() == 1 => None,
            _ => wrong("a single character"),
        },
        Shape::String
        | Shape::Enum
        | Shape::Url
        | Shape::FilePath
        | Shape::TemplateString => (!value.is_string()).then(|| wrong("text")).flatten(),
        // A nullable enum is unset by being absent or null, so null is the point of it.
        Shape::NullableEnum => (!value.is_string() && !value.is_null())
            .then(|| wrong("text, or nothing"))
            .flatten(),
        Shape::ListOfStrings | Shape::CommandList | Shape::ConfigRef => match value {
            Value::Sequence(items) if items.iter().all(Value::is_string) => None,
            // Workbench accepts a bare string where a one-item list is meant.
            Value::String(_) => None,
            _ => wrong("a list of text values"),
        },
        Shape::ListOfNumbers => match value {
            Value::Sequence(items) if items.iter().all(|v| v.is_i64() || v.is_u64()) => None,
            _ => wrong("a list of numbers"),
        },
        Shape::ListOfOneKeyMaps => match value {
            Value::Sequence(items)
                if items
                    .iter()
                    .all(|v| matches!(v.as_mapping(), Some(m) if m.len() == 1)) =>
            {
                None
            }
            _ => wrong("a list of single-key entries"),
        },
        Shape::Map => (!value.is_mapping()).then(|| wrong("a set of key/value pairs")).flatten(),
        Shape::MapOfLists => match value {
            // Written either as a mapping of lists, or as a list of one-key mappings whose
            // values are lists. Workbench's own defaults use the second form.
            Value::Mapping(m) if m.values().all(Value::is_sequence) => None,
            Value::Sequence(items)
                if items.iter().all(|v| {
                    matches!(v.as_mapping(), Some(m) if m.len() == 1 && m.values().all(Value::is_sequence))
                }) =>
            {
                None
            }
            _ => wrong("entries whose values are lists"),
        },
    }
}

/// Mockup `1b` shows a resolved path and whether its folder exists. A path that does not
/// resolve yet is a warning, not an error — Workbench creates some of them itself, and the
/// path may be relative to a workbench directory this crate does not know here.
fn check_path(key: &str, raw: &str) -> Problem {
    let path = Path::new(raw);
    let dir = if raw.ends_with(['/', '\\']) {
        Some(path)
    } else {
        path.parent().filter(|p| !p.as_os_str().is_empty())
    };

    match dir {
        Some(dir) if dir.is_dir() => Problem::new(
            key,
            Severity::Ok,
            format!("Folder {} exists.", dir.display()),
        ),
        Some(dir) if path.is_relative() => Problem::new(
            key,
            Severity::Ok,
            format!("Relative to the Workbench folder: {}", dir.display()),
        ),
        Some(dir) => Problem::new(
            key,
            Severity::Warn,
            format!("Folder {} doesn't exist yet.", dir.display()),
        ),
        None => Problem::new(key, Severity::Ok, "Relative to the Workbench folder."),
    }
}

fn as_bool(value: Option<&Value>) -> bool {
    value.and_then(Value::as_bool).unwrap_or(false)
}

fn is_non_empty_string(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|s| !s.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errors(draft: &ConfigDraft) -> Vec<String> {
        validate(draft)
            .into_iter()
            .filter(|p| p.severity == Severity::Error)
            .map(|p| p.key.unwrap_or_default())
            .collect()
    }

    /// The shapes in the real sample config must survive a load / write / load cycle, or the
    /// builder silently rewrites configs that already work.
    #[test]
    fn sample_config_round_trips() {
        let sample = include_str!("../../test.config.yml");
        let draft = ConfigDraft::from_yaml(sample).expect("sample config parses");
        let again = ConfigDraft::from_yaml(&draft.to_yaml()).expect("re-parses");
        assert_eq!(draft.values, again.values);

        // And the shapes it uses are the interesting ones, not just scalars.
        assert!(draft.values.contains_key("shutdown"));
        assert!(draft.values.contains_key("csv_value_templates"));
        assert!(
            draft
                .values
                .contains_key("rollback_config_filename_template")
        );
    }

    /// The sample config is a config that really runs, so it must not be reported as broken.
    /// `task` is present, so the only errors would be shape mismatches in the catalogue.
    #[test]
    fn sample_config_has_no_errors() {
        let draft = ConfigDraft::from_yaml(include_str!("../../test.config.yml")).unwrap();
        assert_eq!(errors(&draft), Vec::<String>::new());
    }

    #[test]
    fn task_is_required() {
        let draft = ConfigDraft::default();
        assert_eq!(errors(&draft), vec!["task".to_string()]);
    }

    #[test]
    fn timestamp_rollback_needs_a_folder() {
        let mut draft = ConfigDraft::from_yaml("task: create\ntimestamp_rollback: true\n").unwrap();
        assert_eq!(errors(&draft), vec!["rollback_dir".to_string()]);

        draft
            .values
            .insert("rollback_dir".into(), Value::String("./g/rollback".into()));
        assert_eq!(errors(&draft), Vec::<String>::new());
    }

    #[test]
    fn wrong_shapes_are_caught() {
        let draft = ConfigDraft::from_yaml("task: create\nnodes_only: yes please\n").unwrap();
        assert_eq!(errors(&draft), vec!["nodes_only".to_string()]);

        let draft = ConfigDraft::from_yaml("task: not_a_task\n").unwrap();
        assert_eq!(errors(&draft), vec!["task".to_string()]);
    }

    #[test]
    fn unknown_keys_warn_rather_than_fail() {
        let draft = ConfigDraft::from_yaml("task: create\nrolback_dir: ./x\n").unwrap();
        assert!(errors(&draft).is_empty());
        assert!(
            validate(&draft)
                .iter()
                .any(|p| p.key.as_deref() == Some("rolback_dir") && p.severity == Severity::Warn)
        );
    }
}
