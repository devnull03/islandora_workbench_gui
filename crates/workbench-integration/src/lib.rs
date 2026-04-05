//! Islandora Workbench process integration.
#![allow(dead_code)] // WbInfo / install path reserved for upcoming ingest wiring

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use which::which;

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
