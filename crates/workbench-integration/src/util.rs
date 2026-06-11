use std::process::Command;

use crate::StreamLine;

/// Builds the Drupal JSON endpoint URL for language code mapping from a site base URL.
pub fn language_url_from_server_base(server_url: &str) -> String {
    let base = server_url.trim().trim_end_matches('/');
    format!("{}/lang-code?_format=json", base)
}

/// How a [`StreamLine`] should appear in the workspace log (single line).
pub fn format_stream_line(line: &StreamLine) -> String {
    match line {
        StreamLine::Stdout(s) => s.clone(),
        StreamLine::Stderr(s) => format!("[STDERR] {}", s),
        StreamLine::Done(code) => format!("[INFO] Process exited with code: {}", code),
        StreamLine::Error(e) => format!("[ERROR] {}", e),
        StreamLine::InputRequired(s) => format!("[PROMPT] {}", s),
    }
}

pub fn run_command_capture_stdout(program: &str, args: &[&str]) -> std::io::Result<String> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    apply_no_window(&mut cmd);
    let output = cmd.output()?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Suppress the console window Windows would otherwise allocate for a console child process when
/// the parent (the GUI, which has no console of its own in release builds) spawns it. Without this,
/// every workbench run / server ping flashes a blank terminal. No-op off Windows.
pub(crate) fn apply_no_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
    }
}
