//! Shared fixtures for the config module's tests.

use std::path::PathBuf;

/// A scratch directory removed on drop. Unique per process and per call, so parallel tests
/// never share one.
pub(crate) struct TempTree {
    pub root: PathBuf,
}

impl TempTree {
    pub fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "islandora-workbench-config-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    /// Write `body` to `name` (which may contain subfolders) and return its path.
    pub fn write(&self, name: &str, body: &str) -> PathBuf {
        let path = self.root.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, body).unwrap();
        path
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
