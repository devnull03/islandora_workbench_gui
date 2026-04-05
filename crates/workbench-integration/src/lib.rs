use std::path::PathBuf;
use std::process::Command;
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

pub fn run_command_capture_stdout(program: &str, args: &[&str]) -> std::io::Result<String> {
    let output = Command::new(program).args(args).output()?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
