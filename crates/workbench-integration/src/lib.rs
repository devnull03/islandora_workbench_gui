//! Islandora Workbench process integration.
//!
//! **Learning layout:** types and main API stubs live here; utilities and reference
//! implementations are in sibling modules (`util`, `stream`, `sheet`, `placeholder`).

mod config_builder;
mod stream;
mod util;

use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::Receiver;

pub struct Credentials {
    pub username: String,
    pub password: String,
}

pub fn read_credentials(path: &Path) -> anyhow::Result<Credentials> {
    #[derive(serde::Deserialize)]
    struct CredFile {
        username: String,
        password: String,
    }
    let f = std::fs::File::open(path)?;
    let c: CredFile = serde_yaml::from_reader(f)?;
    Ok(Credentials {
        username: c.username,
        password: c.password,
    })
}

use organise::process_google_sheets_and_maybe_generate_items;
use which::which;

pub use organise::ProcessResult;

pub use util::{format_stream_line, language_url_from_server_base, run_command_capture_stdout};

pub use stream::{StdinSink, StreamLine, spawn_command_streaming};

pub use config_builder::WorkbenchConfigHandler;

use crate::config_builder::Ready;

pub struct WbInfo {
    pub install_path: PathBuf,
    pub python_path: Option<PathBuf>,
    pub uv_path: Option<PathBuf>,
    pub use_uv: bool,
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

pub fn build_workbench_command(
    workbench_info: &WbInfo,
    config_file: &WorkbenchConfigHandler<Ready>,
) -> anyhow::Result<Command> {
    let mut cmd = if workbench_info.use_uv {
        let uv = workbench_info
            .uv_path
            .as_deref()
            .and_then(|p| p.to_str())
            .unwrap_or("uv");
        let mut c = Command::new(uv);
        c.args(["run", "python", "-u", "workbench"]);
        c
    } else {
        let python = workbench_info
            .python_path
            .as_deref()
            .and_then(|p| p.to_str())
            .unwrap_or(if cfg!(target_os = "windows") { "python" } else { "python3" });
        let mut c = Command::new(python);
        c.args(["-u", "workbench"]);
        c
    };

    cmd.current_dir(&workbench_info.install_path)
        .env("PYTHONUNBUFFERED", "1")
        .arg("--config")
        .arg(config_file.path());

    Ok(cmd)
}

pub fn run_ingest_streaming(
    workbench_info: &WbInfo,
    config_file: &WorkbenchConfigHandler<Ready>,
    is_check: bool,
) -> anyhow::Result<(Receiver<StreamLine>, StdinSink)> {
    let mut cmd = build_workbench_command(workbench_info, config_file)?;
    if is_check {
        cmd.arg("--check");
    }
    println!(
        "[workbench] cwd: {:?}\n[workbench] cmd: {} {}",
        cmd.get_current_dir(),
        cmd.get_program().to_string_lossy(),
        cmd.get_args()
            .map(|a| format!("{:?}", a))
            .collect::<Vec<_>>()
            .join(" "),
    );
    spawn_command_streaming(cmd).map_err(Into::into)
}
