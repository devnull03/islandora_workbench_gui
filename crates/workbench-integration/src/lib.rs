//! Islandora Workbench process integration.
//!
//! **Learning layout:** types and main API stubs live here; utilities and reference
//! implementations are in sibling modules (`util`, `stream`, `sheet`, `placeholder`).

mod stream;
mod util;
mod config_builder;

use std::path::Path;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use organise::process_google_sheets_and_maybe_generate_items;
use which::which;

pub use organise::ProcessResult;

pub use util::{
    format_stream_line, language_url_from_server_base, run_command_capture_stdout,
};

pub use stream::{spawn_command_streaming, StreamLine};

pub use config_builder::{WorkbenchConfigHandler};

use crate::config_builder::Ready;

const WORKBENCH_INSTALL_PATH: &str = "C:\\Users\\Arnav\\Projects\\islandora_workbench";

struct WbInfo {
    install_path: PathBuf,
    python_path: Option<PathBuf>,
    uv_path: Option<PathBuf>,
    use_uv: bool,
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

/// Runs a Workbench ingest and streams [`StreamLine`]s (same channel shape as [`run_command_streaming`]).
///
/// Replace the body with your implementation. Placeholder: [`placeholder::run_placeholder_ingest_streaming`].
pub fn run_ingest_streaming(workbench_info: &WbInfo, config_file: &WorkbenchConfigHandler<Ready>) -> Receiver<StreamLine> {
    todo!()
}
