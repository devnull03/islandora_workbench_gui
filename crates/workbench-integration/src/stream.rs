use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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

/// Stops a running child. Cloneable, so the UI can hold one while the stream is drained.
#[derive(Clone, Debug)]
pub struct ProcessControl {
    pid: u32,
    exited: Arc<AtomicBool>,
}

impl ProcessControl {
    /// Ask the run to stop the way Workbench documents ("press ctrl-c"): SIGINT to the whole
    /// process group, so `uv run` and the Python it started both get it, and Workbench logs
    /// "exiting after receiving ctrl c" and leaves its rollback/map files consistent.
    ///
    /// Windows has no SIGINT for a console-less child, so there it is [`Self::kill`].
    pub fn interrupt(&self) {
        if self.exited.load(Ordering::SeqCst) {
            return;
        }
        #[cfg(unix)]
        // SAFETY: `kill` takes plain integers; a negative pid addresses the process group the
        // child leads (it was spawned with `process_group(0)`).
        unsafe {
            libc::kill(-(self.pid as i32), libc::SIGINT);
        }
        #[cfg(not(unix))]
        self.kill();
    }

    /// Stop the run now — for a second press of Cancel, when SIGINT was not enough.
    pub fn kill(&self) {
        if self.exited.load(Ordering::SeqCst) {
            return;
        }
        #[cfg(unix)]
        // SAFETY: as in `interrupt`.
        unsafe {
            libc::kill(-(self.pid as i32), libc::SIGKILL);
        }
        #[cfg(not(unix))]
        {
            let mut taskkill = Command::new("taskkill");
            taskkill.args(["/PID", &self.pid.to_string(), "/T", "/F"]);
            crate::util::apply_no_window(&mut taskkill);
            // Fire and forget: waiting would block the UI thread that pressed Cancel.
            let _ = taskkill.spawn();
        }
    }

    /// Whether the child has exited.
    pub fn has_exited(&self) -> bool {
        self.exited.load(Ordering::SeqCst)
    }

    /// Stop the run if it is still going — for when its owner goes away (the window closes or
    /// the app quits). A run in its own process group no longer gets the terminal's SIGHUP, so
    /// without this it would outlive the app.
    pub fn stop_if_running(&self) {
        if !self.has_exited() {
            self.kill();
        }
    }
}

/// A child being streamed: its output, its stdin, and the means to stop it.
pub struct StreamingProcess {
    pub lines: Receiver<StreamLine>,
    pub stdin: StdinSink,
    pub control: ProcessControl,
}

/// Spawns a pre-built command and returns a receiver that streams stdout/stderr lines,
/// plus a [`StdinSink`] to write answers back to the child process.
pub fn spawn_command_streaming(mut cmd: Command) -> std::io::Result<StreamingProcess> {
    // Stop a blank console window flashing on Windows when the GUI spawns workbench (we already
    // pipe stdin/stdout/stderr, so the child never needs a console of its own).
    crate::util::apply_no_window(&mut cmd);
    // Its own process group, so Cancel can signal `uv` and the Python under it together.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
        // A signal ignored in this process stays ignored in the child, and the app is often
        // started that way (a background job of a non-interactive shell, some launchers). A
        // Python started with SIGINT ignored never installs its KeyboardInterrupt handler, so
        // Cancel would do nothing. Restore the default in the child before it execs.
        //
        // SAFETY: `signal` is async-signal-safe, which is all `pre_exec` requires.
        unsafe {
            cmd.pre_exec(|| {
                libc::signal(libc::SIGINT, libc::SIG_DFL);
                Ok(())
            });
        }
    }
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
    let stdout_reader = thread::spawn(move || {
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
                        if is_yn_prompt(&s) {
                            log::debug!("[stream] prompt detected: {s:?}");
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
    let stderr_reader = thread::spawn(move || {
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

    let exited = Arc::new(AtomicBool::new(false));
    let control = ProcessControl {
        pid: child.id(),
        exited: exited.clone(),
    };

    // Wait thread: sends Done/Error when the child process exits — after both readers have
    // drained, because Workbench's last lines ("Contact sheet is at …", "CSV template saved at
    // …") are printed just before it exits and the UI stops reading at Done.
    thread::spawn(move || {
        let status = child.wait();
        exited.store(true, Ordering::SeqCst);
        let _ = stdout_reader.join();
        let _ = stderr_reader.join();
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

    Ok(StreamingProcess {
        lines: rx,
        stdin: StdinSink { tx: stdin_tx },
        control,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Cancel sends SIGINT to the run's process group; a shell that traps it proves it arrived
    /// and that the run ends with its own exit code rather than being killed.
    /// The app may itself have SIGINT ignored (started in the background); the child must not
    /// inherit that, or Cancel does nothing.
    #[test]
    fn interrupt_works_even_when_this_process_ignores_sigint() {
        // SAFETY: process-wide, but nothing in the test harness relies on SIGINT, and it is
        // restored before the assertion.
        let previous = unsafe { libc::signal(libc::SIGINT, libc::SIG_IGN) };
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo ready; exec sleep 30"]);
        let process = spawn_command_streaming(cmd);
        unsafe {
            libc::signal(libc::SIGINT, previous);
        }
        let process = process.unwrap();
        assert!(matches!(
            process.lines.recv_timeout(Duration::from_secs(5)),
            Ok(StreamLine::Stdout(_))
        ));
        process.control.interrupt();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut code = None;
        while Instant::now() < deadline && code.is_none() {
            if let Ok(StreamLine::Done(c)) = process.lines.recv_timeout(Duration::from_secs(5)) {
                code = Some(c);
            }
        }
        // Killed by SIGINT: no exit code, which the stream reports as -1.
        assert_eq!(code, Some(-1));
    }

    /// Done must come after the last line, however close to exit it was printed.
    #[test]
    fn the_last_line_arrives_before_done() {
        for _ in 0..20 {
            let mut cmd = Command::new("sh");
            cmd.args([
                "-c",
                "printf 'Contact sheet is at /x/contact_sheet.htm.\\n'; exit 0",
            ]);
            let process = spawn_command_streaming(cmd).unwrap();
            let mut saw_line = false;
            loop {
                match process.lines.recv_timeout(Duration::from_secs(5)).unwrap() {
                    StreamLine::Stdout(_) => saw_line = true,
                    StreamLine::Done(_) => break,
                    _ => {}
                }
            }
            assert!(saw_line);
        }
    }

    #[test]
    fn interrupt_reaches_the_child_and_it_exits() {
        let mut cmd = Command::new("sh");
        cmd.args([
            "-c",
            "trap 'echo interrupted; exit 130' INT; echo ready; while :; do sleep 0.05; done",
        ]);
        let process = spawn_command_streaming(cmd).unwrap();
        // Wait until the trap is installed.
        match process.lines.recv_timeout(Duration::from_secs(5)).unwrap() {
            StreamLine::Stdout(line) => assert_eq!(line, "ready"),
            _ => panic!("expected the ready line"),
        }
        process.control.interrupt();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut saw_trap = false;
        let mut code = None;
        while Instant::now() < deadline && code.is_none() {
            match process.lines.recv_timeout(Duration::from_secs(5)) {
                Ok(StreamLine::Stdout(line)) if line == "interrupted" => saw_trap = true,
                Ok(StreamLine::Done(c)) => code = Some(c),
                Ok(_) => {}
                Err(_) => break,
            }
        }
        assert!(saw_trap);
        assert_eq!(code, Some(130));
        assert!(process.control.has_exited());
    }
}
