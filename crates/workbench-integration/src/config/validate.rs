//! Local validation for a [`ConfigDraft`] — the ✓ / ! / ✕ lines under each setting in the
//! builder (mockup `1b`).
//!
//! Everything here is local: shape conformance, required settings, whether a path resolves,
//! and a small table of cross-field rules. Nothing touches the network. Server-backed checks
//! (does this vocabulary exist, does this term URI resolve) are a later stage — see
//! `docs/plans/stage-1-config-builder.md`.

use std::path::Path;

use serde_yaml::Value;

use super::catalog::{self, Shape};
use super::{ConfigDraft, chain, draft::resolve_secondary_task, template};

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

/// What validation needs to know beyond the draft itself.
#[derive(Debug, Clone, Copy, Default)]
pub struct ValidationContext<'a> {
    /// The Workbench install directory: what relative `secondary_tasks` entries resolve
    /// against at run time (see [`resolve_secondary_task`]). `None` until Settings has one.
    pub workbench_dir: Option<&'a Path>,
    /// Settings → Config builder → "Check paths as you type". Off skips the file-system
    /// look-ups behind path rows (for slow network drives).
    pub skip_path_checks: bool,
    /// Settings → Config builder → "Warn about settings that clash". Off drops the cross-field
    /// *warnings*; cross-field errors (a run that cannot work) are always reported.
    pub skip_clash_warnings: bool,
}

/// A cross-field rule: settings only make sense together, so some problems have no single
/// owner. Each rule reads the whole draft and returns at most one problem.
///
/// Deliberately a plain slice rather than a trait or a registry — it is expected to grow one
/// entry at a time, and a `fn` per rule is the smallest thing that reads clearly.
type Rule = fn(&ConfigDraft, &ValidationContext) -> Option<Problem>;

const RULES: &[Rule] = &[
    task_needs_its_settings,
    rollback_dir_needed,
    missing_files_are_only_logged,
    adaptive_pause_costs_time,
    secondary_task_cannot_be_self,
];

/// Settings a task cannot run without, from upstream `check_input` (`workbench_utils.py`,
/// "Check for presence of required config keys, which varies by task"). `host`, `username`
/// and `password` are there too; the app supplies those.
const TASK_REQUIRES: &[(&[&str], &str)] = &[
    (
        &["add_media", "update_media", "update_media_by_node"],
        "media_type",
    ),
    (&["create_terms", "update_terms"], "vocab_id"),
    (
        &["get_data_from_view", "get_media_report_from_view"],
        "view_path",
    ),
];

/// Workbench's `--check` exits with "required configuration option … missing" for these; say it
/// on the row instead, before anything runs.
fn task_needs_its_settings(draft: &ConfigDraft, _: &ValidationContext) -> Option<Problem> {
    let task = draft.values.get("task")?.as_str()?;
    TASK_REQUIRES
        .iter()
        .filter(|(tasks, _)| tasks.contains(&task))
        .map(|(_, key)| *key)
        .find(|key| {
            draft.values.get(*key).is_none_or(|value| {
                value.is_null() || value.as_str().is_some_and(|s| s.trim().is_empty())
            })
        })
        .map(|key| {
            Problem::new(
                "task",
                Severity::Error,
                format!("A {task} task needs {key}. Add it with the search above."),
            )
        })
}

/// `timestamp_rollback` writes per-run rollback files, and it has nowhere to write them
/// without `rollback_dir`. Mockup `1b` calls this out as the one hard error.
fn rollback_dir_needed(draft: &ConfigDraft, _: &ValidationContext) -> Option<Problem> {
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

fn missing_files_are_only_logged(draft: &ConfigDraft, _: &ValidationContext) -> Option<Problem> {
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
fn adaptive_pause_costs_time(draft: &ConfigDraft, _: &ValidationContext) -> Option<Problem> {
    let secs = draft.values.get("adaptive_pause")?.as_i64()?;
    (secs >= 3).then(|| {
        Problem::new(
            "adaptive_pause",
            Severity::Warn,
            format!("{secs}s between requests adds {secs} seconds per row to the run."),
        )
    })
}

/// A config that runs itself never terminates. Deeper cycles are reported on the chain's own
/// nodes ([`chain::child_nodes`]); this is the one-step case.
///
/// Compared by file identity, resolved the way Workbench resolves the entry — not by file
/// name, which flagged two different `main.yml`s in different folders (plan 1.10, P0-10).
fn secondary_task_cannot_be_self(draft: &ConfigDraft, cx: &ValidationContext) -> Option<Problem> {
    let path = draft.path.as_deref()?;
    draft
        .secondary_tasks()
        .iter()
        .map(|entry| resolve_secondary_task(entry, Some(path), cx.workbench_dir))
        .any(|entry| chain::same_path(&entry, path))
        .then(|| {
            Problem::new(
                "secondary_tasks",
                Severity::Error,
                "A config can't run inside itself.",
            )
        })
}

/// Every problem in the draft, most severe first.
pub fn validate(draft: &ConfigDraft, cx: &ValidationContext) -> Vec<Problem> {
    let mut problems = Vec::new();
    let task = draft.values.get("task").and_then(Value::as_str);

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
        // App-supplied keys present in a loaded file are shown in the builder's locked band
        // ("set in file — replaced at run time"). A warning here had no row to attach to, so
        // the footer counted "things to know" with nothing to click (plan 1.10, P0-11).
        if ConfigDraft::is_app_supplied(key) {
            continue;
        }
        let Some(def) = catalog::find(key) else {
            let message = match retired_setting(key) {
                Some(note) => note.to_string(),
                None => "Not a setting Workbench recognises. Check the spelling.".to_string(),
            };
            problems.push(Problem::new(key, Severity::Warn, message));
            continue;
        };
        // Null is "unset": the key is not written and Workbench uses its default (§04). Only a
        // required setting has no default to fall back on.
        if value.is_null() {
            if def.required {
                problems.push(Problem::new(
                    key,
                    Severity::Error,
                    format!("{key} is required."),
                ));
            }
            continue;
        }
        problems.extend(check_shape(&def.key, def.shape, value));
        // Nothing picked yet is one short instruction, not a list of every legal value — for
        // `task` that list is eighteen long and reads like a wall of errors.
        if !def.choices.is_empty() && value.as_str() == Some("") {
            problems.push(Problem::new(
                key,
                Severity::Error,
                format!("Choose a {}.", key.replace('_', " ")),
            ));
        } else if !def.choices.is_empty()
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
            && !cx.skip_path_checks
            && let Some(raw) = value.as_str().filter(|s| !s.is_empty())
        {
            let input_dir = draft.values.get("input_dir").and_then(Value::as_str);
            problems.push(check_path(key, raw, cx.workbench_dir, input_dir));
        }
        if def.shape == Shape::Url
            && let Some(raw) = value.as_str()
        {
            problems.extend(check_url(key, raw));
        }
        if def.shape == Shape::TemplateString
            && let Some(raw) = value.as_str()
            && let Some(allowed) = template::allowed_placeholders(key, task)
            && let Some(message) = template::check(raw, allowed)
        {
            problems.push(Problem::new(key, Severity::Error, message));
        }
    }

    problems.extend(
        RULES
            .iter()
            .filter_map(|rule| rule(draft, cx))
            .filter(|p| !(cx.skip_clash_warnings && p.severity == Severity::Warn)),
    );
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
        // Workbench formats these into URLs and requests, so a number reads the same as its
        // digits: `media_use_tid: 17` (a term ID) and `google_sheets_gid: 1867618389` are both
        // in its own docs.
        Shape::String | Shape::Url => (!value.is_string() && !value.is_number())
            .then(|| wrong("text"))
            .flatten(),
        Shape::Enum | Shape::FilePath | Shape::TemplateString => {
            (!value.is_string()).then(|| wrong("text")).flatten()
        }
        Shape::Decimal => (!value.is_number()).then(|| wrong("a number")).flatten(),
        // Any YAML is structurally valid here; Workbench's --check judges the content.
        Shape::Yaml => None,
        // A nullable enum is unset by being absent or null, so null is the point of it.
        Shape::NullableEnum => (!value.is_string() && !value.is_null())
            .then(|| wrong("text, or nothing"))
            .flatten(),
        Shape::ListOfStrings if catalog::holds_term_ids(key) => match value {
            // `"17"` is text to Workbench and matches no term; an ID must be a bare 17.
            Value::Sequence(items)
                if items
                    .iter()
                    .any(|v| v.as_str().is_some_and(|s| s.chars().all(|c| c.is_ascii_digit()))) =>
            {
                Some(Problem::new(
                    key,
                    Severity::Warn,
                    "A term ID in quotes matches nothing; write it as a bare number.",
                ))
            }
            Value::Sequence(items)
                if items.iter().all(|v| v.is_string() || v.is_i64() || v.is_u64()) =>
            {
                None
            }
            _ => wrong("a list of term IDs, URIs or names"),
        },
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
            // Only the list form: upstream iterates these settings (`for types in
            // config["media_types"]: types.items()`), so a mapping crashes Workbench.
            Value::Mapping(_) => wrong("a list of entries, each written `- key: [values]`"),
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

/// Mockup `1b` / spec §03: "Resolves to …\g\logs\workbench.log — folder exists, writable."
/// Relative paths resolve the way Workbench resolves them — against its own folder, where the
/// app runs it — and `~` is expanded as Workbench's `expanduser` does. A folder that does not
/// exist yet is a warning, not an error: Workbench creates some of them itself.
fn check_path(
    key: &str,
    raw: &str,
    workbench_dir: Option<&Path>,
    input_dir: Option<&str>,
) -> Problem {
    // Workbench reads a Google Sheet URL in `input_csv` itself (docs: "Using Google Sheets as
    // input data"); there is no file to look for.
    if key == "input_csv" && (raw.starts_with("https://") || raw.starts_with("http://")) {
        return Problem::new(
            key,
            Severity::Ok,
            "A sheet Workbench downloads when it runs.",
        );
    }
    let mut expanded = expand_home(raw);
    // Workbench joins `input_csv` onto `input_dir` (`os.path.join`, so an absolute `input_csv`
    // wins), and runs from its own folder.
    if key == "input_csv" && !expanded.is_absolute() {
        expanded = expand_home(input_dir.unwrap_or("input_data")).join(expanded);
    }
    let resolved = if expanded.is_absolute() {
        expanded
    } else if let Some(base) = workbench_dir {
        base.join(expanded)
    } else {
        return Problem::new(
            key,
            Severity::Ok,
            "Relative to the Workbench folder (set it in Settings to check this path).",
        );
    };
    // Inputs must already be there; anything else is written, so its folder must be.
    if INPUT_PATHS.contains(&key) {
        return if resolved.exists() {
            Problem::new(
                key,
                Severity::Ok,
                format!("Resolves to {}.", resolved.display()),
            )
        } else {
            Problem::new(
                key,
                Severity::Warn,
                format!("Resolves to {} — nothing is there yet.", resolved.display()),
            )
        };
    }
    let dir = if raw.ends_with(['/', '\\']) {
        resolved.as_path()
    } else {
        resolved.parent().unwrap_or(&resolved)
    };
    match std::fs::metadata(dir) {
        Ok(meta) if meta.is_dir() && meta.permissions().readonly() => Problem::new(
            key,
            Severity::Warn,
            format!(
                "Resolves to {} — the folder is read-only, so Workbench can't write there.",
                resolved.display()
            ),
        ),
        Ok(meta) if meta.is_dir() => Problem::new(
            key,
            Severity::Ok,
            format!("Resolves to {} — the folder exists.", resolved.display()),
        ),
        _ => Problem::new(
            key,
            Severity::Warn,
            format!(
                "Resolves to {} — folder {} doesn't exist yet.",
                resolved.display(),
                dir.display()
            ),
        ),
    }
}

/// Path settings Workbench reads from rather than writes to.
const INPUT_PATHS: &[&str] = &["input_dir", "input_csv"];

/// Settings Workbench's docs still list but its code no longer reads, and what replaced them.
fn retired_setting(key: &str) -> Option<&'static str> {
    Some(match key {
        "strict_check" => {
            "Workbench no longer reads this. Use perform_soft_checks: true in place of strict_check: false."
        }
        "exit_on_first_missing_file_during_check" => {
            "Workbench no longer reads this. Use perform_soft_checks to keep --check going."
        }
        "drupal_8" => "Workbench no longer reads this; it detects the Drupal version itself.",
        _ => return None,
    })
}

/// `~` and `~/…` as Workbench's `os.path.expanduser` reads them.
fn expand_home(raw: &str) -> std::path::PathBuf {
    let home = || {
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(std::path::PathBuf::from)
    };
    if raw == "~" {
        return home().unwrap_or_else(|| raw.into());
    }
    if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\"))
        && let Some(home) = home()
    {
        return home.join(rest);
    }
    std::path::PathBuf::from(raw)
}

/// A URL setting holds a term URI (`http://pcdm.org/use#OriginalFile`) or a term ID (`17`).
/// Anything else will not resolve.
fn check_url(key: &str, raw: &str) -> Option<Problem> {
    let raw = raw.trim();
    if raw.is_empty() || raw.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    // The scheme is case-insensitive (RFC 3986), so `HTTP://` is as good as `http://`.
    let rest = ["http://", "https://"].iter().find_map(|scheme| {
        raw.get(..scheme.len())
            .filter(|head| head.eq_ignore_ascii_case(scheme))
            .map(|_| &raw[scheme.len()..])
    });
    match rest {
        Some(rest)
            if rest
                .split(['/', '#', '?'])
                .next()
                .is_some_and(|h| !h.is_empty()) =>
        {
            None
        }
        _ => Some(Problem::new(
            key,
            Severity::Error,
            "Expected an http(s) URL or a numeric term ID.",
        )),
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
        errors_in(draft, &ValidationContext::default())
    }

    fn errors_in(draft: &ConfigDraft, cx: &ValidationContext) -> Vec<String> {
        validate(draft, cx)
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
    fn an_empty_choice_asks_rather_than_lists() {
        let draft = ConfigDraft::from_yaml("task: ''\n").unwrap();
        let problems = validate(&draft, &ValidationContext::default());
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].message, "Choose a task.");
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

    /// P0-5: an integer cleared back to empty is unset, not a shape error, and is not written.
    #[test]
    fn an_unset_integer_is_not_an_error() {
        let mut draft = ConfigDraft::from_yaml("task: create\n").unwrap();
        draft.values.insert("adaptive_pause".into(), Value::Null);
        assert_eq!(errors(&draft), Vec::<String>::new());

        // A required setting has no default, so unset is still a problem.
        draft.values.insert("task".into(), Value::Null);
        assert_eq!(errors(&draft), vec!["task".to_string()]);
    }

    /// P0-10: two different files may share a name; only the same file is a self-link.
    #[test]
    fn self_link_is_by_file_not_by_name() {
        let tree = crate::config::test_support::TempTree::new();
        let mine = tree.write("a/main.yml", "task: create\n");
        let other = tree.write("b/main.yml", "task: create\n");
        let cx = ValidationContext::default();

        let mut draft = ConfigDraft::load(&mine).unwrap();
        draft.link_secondary_task(&other, None);
        assert_eq!(errors_in(&draft, &cx), Vec::<String>::new());

        draft.set_secondary_tasks(std::slice::from_ref(&mine));
        assert_eq!(errors_in(&draft, &cx), vec!["secondary_tasks".to_string()]);

        // A relative entry resolves against the Workbench directory, as at run time.
        draft.set_secondary_tasks(&[std::path::PathBuf::from("main.yml")]);
        let wb = ValidationContext {
            workbench_dir: mine.parent(),
            ..Default::default()
        };
        assert_eq!(errors_in(&draft, &wb), vec!["secondary_tasks".to_string()]);
        let elsewhere = ValidationContext {
            workbench_dir: other.parent(),
            ..Default::default()
        };
        assert_eq!(errors_in(&draft, &elsewhere), Vec::<String>::new());
    }

    /// P0-11: app-supplied keys in a file are not a warning with nowhere to show it.
    #[test]
    fn app_supplied_keys_in_a_file_raise_nothing() {
        let draft = ConfigDraft::from_yaml(
            "task: create\nhost: https://x.org\ncredentials_file_path: /c.yml\n",
        )
        .unwrap();
        assert!(validate(&draft, &ValidationContext::default()).is_empty());
        // input_csv is an ordinary setting now; a file that sets it is fine.
        let draft = ConfigDraft::from_yaml("task: create\ninput_csv: a.csv\n").unwrap();
        assert!(errors(&draft).is_empty());
    }

    /// Upstream `check_input`: these tasks exit without their setting.
    #[test]
    fn tasks_report_the_setting_they_need() {
        for (yaml, missing) in [
            ("task: add_media\n", "media_type"),
            ("task: update_terms\n", "vocab_id"),
            ("task: get_media_report_from_view\n", "view_path"),
        ] {
            let draft = ConfigDraft::from_yaml(yaml).unwrap();
            let problems = validate(&draft, &ValidationContext::default());
            assert!(
                problems
                    .iter()
                    .any(|p| p.severity == Severity::Error && p.message.contains(missing)),
                "{yaml}"
            );
        }
        let draft = ConfigDraft::from_yaml("task: create_terms\nvocab_id: tags\n").unwrap();
        assert_eq!(errors(&draft), Vec::<String>::new());
        // An empty value is as missing as no value.
        let draft = ConfigDraft::from_yaml("task: create_terms\nvocab_id: ''\n").unwrap();
        assert_eq!(errors(&draft), vec!["task".to_string()]);
    }

    /// Values Workbench's own documentation uses must not be flagged.
    #[test]
    fn documented_values_pass() {
        let draft = ConfigDraft::from_yaml(
            "task: run_scripts\nmedia_use_tid: 17\ngoogle_sheets_gid: 1867618389\n\
             subdelimiter: '%%%'\nadaptive_pause_threshold: 2.5\n\
             view_parameters: ['date=20231202']\n\
             media_type_file_fields: {my_custom_media: field_media_file}\n\
             models: [{'https://schema.org/DigitalDocument': [pdf, doc]}]\n\
             paragraph_fields: {field_p: {type: x, field_order: [a, b]}}\n",
        )
        .unwrap();
        assert_eq!(errors(&draft), Vec::<String>::new());
        assert!(
            validate(&draft, &ValidationContext::default())
                .iter()
                .all(|p| !p.message.starts_with("Not a setting"))
        );
    }

    /// Upstream iterates map-of-lists settings, so only the list form runs.
    #[test]
    fn map_of_lists_must_be_the_list_form() {
        let mapping =
            ConfigDraft::from_yaml("task: create\nmedia_types: {image: [png]}\n").unwrap();
        assert_eq!(errors(&mapping), vec!["media_types".to_string()]);
        let list = ConfigDraft::from_yaml("task: create\nmedia_types: [{image: [png]}]\n").unwrap();
        assert_eq!(errors(&list), Vec::<String>::new());
    }

    /// The two Settings switches gate what they name, and only that.
    #[test]
    fn settings_gate_path_checks_and_clash_warnings() {
        let draft = ConfigDraft::from_yaml(
            "task: create\nallow_missing_files: true\nlog_file_path: /x/y.log\n\
             timestamp_rollback: true\n",
        )
        .unwrap();
        let quiet = ValidationContext {
            skip_path_checks: true,
            skip_clash_warnings: true,
            ..Default::default()
        };
        let problems = validate(&draft, &quiet);
        assert!(
            problems
                .iter()
                .all(|p| p.key.as_deref() != Some("log_file_path"))
        );
        assert!(
            problems
                .iter()
                .all(|p| p.key.as_deref() != Some("allow_missing_files"))
        );
        // The rollback folder is an error, so it stays.
        assert!(
            problems
                .iter()
                .any(|p| p.key.as_deref() == Some("rollback_dir"))
        );
    }

    #[test]
    fn urls_are_uris_or_term_ids() {
        for ok in [
            "http://pcdm.org/use#OriginalFile",
            "https://x.org",
            "17",
            "HTTPS://X.org",
        ] {
            assert!(check_url("media_use_tid", ok).is_none(), "{ok}");
        }
        for bad in ["pcdm.org/use", "http://", "ftp://x.org", "seventeen"] {
            assert!(check_url("media_use_tid", bad).is_some(), "{bad}");
        }
    }

    /// The rollback template pitfall from the docs is caught on the row.
    #[test]
    fn rollback_templates_are_checked_like_python() {
        let draft = ConfigDraft::from_yaml(
            "task: create\nrollback_config_filename_template: x_$config_filename_$input_csv_filename\n",
        )
        .unwrap();
        assert_eq!(
            errors(&draft),
            vec!["rollback_config_filename_template".to_string()]
        );
        let draft = ConfigDraft::from_yaml(
            "task: create_from_files\nrollback_csv_filename_template: ${input_csv_filename}\n",
        )
        .unwrap();
        assert_eq!(
            errors(&draft),
            vec!["rollback_csv_filename_template".to_string()]
        );
    }

    #[test]
    fn paths_resolve_against_the_workbench_folder() {
        let tree = crate::config::test_support::TempTree::new();
        std::fs::create_dir_all(tree.root.join("logs")).unwrap();
        let ok = check_path("log_file_path", "logs/wb.log", Some(&tree.root), None);
        assert_eq!(ok.severity, Severity::Ok);
        assert!(ok.message.contains("the folder exists"));
        assert!(
            ok.message
                .contains(&tree.root.join("logs/wb.log").display().to_string())
        );
        let missing = check_path("log_file_path", "nope/wb.log", Some(&tree.root), None);
        assert_eq!(missing.severity, Severity::Warn);
        let unknown = check_path("log_file_path", "x/wb.log", None, None);
        assert_eq!(unknown.severity, Severity::Ok);
    }

    /// `input_csv` sits inside `input_dir`, and must exist rather than merely have a folder.
    #[test]
    fn input_csv_resolves_inside_input_dir() {
        let tree = crate::config::test_support::TempTree::new();
        tree.write("in/meta.csv", "id\n");
        let found = check_path("input_csv", "meta.csv", Some(&tree.root), Some("in"));
        assert_eq!(found.severity, Severity::Ok);
        assert!(
            found
                .message
                .contains(&tree.root.join("in/meta.csv").display().to_string())
        );
        let sheet = check_path(
            "input_csv",
            "https://docs.google.com/spreadsheets/d/abc/edit",
            Some(&tree.root),
            None,
        );
        assert_eq!(sheet.severity, Severity::Ok);
        let default_dir = check_path("input_csv", "meta.csv", Some(&tree.root), None);
        assert_eq!(default_dir.severity, Severity::Warn);
        assert!(default_dir.message.contains("input_data"));
    }

    /// The docs write media use terms as `[17, 1]`; those are numbers, and stay numbers.
    #[test]
    fn media_use_term_lists_take_ids() {
        let draft = ConfigDraft::from_yaml(
            "task: delete_media_by_node\ndelete_media_by_node_media_use_tids: [17, 1, http://pcdm.org/use#OriginalFile]\n",
        )
        .unwrap();
        assert!(errors(&draft).is_empty());
        let quoted = ConfigDraft::from_yaml(
            "task: delete_media_by_node\ndelete_media_by_node_media_use_tids: [\"17\"]\n",
        )
        .unwrap();
        assert!(
            validate(&quoted, &ValidationContext::default())
                .iter()
                .any(|p| p.severity == Severity::Warn && p.message.contains("bare number"))
        );
    }

    #[test]
    fn retired_settings_name_their_replacement() {
        let draft = ConfigDraft::from_yaml("task: create\nstrict_check: true\n").unwrap();
        assert!(
            validate(&draft, &ValidationContext::default())
                .iter()
                .any(|p| p.key.as_deref() == Some("strict_check")
                    && p.message.contains("perform_soft_checks"))
        );
    }

    #[test]
    fn unknown_keys_warn_rather_than_fail() {
        let draft = ConfigDraft::from_yaml("task: create\nrolback_dir: ./x\n").unwrap();
        assert!(errors(&draft).is_empty());
        assert!(
            validate(&draft, &ValidationContext::default())
                .iter()
                .any(|p| p.key.as_deref() == Some("rolback_dir") && p.severity == Severity::Warn)
        );
    }
}
