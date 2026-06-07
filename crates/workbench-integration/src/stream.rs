use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;

pub enum StreamLine {
    Stdout(String),
    Stderr(String),
    Done(i32),
    Error(String),
    /// A y/n prompt emitted without a trailing newline; the child is blocking for input.
    InputRequired(String),
}

/// A cloneable handle for writing answers back to the child process's stdin.
#[derive(Clone)]
pub struct StdinSink {
    tx: SyncSender<String>,
}

impl StdinSink {
    pub fn send(&self, answer: &str) {
        let _ = self.tx.send(format!("{}\n", answer));
    }
}

fn is_yn_prompt(s: &str) -> bool {
    let lower = s.to_lowercase();
    lower.contains("[y/n]")
        || lower.contains("[yes/no]")
        || lower.contains("[y/n]:")
        || lower.contains("(y/n)")
        || lower.contains("(yes/no)")
        || lower.contains("(y/n):")
}

/// Spawns a pre-built command and returns a receiver that streams stdout/stderr lines,
/// plus a [`StdinSink`] to write answers back to the child process.
pub fn spawn_command_streaming(
    mut cmd: Command,
) -> std::io::Result<(Receiver<StreamLine>, StdinSink)> {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().expect("Failed to capture stdout");
    let stderr = child.stderr.take().expect("Failed to capture stderr");
    let stdin = child.stdin.take().expect("Failed to capture stdin");

    let (tx, rx) = mpsc::channel();
    let (stdin_tx, stdin_rx) = mpsc::sync_channel::<String>(4);

    // Stdout: byte-by-byte reader to detect y/n prompts that don't end with '\n'.
    let tx_stdout = tx.clone();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut buf: Vec<u8> = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            match reader.read(&mut byte) {
                Ok(0) => {
                    if !buf.is_empty() {
                        let s = String::from_utf8_lossy(&buf).into_owned();
                        let _ = tx_stdout.send(StreamLine::Stdout(s));
                    }
                    break;
                }
                Ok(_) => {
                    if byte[0] == b'\n' {
                        if buf.last() == Some(&b'\r') {
                            buf.pop();
                        }
                        let s = String::from_utf8_lossy(&buf).into_owned();
                        if tx_stdout.send(StreamLine::Stdout(s)).is_err() {
                            break;
                        }
                        buf.clear();
                    } else {
                        buf.push(byte[0]);
                        let s = String::from_utf8_lossy(&buf);
                        eprintln!("[stdout-partial] {:?}", s);
                        if is_yn_prompt(&s) {
                            eprintln!("\n[PROMPT DETECTED] {:?}", s);
                            if tx_stdout
                                .send(StreamLine::InputRequired(s.into_owned()))
                                .is_err()
                            {
                                break;
                            }
                            buf.clear();
                        }
                    }
                }
                Err(e) => {
                    let _ = tx_stdout.send(StreamLine::Error(e.to_string()));
                    break;
                }
            }
        }
    });

    // Stderr: line-by-line (prompts never come from stderr).
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

    // Wait thread: sends Done/Error when the child process exits.
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

    // Stdin writer thread: forwards answers from the UI to the child process.
    thread::spawn(move || {
        let mut stdin = stdin;
        for answer in stdin_rx {
            if stdin.write_all(answer.as_bytes()).is_err() {
                break;
            }
        }
    });

    Ok((rx, StdinSink { tx: stdin_tx }))
}
