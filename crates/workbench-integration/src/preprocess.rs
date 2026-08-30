//! Turning whatever the user has into the `metadata.csv` Workbench ingests.
//!
//! Two independent choices, which is the whole point of Stage 2 (mockup `2c`): **where the rows
//! come from** ([`InputSource`]) and **what transforms them** ([`Processor`]). Neither knows
//! about the other, so a site with a Google Sheet and a site with a CSV on disk run the same
//! preprocessor, and a site with no transform at all picks [`Processor::None`].
//!
//! See `docs/plans/stage-2-main-window.md`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use organise::{CsvModifier, process_csv_and_maybe_generate_items};

use crate::WbInfo;
use std::process::Command;

pub use organise::ProcessResult;

/// Where the rows come from.
#[derive(Debug, Clone, Copy)]
pub enum InputSource<'a> {
    /// A published Google Sheet, fetched as CSV.
    GoogleSheet(&'a str),
    /// A CSV already on this machine.
    CsvFile(&'a Path),
}

/// What turns the source rows into `metadata.csv`.
#[derive(Debug, Clone, Copy)]
pub enum Processor<'a> {
    /// The built-in Rust importer.
    Builtin,
    /// A `.py` file from the user's scripts folder.
    Script(&'a Path),
    /// Nothing — the source rows are already the metadata Workbench wants.
    None,
}

/// One preprocessing run: a source, a processor, and where the output belongs.
pub struct PreprocessJob<'a> {
    pub source: InputSource<'a>,
    pub processor: Processor<'a>,
    /// Workbench's `input_data` directory — `metadata.csv` lands here.
    pub output_dir: &'a Path,
    pub language_url: &'a str,
    /// The selected Workbench config, when one is selected. Passed to scripts that want it;
    /// optional because preprocessing usually happens before a config is chosen.
    pub config_file: Option<&'a Path>,
    /// The Python environment. Only [`Processor::Script`] needs it.
    pub workbench: Option<&'a WbInfo>,
}

/// What a processor produced.
pub struct PreprocessResult {
    pub metadata_csv: PathBuf,
    /// Row and validation counts. Only the built-in processor reports them — an external script
    /// is a black box, and `None` is what "we genuinely do not know" looks like.
    pub details: Option<ProcessResult>,
    /// Anything the script printed, for the log.
    pub output: Vec<String>,
}

pub fn run(job: PreprocessJob<'_>) -> Result<PreprocessResult> {
    std::fs::create_dir_all(job.output_dir)
        .with_context(|| format!("create output directory {}", job.output_dir.display()))?;

    let source_csv = acquire(job.source, job.output_dir)?;

    match job.processor {
        Processor::Builtin => builtin(&source_csv, job.output_dir, job.language_url),
        Processor::Script(script) => {
            let wb = job.workbench.context(
                "a preprocess script needs a Python environment; set the Workbench path in Settings",
            )?;
            script_run(script, wb, &source_csv, job.output_dir, job.config_file)
        }
        Processor::None => passthrough(&source_csv, job.output_dir),
    }
}

/// Get the source down to a local CSV path. A sheet is downloaded beside the output; a local
/// file is used where it lies, so nothing is copied for no reason.
fn acquire(source: InputSource<'_>, output_dir: &Path) -> Result<PathBuf> {
    match source {
        InputSource::CsvFile(path) => {
            anyhow::ensure!(path.is_file(), "no CSV at {}", path.display());
            Ok(path.to_path_buf())
        }
        InputSource::GoogleSheet(url) => {
            let csv = CsvModifier::fetch_google_sheets_csv(url)?;
            let dest = output_dir.join("source.csv");
            std::fs::write(&dest, csv).with_context(|| format!("write {}", dest.display()))?;
            Ok(dest)
        }
    }
}

fn builtin(source_csv: &Path, output_dir: &Path, language_url: &str) -> Result<PreprocessResult> {
    let details = process_csv_and_maybe_generate_items(
        source_csv.to_string_lossy().as_ref(),
        Some("metadata.csv"),
        Some(output_dir.to_string_lossy().as_ref()),
        &[],
        &[],
        Some(language_url),
        false,
        None,
        None,
    )?;
    Ok(PreprocessResult {
        metadata_csv: PathBuf::from(&details.processed_output_path),
        details: Some(details),
        output: Vec::new(),
    })
}

/// The whole contract for an external script, and deliberately no more than this: it is invoked
///
/// ```text
/// python <script> --input <source.csv> --output-dir <dir> [--config <config.yml>]
/// ```
///
/// and must write a CSV. If its last line of stdout is a path that exists, that is the result;
/// otherwise `<dir>/metadata.csv` is. Scripts declare nothing and register nothing — dropping a
/// `.py` in the scripts folder is the entire installation step.
fn script_run(
    script: &Path,
    wb: &WbInfo,
    source_csv: &Path,
    output_dir: &Path,
    config_file: Option<&Path>,
) -> Result<PreprocessResult> {
    anyhow::ensure!(script.is_file(), "no script at {}", script.display());

    let mut cmd = wb.python_command();
    cmd.arg(script)
        .arg("--input")
        .arg(source_csv)
        .arg("--output-dir")
        .arg(output_dir);
    if let Some(config) = config_file {
        cmd.arg("--config").arg(config);
    }

    let stdout =
        capture(cmd).with_context(|| format!("run preprocess script {}", script.display()))?;

    let output: Vec<String> = stdout.lines().map(str::to_string).collect();
    let reported = output
        .iter()
        .rev()
        .map(|line| PathBuf::from(line.trim()))
        .find(|path| path.is_file());
    let metadata_csv = reported.unwrap_or_else(|| output_dir.join("metadata.csv"));

    anyhow::ensure!(
        metadata_csv.is_file(),
        "{} finished but wrote no CSV — expected {} or a path on its last line of output",
        script.display(),
        output_dir.join("metadata.csv").display()
    );

    Ok(PreprocessResult {
        metadata_csv,
        details: None,
        output,
    })
}

/// No transform: the source rows are already what Workbench wants.
fn passthrough(source_csv: &Path, output_dir: &Path) -> Result<PreprocessResult> {
    let dest = output_dir.join("metadata.csv");
    if source_csv != dest {
        std::fs::copy(source_csv, &dest)
            .with_context(|| format!("copy {} to {}", source_csv.display(), dest.display()))?;
    }
    Ok(PreprocessResult {
        metadata_csv: dest,
        details: None,
        output: Vec::new(),
    })
}

/// Run to completion, and treat a non-zero exit as an error carrying stderr — unlike
/// [`crate::run_command_capture_stdout`], which is for probes where failure is expected.
fn capture(mut cmd: Command) -> Result<String> {
    crate::util::apply_no_window(&mut cmd);
    let out = cmd.output()?;
    anyhow::ensure!(
        out.status.success(),
        "exited with {}: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
#[cfg(test)]
mod tests {
    use super::*;

    /// The two branches that touch no network: a local CSV through `None` must land as
    /// `metadata.csv`, and a missing source must fail rather than produce an empty one.
    #[test]
    fn csv_file_through_no_processor_becomes_metadata_csv() {
        let dir = std::env::temp_dir().join("wbgui-preprocess-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("rows.csv");
        std::fs::write(&src, "id,title\n1,A\n").unwrap();
        let out = dir.join("input_data");

        let result = run(PreprocessJob {
            source: InputSource::CsvFile(&src),
            processor: Processor::None,
            output_dir: &out,
            language_url: "",
            config_file: None,
            workbench: None,
        })
        .unwrap();

        assert_eq!(result.metadata_csv, out.join("metadata.csv"));
        assert_eq!(
            std::fs::read_to_string(&result.metadata_csv).unwrap(),
            "id,title\n1,A\n"
        );
        assert!(result.details.is_none());

        let missing = dir.join("nope.csv");
        assert!(
            run(PreprocessJob {
                source: InputSource::CsvFile(&missing),
                processor: Processor::None,
                output_dir: &out,
                language_url: "",
                config_file: None,
                workbench: None,
            })
            .is_err()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
