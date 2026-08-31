//! Recursive secondary-config graph loading and link guardrails.
//!
//! Workbench executes `secondary_tasks` depth-first. The builder needs the same view so its
//! nesting UI, run-order strip, and link validation agree with the runtime rather than each
//! implementing a slightly different walk.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use super::ConfigDraft;

#[derive(Debug, Clone)]
pub struct SecondaryConfigNode {
    pub path: PathBuf,
    pub label: String,
    pub task: Option<String>,
    pub settings: usize,
    pub children: Vec<SecondaryConfigNode>,
    pub error: Option<String>,
}

/// Load the root's complete child graph. A node is still returned when it is missing, malformed,
/// repeated, or cyclic, so the UI can explain and repair the link in place.
pub fn child_nodes(root: &ConfigDraft) -> Vec<SecondaryConfigNode> {
    let mut seen = HashSet::new();
    let mut ancestry = Vec::new();
    if let Some(path) = root.path.as_deref() {
        let key = identity(path);
        seen.insert(key.clone());
        ancestry.push(key);
    }
    root.resolved_secondary_tasks()
        .into_iter()
        .map(|path| load_node(path, &ancestry, &mut seen))
        .collect()
}

/// Flatten the graph in the order Workbench walks it: the root's children depth-first.
pub fn flattened_paths(root: &ConfigDraft) -> Vec<PathBuf> {
    fn visit(node: &SecondaryConfigNode, output: &mut Vec<PathBuf>) {
        if node.error.is_none() {
            output.push(node.path.clone());
            for child in &node.children {
                visit(child, output);
            }
        }
    }

    let mut output = Vec::new();
    for node in child_nodes(root) {
        visit(&node, &mut output);
    }
    output
}

/// Explain why linking `candidate` below `owner` is unsafe. Missing files are allowed so a user
/// can create a link before the child exists; cycles and duplicate references are rejected as
/// soon as they can be proven.
pub fn link_error(owner: &ConfigDraft, candidate: &Path) -> Option<String> {
    let candidate = resolve_for(owner.path.as_deref(), candidate);
    if owner
        .path
        .as_deref()
        .is_some_and(|path| same_path(path, &candidate))
    {
        return Some("A config can't run inside itself.".into());
    }
    if child_nodes(owner)
        .iter()
        .any(|node| contains_path(node, &candidate))
    {
        return Some("That config already runs in this chain.".into());
    }

    // If the candidate already exists, inspect its descendants for the owner. This catches an
    // indirect cycle (A → B, then attempting to link A below B), not only a direct self-link.
    if candidate.is_file()
        && owner
            .path
            .as_deref()
            .is_some_and(|owner_path| reaches_path(&candidate, owner_path, &mut HashSet::new()))
    {
        return Some("That link would create a cycle in the secondary-config chain.".into());
    }
    None
}

fn load_node(
    path: PathBuf,
    ancestry: &[PathBuf],
    seen: &mut HashSet<PathBuf>,
) -> SecondaryConfigNode {
    let key = identity(&path);
    let label = path
        .file_stem()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());

    if ancestry.contains(&key) {
        return broken(path, label, "This link points back to an ancestor config.");
    }
    if !seen.insert(key.clone()) {
        return broken(
            path,
            label,
            "This config is already linked elsewhere in the chain.",
        );
    }
    if !path.is_file() {
        return broken(path, label, "This config was moved or deleted.");
    }

    let draft = match ConfigDraft::load(&path) {
        Ok(draft) => draft,
        Err(error) => {
            return broken(path, label, format!("Couldn't read this config: {error}"));
        }
    };
    let mut next_ancestry = ancestry.to_vec();
    next_ancestry.push(key);
    let children = draft
        .resolved_secondary_tasks()
        .into_iter()
        .map(|child| load_node(child, &next_ancestry, seen))
        .collect();
    SecondaryConfigNode {
        path,
        label,
        task: draft
            .values
            .get("task")
            .and_then(|value| value.as_str())
            .map(str::to_owned),
        settings: draft.values.len(),
        children,
        error: None,
    }
}

fn broken(path: PathBuf, label: String, error: impl Into<String>) -> SecondaryConfigNode {
    SecondaryConfigNode {
        path,
        label,
        task: None,
        settings: 0,
        children: Vec::new(),
        error: Some(error.into()),
    }
}

fn contains_path(node: &SecondaryConfigNode, target: &Path) -> bool {
    same_path(&node.path, target)
        || node
            .children
            .iter()
            .any(|child| contains_path(child, target))
}

fn reaches_path(start: &Path, target: &Path, seen: &mut HashSet<PathBuf>) -> bool {
    let key = identity(start);
    if !seen.insert(key) {
        return false;
    }
    let Ok(draft) = ConfigDraft::load(start) else {
        return false;
    };
    draft
        .resolved_secondary_tasks()
        .into_iter()
        .any(|child| same_path(&child, target) || reaches_path(&child, target, seen))
}

fn resolve_for(owner: Option<&Path>, candidate: &Path) -> PathBuf {
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else if let Some(parent) = owner.and_then(Path::parent) {
        parent.join(candidate)
    } else {
        candidate.to_path_buf()
    }
}

fn identity(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|current| current.join(path))
                .unwrap_or_else(|_| path.to_path_buf())
        }
    })
}

fn same_path(left: &Path, right: &Path) -> bool {
    identity(left) == identity(right)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempTree {
        root: PathBuf,
    }

    impl TempTree {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "islandora-workbench-chain-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn write(&self, name: &str, body: &str) -> PathBuf {
            let path = self.root.join(name);
            std::fs::write(&path, body).unwrap();
            path
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn nested_graph_flattens_depth_first() {
        let tree = TempTree::new();
        tree.write("grand.yml", "task: update\n");
        tree.write(
            "child.yml",
            "task: add_media\nsecondary_tasks: [grand.yml]\n",
        );
        let root_path = tree.write("root.yml", "task: create\nsecondary_tasks: [child.yml]\n");
        let root = ConfigDraft::load(&root_path).unwrap();
        let nodes = child_nodes(&root);
        assert_eq!(nodes[0].label, "child");
        assert_eq!(nodes[0].children[0].label, "grand");
        assert_eq!(flattened_paths(&root).len(), 2);
    }

    #[test]
    fn direct_and_indirect_cycles_are_rejected() {
        let tree = TempTree::new();
        let root_path = tree.write("root.yml", "task: create\n");
        let child_path = tree.write("child.yml", "task: update\nsecondary_tasks: [root.yml]\n");
        let root = ConfigDraft::load(&root_path).unwrap();
        assert!(link_error(&root, &root_path).is_some());
        // Root → child → root is an indirect cycle even though the candidate is not root itself.
        assert!(link_error(&root, &child_path).is_some());
        let child = ConfigDraft::load(&child_path).unwrap();
        assert!(link_error(&child, &root_path).is_some());
    }

    #[test]
    fn missing_files_are_visible_and_duplicate_references_are_marked() {
        let tree = TempTree::new();
        let missing = tree.root.join("missing.yml");
        let child = tree.write("child.yml", "task: update\n");
        let root_path = tree.write(
            "root.yml",
            &format!(
                "task: create\nsecondary_tasks: [{}, {}]\n",
                child.display(),
                child.display()
            ),
        );
        let root = ConfigDraft::load(&root_path).unwrap();
        let nodes = child_nodes(&root);
        assert!(nodes[1].error.is_some());
        assert!(link_error(&root, &missing).is_none());
        assert!(link_error(&root, &child).is_some());
    }
}
