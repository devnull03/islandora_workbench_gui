//! Islandora Workbench process integration.
//!
//! **Learning layout:** types and main API stubs live here; utilities and reference
//! implementations are in sibling modules (`util`, `stream`, `sheet`, `placeholder`).

pub mod catalog;
mod config_builder;
mod provision;
mod resolve;
mod stream;
mod util;
pub mod validate;

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

use organise::{
    process_csv_and_maybe_generate_items, process_google_sheets_and_maybe_generate_items,
};
use which::which;

pub use organise::ProcessResult;

pub use util::{format_stream_line, language_url_from_server_base, run_command_capture_stdout};

pub use stream::{StdinSink, StreamLine, spawn_command_streaming};

pub use config_builder::{APP_SUPPLIED, ConfigDraft, WorkbenchConfigHandler};

pub use provision::provision_workbench;

pub use resolve::RegistryInstall;

use crate::config_builder::Ready;

pub struct WbInfo {
    pub install_path: PathBuf,
    pub python_path: Option<PathBuf>,
    pub uv_path: Option<PathBuf>,
    pub use_uv: bool,
}

impl WbInfo {
    /// `uv_path` is the caller-resolved uv executable (settings → installer registry). When `None`
    /// we fall back to discovering `uv` on `PATH`, preserving the previous behaviour.
    pub fn new(workbench_path: PathBuf, use_uv: bool, uv_path: Option<PathBuf>) -> Self {
        Self {
            install_path: workbench_path,
            python_path: which("python").ok(),
            uv_path: uv_path.or_else(|| which("uv").ok()),
            use_uv,
        }
    }
}

/// The input a metadata processor receives from the workspace.
///
/// A processor must write a new metadata CSV and report its path in
/// [`PreprocessResult`]. The selected Workbench config is available for processors that need
/// site-specific rules, but is intentionally optional because processing can happen before a
/// config is chosen.
pub struct PreprocessRequest<'a> {
    pub input_csv: &'a Path,
    pub output_dir: &'a Path,
    pub language_url: &'a str,
    pub config_file: Option<&'a Path>,
}

/// The universal result returned by a metadata processor.
pub struct PreprocessResult {
    pub metadata_csv: PathBuf,
    pub details: ProcessResult,
}

/// Run the built-in Workbench preprocessor on a complete local CSV.
///
/// The built-in processor currently does not read `config_file`; accepting it here establishes
/// the contract used by future external processors without coupling them to the main window.
pub fn process_workbench_csv(request: PreprocessRequest<'_>) -> anyhow::Result<PreprocessResult> {
    std::fs::create_dir_all(request.output_dir).map_err(|e| {
        anyhow::anyhow!(
            "create output directory {}: {}",
            request.output_dir.display(),
            e
        )
    })?;
    let input_csv = request.input_csv.to_string_lossy();
    let out_dir = request.output_dir.to_string_lossy();
    let details = process_csv_and_maybe_generate_items(
        input_csv.as_ref(),
        Some("metadata.csv"),
        Some(out_dir.as_ref()),
        &[],
        &[],
        Some(request.language_url),
        false,
        None,
        None,
    )?;
    Ok(PreprocessResult {
        metadata_csv: PathBuf::from(&details.processed_output_path),
        details,
    })
}

/// Acquire a Google Sheet and run the built-in processor. This is a source adapter, not the
/// processor contract: additional processors still receive a local CSV through
/// [`PreprocessRequest`].
pub fn process_google_sheet_source(
    sheet_url: &str,
    output_dir: &Path,
    language_url: &str,
) -> anyhow::Result<PreprocessResult> {
    std::fs::create_dir_all(output_dir)?;
    let out_dir = output_dir.to_string_lossy();
    let details = process_google_sheets_and_maybe_generate_items(
        sheet_url,
        Some("metadata.csv"),
        Some(out_dir.as_ref()),
        &[],
        &[],
        Some(language_url),
        false,
        None,
        None,
    )?;
    Ok(PreprocessResult {
        metadata_csv: PathBuf::from(&details.processed_output_path),
        details,
    })
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
            .unwrap_or(if cfg!(target_os = "windows") {
                "python"
            } else {
                "python3"
            });
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
