//! First-run provisioning of the Islandora Workbench Python tool.
//!
//! The installer can opt a user in to having the GUI manage workbench for them. Because the
//! workbench directory must be **writable at runtime** (uv writes `uv.lock`/`.venv`, the GUI writes
//! `input_data/`, and workbench writes logs there), it lives in a per-user location and is fetched
//! here on first use — not by the elevated installer, which can't reliably target the real user's
//! profile.

use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

/// The `main` branch zipball. GitHub wraps the contents in a single top-level
/// `islandora_workbench-main/` directory, which `install_zip_bytes` flattens away.
const WORKBENCH_ZIP_URL: &str =
    "https://github.com/mjordan/islandora_workbench/archive/refs/heads/main.zip";

/// Download the Islandora Workbench `main` zipball and extract it into `dest`.
///
/// ponytail: tracks `main`, no version pin and no update check — the caller treats an existing
/// `dest/pyproject.toml` as "already provisioned". Pin a tag here if reproducibility matters.
pub fn provision_workbench(dest: &Path) -> anyhow::Result<()> {
    // The zipball is a few MB, so buffering it in memory beats managing a temp file.
    let resp = ureq::get(WORKBENCH_ZIP_URL)
        .call()
        .map_err(|e| anyhow::anyhow!("download workbench: {e}"))?;
    let mut bytes = Vec::new();
    resp.into_reader().read_to_end(&mut bytes)?;
    install_zip_bytes(bytes, dest)
}

/// Extract a workbench zipball into `dest`, flattening the archive's single top-level directory.
///
/// Idempotent: an existing `dest` is replaced. Extraction happens in a staging dir and is only
/// swapped into place after a sanity check, so a failed/partial download never leaves a broken
/// `dest` behind (which the caller relies on to decide whether provisioning already happened).
/// Split out from the download so it is testable without network access.
fn install_zip_bytes(bytes: Vec<u8>, dest: &Path) -> anyhow::Result<()> {
    let parent = dest
        .parent()
        .ok_or_else(|| anyhow::anyhow!("workbench dest has no parent: {}", dest.display()))?;
    fs::create_dir_all(parent)?;
    let staging = parent.join(".workbench_dl_tmp");
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;

    let extract = || -> anyhow::Result<PathBuf> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
        archive.extract(&staging)?;
        // Flatten the single top-level `islandora_workbench-main/` folder.
        let top = first_subdir(&staging)?
            .ok_or_else(|| anyhow::anyhow!("workbench archive had no top-level directory"))?;
        // Sanity-check it really is workbench before we trust it.
        if !top.join("pyproject.toml").exists() {
            anyhow::bail!("downloaded archive is not Islandora Workbench (no pyproject.toml)");
        }
        Ok(top)
    };

    let top = match extract() {
        Ok(top) => top,
        Err(e) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
    };

    // Swap the flattened dir into place. Same volume as the staging dir, so this is a rename.
    if dest.exists() {
        fs::remove_dir_all(dest)?;
    }
    fs::rename(&top, dest)?;
    let _ = fs::remove_dir_all(&staging);
    Ok(())
}

/// The first immediate subdirectory of `dir`, if any.
fn first_subdir(dir: &Path) -> std::io::Result<Option<PathBuf>> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            return Ok(Some(entry.path()));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A GitHub-shaped zipball: everything under one top-level directory.
    fn zipball(top: &str, files: &[(&str, &str)]) -> Vec<u8> {
        use std::io::Write;
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
        w.add_directory(format!("{top}/"), opts).unwrap();
        for (name, body) in files {
            w.start_file(format!("{top}/{name}"), opts).unwrap();
            w.write_all(body.as_bytes()).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wb_provision_test_{tag}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn flattens_top_level_dir() {
        let root = tmpdir("ok");
        let dest = root.join("islandora_workbench");
        let bytes = zipball(
            "islandora_workbench-main",
            &[("pyproject.toml", "[project]"), ("workbench", "#!/bin/sh")],
        );

        install_zip_bytes(bytes, &dest).unwrap();

        // Contents land directly in dest, not under a nested `islandora_workbench-main/`.
        assert!(dest.join("pyproject.toml").exists());
        assert!(dest.join("workbench").exists());
        assert!(!dest.join("islandora_workbench-main").exists());
        assert!(!root.join(".workbench_dl_tmp").exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_non_workbench_archive_without_touching_dest() {
        let root = tmpdir("bad");
        let dest = root.join("islandora_workbench");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("pyproject.toml"), "existing").unwrap();

        let err = install_zip_bytes(zipball("something-else", &[("README.md", "hi")]), &dest)
            .unwrap_err();
        assert!(err.to_string().contains("not Islandora Workbench"));

        // A rejected download must leave the previous install intact.
        assert_eq!(
            fs::read_to_string(dest.join("pyproject.toml")).unwrap(),
            "existing"
        );
        assert!(!root.join(".workbench_dl_tmp").exists());

        let _ = fs::remove_dir_all(&root);
    }
}
