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
    /// An Excel or OpenDocument spreadsheet on this machine, and the worksheet to read (the
    /// first when `None`) — Workbench's own Excel input (`excel_worksheet`), but converted
    /// here, so the built-in preprocessor and scripts get a CSV like any other source.
    Spreadsheet {
        path: &'a Path,
        worksheet: Option<&'a str>,
    },
}

/// Whether `path` names a spreadsheet the [`InputSource::Spreadsheet`] source reads.
pub fn is_spreadsheet(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "xlsx" | "xlsm" | "xls" | "ods"
        )
    })
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
        InputSource::Spreadsheet { path, worksheet } => {
            let csv = spreadsheet_to_csv(path, worksheet)?;
            let dest = output_dir.join("source.csv");
            std::fs::write(&dest, csv).with_context(|| format!("write {}", dest.display()))?;
            Ok(dest)
        }
        InputSource::GoogleSheet(url) => {
            let csv = fetch_sheet(url)?;
            let dest = output_dir.join("source.csv");
            std::fs::write(&dest, csv).with_context(|| format!("write {}", dest.display()))?;
            Ok(dest)
        }
    }
}

/// One worksheet as CSV text: every row, cells quoted where CSV needs it. Numbers that are whole
/// are written without a trailing `.0`, as a person typing them into a CSV would.
fn spreadsheet_to_csv(path: &Path, worksheet: Option<&str>) -> Result<String> {
    use calamine::{Data, DataType as _, Reader, open_workbook_auto};
    let mut book = open_workbook_auto(path)
        .with_context(|| format!("Couldn't open the spreadsheet {}", path.display()))?;
    let names = book.sheet_names().to_vec();
    let name = match worksheet.map(str::trim).filter(|w| !w.is_empty()) {
        Some(wanted) => names
            .iter()
            .find(|n| n.as_str() == wanted)
            .cloned()
            .with_context(|| {
                format!(
                    "{} has no worksheet called \"{wanted}\" (it has: {})",
                    path.display(),
                    names.join(", ")
                )
            })?,
        None => names
            .first()
            .cloned()
            .with_context(|| format!("{} has no worksheets", path.display()))?,
    };
    let range = book
        .worksheet_range(&name)
        .with_context(|| format!("Couldn't read the worksheet \"{name}\""))?;
    let mut out = String::new();
    for row in range.rows() {
        let cells: Vec<String> = row
            .iter()
            .map(|cell| {
                let text = match cell {
                    Data::Empty => String::new(),
                    Data::Float(f) if f.fract() == 0.0 && f.abs() < 1e15 => {
                        format!("{}", *f as i64)
                    }
                    // A date cell is a day count underneath; write it as a date (EDTF and
                    // Drupal date fields read `yyyy-mm-dd`), with the time only when it has one.
                    // A duration (`[h]:mm`) is a span, not a date.
                    Data::DateTime(d) if d.is_duration() => match d.as_duration() {
                        Some(span) => duration_text(span.num_seconds()),
                        None => cell.to_string(),
                    },
                    Data::DateTime(d) => match cell.as_datetime() {
                        // Under one day: a time of day with no date (its "date" is Excel's
                        // day zero, which calamine reports as 1899-12-30 or -31).
                        Some(dt) if (0.0..1.0).contains(&d.as_f64()) => {
                            dt.format("%H:%M:%S").to_string()
                        }
                        Some(dt) if dt.time() == chrono::NaiveTime::MIN => {
                            dt.format("%Y-%m-%d").to_string()
                        }
                        Some(dt) => dt.format("%Y-%m-%dT%H:%M:%S").to_string(),
                        None => cell.to_string(),
                    },
                    Data::DateTimeIso(text) | Data::DurationIso(text) => text.clone(),
                    // A formula error (`#DIV/0!`) is not a value; leave the cell empty.
                    Data::Error(_) => String::new(),
                    other => other.to_string(),
                };
                csv_field(&text)
            })
            .collect();
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    Ok(out)
}

/// A CSV field, quoted when it holds a comma, a quote or a line break.
fn csv_field(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

/// The sheet's CSV export, for the worksheet the URL names. Workbench's `google_sheets_gid`
/// picks a worksheet; here the link does, the way Google writes it (`#gid=…` or `?gid=…`) —
/// without it the export is always the first worksheet.
fn fetch_sheet(url: &str) -> Result<String> {
    fetch_csv(&sheet_export_url(url)?)
}

/// Download `export`, which must answer with a CSV.
fn fetch_csv(export: &str) -> Result<String> {
    let response = ureq::get(export)
        .timeout(std::time::Duration::from_secs(60))
        .call()
        .with_context(|| format!("Couldn't fetch the sheet from {export}"))?;
    // A sheet that is not shared answers with Google's sign-in page, not a CSV.
    if !response.content_type().contains("csv") {
        anyhow::bail!(
            "The sheet did not come back as a CSV — share it as \"Anyone with the link can view\" \
             (or publish it) and try again."
        );
    }
    response
        .into_string()
        .context("The sheet's CSV could not be read as text")
}

fn sheet_export_url(url: &str) -> Result<String> {
    let base = CsvModifier::google_sheets_to_csv_url(url)?;
    Ok(match sheet_gid(url) {
        Some(gid) => format!("{base}&gid={gid}"),
        None => base,
    })
}

/// The worksheet id in a sheet link: `gid=` in the query or the fragment.
fn sheet_gid(url: &str) -> Option<String> {
    url.split(['?', '#', '&'])
        .skip(1)
        .find_map(|part| part.strip_prefix("gid="))
        .map(|gid| {
            gid.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
        })
        .filter(|gid| !gid.is_empty())
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
/// A span of seconds as `h:mm:ss`; a negative one (1904-based workbooks) as `-h:mm:ss`.
fn duration_text(secs: i64) -> String {
    let sign = if secs < 0 { "-" } else { "" };
    let secs = secs.unsigned_abs();
    format!(
        "{sign}{}:{:02}:{:02}",
        secs / 3600,
        secs / 60 % 60,
        secs % 60
    )
}

#[cfg(test)]
mod tests {
    /// Answer one HTTP request on a local port with `content_type` and `body`.
    fn serve_once(content_type: &'static str, body: &'static str) -> String {
        use std::io::{Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        });
        format!("http://{address}/export?format=csv")
    }

    #[test]
    fn a_sheet_must_come_back_as_csv() {
        let csv = serve_once("text/csv; charset=utf-8", "id,title\n1,One\n");
        assert_eq!(super::fetch_csv(&csv).unwrap(), "id,title\n1,One\n");
        // An unshared sheet answers with Google's sign-in page.
        let page = serve_once("text/html; charset=utf-8", "<html>Sign in</html>");
        let error = super::fetch_csv(&page).unwrap_err().to_string();
        assert!(error.contains("did not come back as a CSV"), "{error}");
    }

    #[test]
    fn a_link_without_a_worksheet_exports_the_first() {
        let first = "https://docs.google.com/spreadsheets/d/abc123/edit";
        assert!(!super::sheet_export_url(first).unwrap().contains("gid"));
        assert_eq!(super::sheet_gid("x?usp=sharing&gid=7#x"), Some("7".into()));
        assert_eq!(super::sheet_gid("x#gid="), None);
    }

    #[test]
    fn negative_durations_keep_one_sign() {
        assert_eq!(super::duration_text(-5400), "-1:30:00");
        assert_eq!(super::duration_text(37 * 3600 + 5), "37:00:05");
    }

    /// A real workbook (made with openpyxl): the named worksheet, a date, a comma, a number.
    #[test]
    fn spreadsheets_convert_like_a_person_would_type_them() {
        let book =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/items.xlsx");
        let csv = super::spreadsheet_to_csv(&book, Some("Items")).unwrap();
        assert_eq!(
            csv,
            "id,title,date\n1,\"First, with comma\",2024-03-05\n2,Second,2024-03-05T14:30:00\n"
        );
        // No worksheet named: the first one.
        assert_eq!(
            super::spreadsheet_to_csv(&book, None).unwrap(),
            "ignore me\n"
        );
        assert!(super::spreadsheet_to_csv(&book, Some("Nope")).is_err());
        // A time of day has no date; a duration is a span.
        assert_eq!(
            super::spreadsheet_to_csv(&book, Some("Times")).unwrap(),
            "at,took\n14:30:00,26:05:00\n"
        );
    }

    #[test]
    fn csv_fields_are_quoted_only_when_needed() {
        assert_eq!(super::csv_field("plain"), "plain");
        assert_eq!(super::csv_field("a, b"), "\"a, b\"");
        assert_eq!(super::csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert!(super::is_spreadsheet(std::path::Path::new("/x/Items.XLSX")));
        assert!(!super::is_spreadsheet(std::path::Path::new("/x/items.csv")));
    }

    #[test]
    fn a_sheet_link_keeps_its_worksheet() {
        let base = "https://docs.google.com/spreadsheets/d/abc123/edit";
        assert_eq!(
            super::sheet_export_url(&format!("{base}#gid=1867618389")).unwrap(),
            "https://docs.google.com/spreadsheets/d/abc123/export?format=csv&gid=1867618389"
        );
        assert_eq!(
            super::sheet_export_url(&format!("{base}?usp=sharing&gid=7")).unwrap(),
            "https://docs.google.com/spreadsheets/d/abc123/export?format=csv&gid=7"
        );
        assert_eq!(
            super::sheet_export_url(base).unwrap(),
            "https://docs.google.com/spreadsheets/d/abc123/export?format=csv"
        );
    }

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
