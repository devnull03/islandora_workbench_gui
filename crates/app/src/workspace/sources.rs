//! The vocabulary behind step 1's two dropdowns (mockup `2c`).
//!
//! Both dropdowns are keyed by a string that goes into the select and comes back out of it, so
//! this module is the one place that knows what those strings mean. Everything downstream takes
//! a [`workbench_integration::InputSource`] / [`workbench_integration::Processor`].

use std::path::{Path, PathBuf};

use gpui::{App, SharedString};
use settings::AppSettings;
use ui::DetailSelectItem;

pub const SOURCE_SHEET: &str = "google-sheet";
pub const SOURCE_CSV: &str = "csv-file";

/// The built-in Rust importer, and "leave the rows alone". Real scripts sit between them, keyed
/// by their absolute path, which is also how [`processor_for`] recognises them.
pub const PROCESSOR_BUILTIN: &str = "builtin";
pub const PROCESSOR_NONE: &str = "none";

pub fn source_items() -> Vec<DetailSelectItem> {
    vec![
        DetailSelectItem {
            label: "Google Sheet → CSV".into(),
            subtitle: "Fetch a published sheet".into(),
            value: SOURCE_SHEET.into(),
            divider_above: false,
        },
        DetailSelectItem {
            label: "CSV file".into(),
            subtitle: "A CSV already on this machine".into(),
            value: SOURCE_CSV.into(),
            divider_above: true,
        },
    ]
}

/// The scripts folder from Settings, if it is set and still exists.
pub fn scripts_dir(cx: &App) -> Option<PathBuf> {
    let raw = AppSettings::get(cx)
        .values
        .get("preprocess_scripts_dir")
        .map(|v| v.text())
        .filter(|s| !s.trim().is_empty())?;
    let dir = PathBuf::from(raw.trim());
    dir.is_dir().then_some(dir)
}

/// Built-in first, then every `.py` in the scripts folder, then None.
///
/// A script declares nothing — being a `.py` in that folder is the whole registration. Sorted by
/// file name so the list does not reshuffle between reads of the directory.
pub fn processor_items(cx: &App) -> Vec<DetailSelectItem> {
    let mut items = vec![DetailSelectItem {
        label: "Workbench preprocessor".into(),
        subtitle: "Built-in Rust importer".into(),
        value: PROCESSOR_BUILTIN.into(),
        divider_above: false,
    }];

    let mut scripts: Vec<PathBuf> = scripts_dir(cx)
        .and_then(|dir| std::fs::read_dir(dir).ok())
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("py")))
        .collect();
    scripts.sort();

    let mut first_script = true;
    for script in scripts {
        items.push(DetailSelectItem {
            label: script
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
                .into(),
            subtitle: "Preprocess script".into(),
            value: script.to_string_lossy().into_owned().into(),
            divider_above: std::mem::take(&mut first_script),
        });
    }

    items.push(DetailSelectItem {
        label: "None".into(),
        subtitle: "Use the source rows unchanged".into(),
        value: PROCESSOR_NONE.into(),
        divider_above: true,
    });
    items
}

/// A dropdown value back into the processor it names. Anything that is not one of the two
/// reserved words is a script path.
pub fn processor_for(value: &str) -> ProcessorChoice {
    match value {
        PROCESSOR_BUILTIN => ProcessorChoice::Builtin,
        PROCESSOR_NONE => ProcessorChoice::None,
        path => ProcessorChoice::Script(PathBuf::from(path)),
    }
}

/// Owned counterpart to [`workbench_integration::Processor`], which borrows its path. The
/// workspace holds one of these across the `await` in `process_metadata`.
#[derive(Debug, Clone)]
pub enum ProcessorChoice {
    Builtin,
    Script(PathBuf),
    None,
}

impl ProcessorChoice {
    pub fn as_processor(&self) -> workbench_integration::Processor<'_> {
        match self {
            Self::Builtin => workbench_integration::Processor::Builtin,
            Self::Script(path) => workbench_integration::Processor::Script(path),
            Self::None => workbench_integration::Processor::None,
        }
    }

    pub fn label(&self) -> SharedString {
        match self {
            Self::Builtin => "Workbench preprocessor".into(),
            Self::Script(path) => file_label(path),
            Self::None => "no processor".into(),
        }
    }

    /// Only a script runs Python, so only a script needs the Workbench environment.
    pub fn needs_workbench(&self) -> bool {
        matches!(self, Self::Script(_))
    }
}

fn file_label(path: &Path) -> SharedString {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
        .into()
}
