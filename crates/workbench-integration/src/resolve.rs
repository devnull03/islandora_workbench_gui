//! Install paths recorded by the Windows installer, consumed once by the app on first run.
//!
//! Kept in this (gpui-free) crate so it stays free of registry/UI deps.

use std::path::PathBuf;

/// Paths recorded by the Windows installer under `HKLM\Software\Islandora Workbench GUI`.
/// Populated by the app (via `winreg`); this crate stays free of registry deps.
#[derive(Default, Clone)]
pub struct RegistryInstall {
    /// Full path to the bundled `uv.exe`, if the installer placed one.
    pub uv_path: Option<PathBuf>,
    /// User opted in to having the app manage the workbench install.
    pub provision_workbench: bool,
}
