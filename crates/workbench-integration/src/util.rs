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
    let output = Command::new(program).args(args).output()?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
