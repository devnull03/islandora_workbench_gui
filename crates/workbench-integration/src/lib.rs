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

use organise::process_google_sheets_and_maybe_generate_items;
use which::which;

pub use organise::ProcessResult;

pub use util::{format_stream_line, language_url_from_server_base, run_command_capture_stdout};

pub use stream::{StreamLine, spawn_command_streaming};

pub use config_builder::WorkbenchConfigHandler;

use crate::config_builder::Ready;

const WORKBENCH_INSTALL_PATH: &str = "C:\\Users\\Arnav\\Projects\\islandora_workbench";

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
    let (exe, base_args) = if workbench_info.use_uv {
        let uv = workbench_info.uv_path.as_deref()
            .and_then(|p| p.to_str())
            .unwrap_or("uv");
        (uv, vec!["run", "python", "workbench"])
    } else {
        let python = workbench_info.python_path.as_deref()
            .and_then(|p| p.to_str())
            .unwrap_or(if cfg!(target_os = "windows") { "python" } else { "python3" });
        (python, vec!["workbench"])
    };

    let mut cmd = Command::new(exe);
    
    cmd.args(base_args)
       .current_dir(&workbench_info.install_path)
       .arg("--config")
       .arg(config_file.path()); 
    
    Ok(cmd)
}

pub fn run_ingest_streaming(
    workbench_info: &WbInfo,
    config_file: &WorkbenchConfigHandler<Ready>,
    is_check: bool,
) -> anyhow::Result<Receiver<StreamLine>> {
    let mut cmd = build_workbench_command(workbench_info, config_file)?;
    if is_check {
        cmd.arg("--check");
    }
    spawn_command_streaming(cmd).map_err(Into::into)
}
