//! Reference implementation for [`crate::run_command_streaming`].
//! Copy or call from your stub in `lib.rs` when you implement it.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;

pub enum StreamLine {
    Stdout(String),
    Stderr(String),
    Done(i32),
    Error(String),
}

/// Spawns a pre-built command and returns a receiver that streams stdout/stderr lines.
pub fn spawn_command_streaming(mut cmd: Command) -> std::io::Result<Receiver<StreamLine>> {
    let mut child = cmd
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
