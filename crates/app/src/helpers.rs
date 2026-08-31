//! Desktop and operating-system services owned by the application shell.

use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use gpui::App;
use settings::AppSettings;

/// `{workbench_path}/input_data` from the `workbench_path` setting, or `None` when it's empty.
pub fn workbench_input_data_dir(cx: &App) -> Option<PathBuf> {
    let path = AppSettings::get(cx)
        .values
        .get("workbench_path")
        .map(|v| v.text())
        .filter(|s| !s.trim().is_empty())?;
    Some(PathBuf::from(path.trim()).join("input_data"))
}

// `RegistryInstall` lives in the (gpui-free) workbench-integration crate; re-exported here so app
// code keeps importing it from `helpers`.
pub use workbench_integration::RegistryInstall;

/// Read the installer-written registry values. Returns an empty struct off-Windows or when the
/// key is absent (i.e. workbench was installed manually).
#[cfg(windows)]
pub fn registry_install() -> RegistryInstall {
    use winreg::RegKey;
    use winreg::enums::HKEY_LOCAL_MACHINE;

    let Ok(key) =
        RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"Software\Islandora Workbench GUI")
    else {
        return RegistryInstall::default();
    };
    // Drop a uv path that no longer exists (e.g. antivirus quarantined it) so callers fall back to PATH.
    let uv_path = key
        .get_value::<String, _>("UvPath")
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.exists());
    let provision_workbench = key
        .get_value::<String, _>("ProvisionWorkbench")
        .map(|v: String| v == "1")
        .unwrap_or(false);
    RegistryInstall {
        uv_path,
        provision_workbench,
    }
}

#[cfg(not(windows))]
pub fn registry_install() -> RegistryInstall {
    RegistryInstall::default()
}

/// Per-user, writable location the app provisions workbench into. Shares the base directory with
/// the settings DB (`dirs::data_local_dir()/islandora_workbench_gui`).
pub fn per_user_workbench_dir() -> Option<PathBuf> {
    Some(
        dirs::data_local_dir()?
            .join("islandora_workbench_gui")
            .join("islandora_workbench"),
    )
}

/// Opens the file manager and selects `path`.
///
/// For a file, highlights that file in its parent folder. For a directory, shows that folder
/// selected in its parent (Windows) or opens it (macOS).
pub fn reveal_in_folder(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("path does not exist: {}", path.display()),
        ));
    }

    #[cfg(windows)]
    {
        Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn()?;
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-R", &path.to_string_lossy()])
            .spawn()?;
    }

    #[cfg(target_os = "linux")]
    {
        // xdg-open has no cross-desktop "select this file" operation, so open the file's parent.
        let folder = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(path)
        };
        Command::new("xdg-open").arg(folder).spawn()?;
    }

    Ok(())
}

/// Opens a terminal at `cwd`. If `command` is non-empty it is executed after `cd`.
///
/// Windows: tries Windows Terminal (`wt`) first, falls back to `cmd.exe`.
/// macOS: checks `$TERM_PROGRAM` for iTerm2, falls back to Terminal.app.
/// Linux: opens the desktop's `x-terminal-emulator` and starts in `cwd`.
pub fn spawn_terminal_at(cwd: &Path, command: &str) -> std::io::Result<()> {
    if !cwd.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("spawn_terminal_at expects a directory: {}", cwd.display()),
        ));
    }

    #[cfg(windows)]
    {
        let cwd_str = cwd.to_string_lossy();

        // Try Windows Terminal first — it accepts --startingDirectory natively.
        let mut wt = Command::new("wt");
        wt.arg("--startingDirectory").arg(cwd);
        if !command.trim().is_empty() {
            wt.args(["cmd", "/k", command]);
        }
        if wt.spawn().is_ok() {
            return Ok(());
        }

        // Fall back to cmd.exe. `start /d path` sets the new window's working directory.
        let mut args = vec!["/C", "start", "", "/d", &cwd_str, "cmd"];
        let script;
        if !command.trim().is_empty() {
            script = command.to_string();
            args.extend_from_slice(&["/k", &script]);
        }
        Command::new("cmd").args(&args).spawn()?;
    }

    #[cfg(target_os = "macos")]
    {
        let term_prog = std::env::var("TERM_PROGRAM").unwrap_or_default();

        if command.trim().is_empty() {
            if term_prog == "iTerm.app" {
                let escaped = escape_applescript(&cwd.to_string_lossy());
                let osa = format!(
                    r#"tell application "iTerm" to create window with default profile command "cd \"{}\"" "#,
                    escaped
                );
                Command::new("osascript").args(["-e", &osa]).spawn()?;
            } else {
                // Terminal.app accepts a directory argument directly.
                Command::new("open")
                    .args(["-a", "Terminal"])
                    .arg(cwd)
                    .spawn()?;
            }
        } else {
            let script = format!("cd \"{}\" && {}", cwd.display(), command);
            let escaped = escape_applescript(&script);
            let app = if term_prog == "iTerm.app" {
                "iTerm"
            } else {
                "Terminal"
            };
            let osa = format!(r#"tell application "{}" to do script "{}""#, app, escaped);
            Command::new("osascript").args(["-e", &osa]).spawn()?;
        }
    }

    #[cfg(target_os = "linux")]
    {
        let mut terminal = Command::new("x-terminal-emulator");
        terminal.current_dir(cwd);
        if !command.trim().is_empty() {
            terminal.args([
                "-e",
                "sh",
                "-lc",
                &format!("{command}; exec \"${{SHELL:-sh}}\""),
            ]);
        }
        terminal.spawn()?;
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn escape_applescript(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
