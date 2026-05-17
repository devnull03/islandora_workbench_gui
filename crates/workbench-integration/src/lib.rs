//! Islandora Workbench process integration.
#![allow(dead_code)] // WbInfo / install path reserved for upcoming ingest wiring

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;
use which::which;

pub use organise::ProcessResult;
use organise::process_google_sheets_and_maybe_generate_items;

const WORKBENCH_INSTALL_PATH: &str = "C:\\Users\\Arnav\\Projects\\islandora_workbench";

struct WbInfo {
    install_path: PathBuf,
    python_path: Option<PathBuf>,
    uv_path: Option<PathBuf>,
    use_uv: bool,
}

struct IngestInfo {
    config_path: PathBuf,
}

impl WbInfo {
    pub fn new(workbench_path: PathBuf, use_uv: bool) -> Self {
        Self {
            install_path: workbench_path,
            python_path: which("python").ok(),
            uv_path: which("uv").ok(),
            use_uv,
        }
    }
}

/// Builds the Drupal JSON endpoint URL for language code mapping from a site base URL.
pub fn language_url_from_server_base(server_url: &str) -> String {
    let base = server_url.trim().trim_end_matches('/');
    format!("{}/lang-code?_format=json", base)
}

/// Runs the Google Sheet preprocessor (metadata CSV) and optional items generation under
/// `input_data_dir` (created if missing). Output defaults to `metadata.csv` in that folder.
pub fn process_google_sheet_metadata(
    sheet_url: &str,
    input_data_dir: &Path,
    language_url: &str,
    node_id: &str,
) -> anyhow::Result<ProcessResult> {
    std::fs::create_dir_all(input_data_dir).map_err(|e| {
        anyhow::anyhow!(
            "create output directory {}: {}",
            input_data_dir.display(),
            e
        )
    })?;
    let out_dir = input_data_dir.to_string_lossy();
    process_google_sheets_and_maybe_generate_items(
        sheet_url,
        Some("metadata.csv"),
        Some(out_dir.as_ref()),
        &[],
        &[],
        Some(language_url),
        true,
        None,
        Some(node_id),
    )
}

pub fn run_command_capture_stdout(program: &str, args: &[&str]) -> std::io::Result<String> {
    let output = Command::new(program).args(args).output()?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub enum StreamLine {
    Stdout(String),
    Stderr(String),
    Done(i32),
    Error(String),
}

/// How a [`StreamLine`] should appear in the workspace log (single line).
pub fn format_stream_line(line: &StreamLine) -> String {
    match line {
        StreamLine::Stdout(s) => s.clone(),
        StreamLine::Stderr(s) => format!("[STDERR] {}", s),
        StreamLine::Done(code) => format!("[INFO] Process exited with code: {}", code),
        StreamLine::Error(e) => format!("[ERROR] {}", e),
    }
}

/// Inputs for an ingest run (check or full). Used when invoking the Workbench CLI.
pub struct IngestParams<'a> {
    pub check: bool,
    pub ingest_files_dir: &'a Path,
    pub task_label: &'a str,
    pub server_label: &'a str,
}

/// Runs a Workbench ingest and streams [`StreamLine`]s (same channel shape as [`run_command_streaming`]).
pub fn run_ingest_streaming(params: IngestParams<'_>) -> Receiver<StreamLine> {
    let IngestParams { check, .. } = params;
    run_placeholder_ingest_streaming(check)
}

const PLACEHOLDER_INGEST_STEPS: &[(u64, &str)] = &[
    (300, "[INFO] Loading configuration..."),
    (500, "[INFO] Config loaded from: /path/to/config.yml"),
    (400, "[INFO] Validating input files..."),
    (600, "[WARN] Found 3 files with missing metadata"),
    (800, "[INFO] Processing batch 1 of 5..."),
    (500, "[INFO] Successfully processed 25 items"),
    (700, "[INFO] Processing batch 2 of 5..."),
    (400, "[INFO] Successfully processed 25 items"),
    (600, "[INFO] Processing batch 3 of 5..."),
    (
        900,
        "[ERROR] Failed to process item ID: 12345 - Connection timeout",
    ),
    (500, "[INFO] Retrying failed items..."),
    (700, "[INFO] Processing batch 4 of 5..."),
    (400, "[INFO] Successfully processed 25 items"),
    (600, "[INFO] Processing batch 5 of 5..."),
    (500, "[INFO] Successfully processed 24 items"),
    (
        300,
        "[INFO] Processing complete. Total: 125 items, Success: 124, Failed: 1",
    ),
];

fn run_placeholder_ingest_streaming(check: bool) -> Receiver<StreamLine> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let start = if check {
            "[INFO] Starting ingest (check mode)..."
        } else {
            "[INFO] Starting ingest..."
        };
        if tx.send(StreamLine::Stdout(start.to_string())).is_err() {
            return;
        }
        for &(delay_ms, line) in PLACEHOLDER_INGEST_STEPS {
            thread::sleep(Duration::from_millis(delay_ms));
            if tx.send(StreamLine::Stdout(line.to_string())).is_err() {
                return;
            }
        }
        let _ = tx.send(StreamLine::Done(0));
    });
    rx
}

/// Spawns a command and returns a receiver that streams stdout/stderr lines.
pub fn run_command_streaming(
    program: &str,
    args: &[&str],
) -> std::io::Result<Receiver<StreamLine>> {
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().expect("Failed to capture stdout");
    let stderr = child.stderr.take().expect("Failed to capture stderr");

    let (tx, rx) = mpsc::channel();

    let tx_stdout = tx.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    if tx_stdout.send(StreamLine::Stdout(line)).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = tx_stdout.send(StreamLine::Error(e.to_string()));
                    break;
                }
            }
        }
    });

    let tx_stderr = tx.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    if tx_stderr.send(StreamLine::Stderr(line)).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = tx_stderr.send(StreamLine::Error(e.to_string()));
                    break;
                }
            }
        }
    });

    thread::spawn(move || {
        let status = child.wait();
        match status {
            Ok(s) => {
                let _ = tx.send(StreamLine::Done(s.code().unwrap_or(-1)));
            }
            Err(e) => {
                let _ = tx.send(StreamLine::Error(e.to_string()));
            }
        }
    });

    Ok(rx)
}
