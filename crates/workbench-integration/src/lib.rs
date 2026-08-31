//! Islandora Workbench process integration.
//!
//! **Learning layout:** types and main API stubs live here; utilities and reference
//! implementations are in sibling modules (`util`, `stream`, `sheet`, `placeholder`).

mod check;
pub mod config;
mod preprocess;
mod provision;
mod resolve;
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

use which::which;

pub use util::{format_stream_line, language_url_from_server_base, run_command_capture_stdout};

pub use stream::{StdinSink, StreamLine, spawn_command_streaming};

pub use config::{APP_SUPPLIED, ConfigDraft, WorkbenchConfigHandler};

pub use check::{ServerCheck, check_server};
pub use preprocess::{
    InputSource, PreprocessJob, PreprocessResult, ProcessResult, Processor, run as run_preprocess,
};
pub use provision::provision_workbench;

pub use resolve::RegistryInstall;

use crate::config::Ready;

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

    /// The Python interpreter for this install, as a `Command` with the workbench directory as
    /// its working directory — so `uv run` picks up workbench's environment, and a preprocess
    /// script gets the same dependencies Workbench itself has.
    pub fn python_command(&self) -> Command {
        if self.use_uv {
            let uv = self
                .uv_path
                .as_deref()
                .and_then(|p| p.to_str())
                .unwrap_or("uv");
            let mut c = Command::new(uv);
            c.args(["run", "python", "-u"]);
            c.current_dir(&self.install_path);
            c
        } else {
            let python = self
                .python_path
                .as_deref()
                .and_then(|p| p.to_str())
                .unwrap_or(if cfg!(target_os = "windows") {
                    "python"
                } else {
                    "python3"
                });
            let mut c = Command::new(python);
            c.arg("-u");
            c.current_dir(&self.install_path);
            c
        }
    }
}

pub fn build_workbench_command(
    workbench_info: &WbInfo,
    config_file: &WorkbenchConfigHandler<Ready>,
) -> anyhow::Result<Command> {
    let mut cmd = workbench_info.python_command();
    cmd.arg("workbench")
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
    log::info!(
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
