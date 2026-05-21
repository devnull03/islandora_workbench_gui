use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use gpui::*;
use gpui_component::input::InputState;
use settings::{AppSettings, ServerConfig};

use crate::workspace::Workspace;

/// `{workbench_path}/input_data` from Settings (Workbench Path).
pub fn workbench_input_data_dir(cx: &App) -> Option<PathBuf> {
    let raw = AppSettings::get(cx).values.get("workbench_path")?.text();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(PathBuf::from(trimmed).join("input_data"))
}

pub fn server_config_for_label<'a>(cx: &'a App, label: &SharedString) -> Option<&'a ServerConfig> {
    AppSettings::get(cx)
        .server_configs
        .iter()
        .find(|s| &s.label == label)
}

pub fn get_file(
    window: &mut Window,
    cx: &mut Context<Workspace>,
    input: &Entity<InputState>,
    prompt: SharedString,
    is_folder: bool,
) {
    let receiver = cx.prompt_for_paths(PathPromptOptions {
        files: !is_folder,
        directories: is_folder,
        multiple: false,
        prompt: Some(prompt),
    });

    let input = input.clone();
    cx.spawn_in(window, async move |_, cx| {
        if let Ok(Ok(Some(paths))) = receiver.await
            && let Some(path) = paths.first()
        {
            cx.update(|window, cx| {
                input.update(cx, |state, cx| {
                    state.set_value(path.to_string_lossy().to_string(), window, cx);
                });
            })
            .ok();
        }
    })
    .detach();
}

/// Opens a folder in the system file manager (Explorer on Windows, Finder on macOS).
pub fn _open_folder(path: &Path) -> std::io::Result<()> {
    if !path.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("open_folder expects a directory: {}", path.display()),
        ));
    }

    #[cfg(windows)]
    {
        Command::new("explorer").arg(path).spawn()?;
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).spawn()?;
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "open_folder is only supported on Windows and macOS",
        ));
    }

    Ok(())
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

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "reveal_in_folder is only supported on Windows and macOS",
        ));
    }

    Ok(())
}

/// Opens the default terminal with `cwd` as the working directory.
///
/// If `command` is non-empty, it is run after `cd` (via `&&`). Use an empty string for a shell
/// opened at `cwd` only. Uses `cmd /k` on Windows and Terminal.app on macOS.
pub fn spawn_terminal_at(cwd: &Path, command: &str) -> std::io::Result<()> {
    if !cwd.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("spawn_terminal_at expects a directory: {}", cwd.display()),
        ));
    }

    #[cfg(windows)]
    {
        let script = if command.trim().is_empty() {
            format!("cd /d \"{}\"", cwd.display())
        } else {
            format!("cd /d \"{}\" && {}", cwd.display(), command)
        };
        Command::new("cmd")
            .args(["/C", "start", "", "cmd", "/k", &script])
            .spawn()?;
    }

    #[cfg(target_os = "macos")]
    {
        let script = if command.trim().is_empty() {
            format!("cd \"{}\"", cwd.display())
        } else {
            format!("cd \"{}\" && {}", cwd.display(), command)
        };
        let escaped = escape_applescript(&script);
        let osa = format!(r#"tell application "Terminal" to do script "{}""#, escaped);
        Command::new("osascript").args(["-e", &osa]).spawn()?;
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "spawn_terminal_at is only supported on Windows and macOS",
        ));
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn escape_applescript(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
