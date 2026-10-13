//! Finding Python and uv the way a user would expect on each OS.
//!
//! A desktop app does not get a terminal's `PATH`: launched from Finder or the Dock on macOS it
//! sees only `/usr/bin:/bin:/usr/sbin:/sbin`, where `python3` is Apple's stub (it offers to
//! install the Command Line Tools) and Homebrew or `~/.local/bin` tools are missing. On Windows
//! the python.org installer leaves `python` off `PATH` by default and installs the `py`
//! launcher instead, while `WindowsApps\python.exe` is a Store alias that only prints "Python
//! was not found".

use std::path::{Path, PathBuf};

/// The folders searched for a program: `PATH`, then on macOS the usual install locations a
/// Finder-launched app does not inherit.
fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    if cfg!(target_os = "macos") {
        let home = dirs::home_dir();
        let extra = ["/opt/homebrew/bin", "/usr/local/bin"]
            .into_iter()
            .map(PathBuf::from)
            .chain(
                home.iter()
                    .flat_map(|h| [h.join(".local/bin"), h.join(".cargo/bin")]),
            );
        for dir in extra {
            if !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
    }
    dirs
}

/// Every executable called `name` (`name.exe` on Windows) in the search folders, in order.
fn candidates(name: &str) -> Vec<PathBuf> {
    let file = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    search_dirs()
        .into_iter()
        .map(|dir| dir.join(&file))
        .filter(|path| path.is_file())
        .collect()
}

/// A program that would not do what its name says: the Store alias on Windows, Apple's stub
/// on macOS.
fn is_placeholder(path: &Path) -> bool {
    if cfg!(windows) {
        path.components()
            .any(|c| c.as_os_str().eq_ignore_ascii_case("WindowsApps"))
    } else if cfg!(target_os = "macos") {
        path == Path::new("/usr/bin/python3")
    } else {
        false
    }
}

/// The first real `name` on the search path, falling back to a placeholder only when nothing
/// else exists (so the user at least sees the system's own message).
fn first_real(names: &[&str]) -> Option<PathBuf> {
    let all: Vec<PathBuf> = names.iter().flat_map(|name| candidates(name)).collect();
    all.iter()
        .find(|path| !is_placeholder(path))
        .or_else(|| all.first())
        .cloned()
}

/// The Python to run Workbench with when Settings names none. On Windows the `py` launcher
/// stands in when `python` is not on `PATH` ([`launcher_args`] adds its `-3`).
pub fn find_python() -> Option<PathBuf> {
    if cfg!(windows) {
        first_real(&["python"])
            .filter(|path| !is_placeholder(path))
            .or_else(|| first_real(&["py"]))
    } else {
        first_real(&["python3", "python"])
    }
}

/// uv, wherever it was installed.
pub fn find_uv() -> Option<PathBuf> {
    first_real(&["uv"])
}

/// Arguments the interpreter needs before the script: the Windows `py` launcher must be told
/// to pick a Python 3.
pub fn launcher_args(python: &Path) -> &'static [&'static str] {
    let stem = python.file_stem().and_then(|s| s.to_str());
    if stem.is_some_and(|s| s.eq_ignore_ascii_case("py")) {
        &["-3"]
    } else {
        &[]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_py_launcher_asks_for_python_3() {
        assert_eq!(launcher_args(Path::new("C:/Windows/py.exe")), ["-3"]);
        assert!(launcher_args(Path::new("/usr/bin/python3")).is_empty());
    }

    #[test]
    fn placeholders_are_skipped() {
        if cfg!(windows) {
            assert!(is_placeholder(Path::new(
                r"C:\Users\a\AppData\Local\Microsoft\WindowsApps\python.exe"
            )));
        }
        if cfg!(target_os = "macos") {
            assert!(is_placeholder(Path::new("/usr/bin/python3")));
        }
        assert!(!is_placeholder(Path::new("/opt/homebrew/bin/python3")));
    }
}
