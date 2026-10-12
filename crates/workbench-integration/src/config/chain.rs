//! Recursive secondary-config graph loading and link guardrails.
//!
//! Workbench executes `secondary_tasks` depth-first. The builder needs the same view so its
//! nesting UI, run-order strip, and link validation agree with the runtime rather than each
//! implementing a slightly different walk.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use super::{ConfigDraft, draft::resolve_secondary_task};

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
///
/// `workbench_dir` is the base relative entries resolve against at run time — see
/// [`resolve_secondary_task`]. Every function here takes it for the same reason.
pub fn child_nodes(root: &ConfigDraft, workbench_dir: Option<&Path>) -> Vec<SecondaryConfigNode> {
    let mut seen = HashSet::new();
    let mut ancestry = Vec::new();
    if let Some(path) = root.path.as_deref() {
        let key = identity(path);
        seen.insert(key.clone());
        ancestry.push(key);
    }
    root.resolved_secondary_tasks(workbench_dir)
        .into_iter()
        .map(|path| load_node(path, &ancestry, &mut seen, workbench_dir))
        .collect()
}

/// Flatten the graph in the order Workbench walks it: the root's children depth-first.
pub fn flattened_paths(root: &ConfigDraft, workbench_dir: Option<&Path>) -> Vec<PathBuf> {
    fn visit(node: &SecondaryConfigNode, output: &mut Vec<PathBuf>) {
        if node.error.is_none() {
            output.push(node.path.clone());
            for child in &node.children {
                visit(child, output);
            }
        }
    }

    let mut output = Vec::new();
    for node in child_nodes(root, workbench_dir) {
        visit(&node, &mut output);
    }
    output
}

/// Explain why linking `candidate` below `owner` is unsafe. Missing files are allowed so a user
/// can create a link before the child exists; cycles and duplicate references are rejected as
/// soon as they can be proven.
pub fn link_error(
    owner: &ConfigDraft,
    candidate: &Path,
    workbench_dir: Option<&Path>,
) -> Option<String> {
    let candidate = resolve_secondary_task(candidate, owner.path.as_deref(), workbench_dir);
    if owner
        .path
        .as_deref()
        .is_some_and(|path| same_path(path, &candidate))
    {
        return Some("A config can't run inside itself.".into());
    }
    if child_nodes(owner, workbench_dir)
        .iter()
        .any(|node| contains_path(node, &candidate))
    {
        return Some("That config already runs in this chain.".into());
    }

    // If the candidate already exists, inspect its descendants for the owner. This catches an
    // indirect cycle (A → B, then attempting to link A below B), not only a direct self-link.
    if candidate.is_file()
        && owner.path.as_deref().is_some_and(|owner_path| {
            reaches_path(&candidate, owner_path, workbench_dir, &mut HashSet::new())
        })
    {
        return Some("That link would create a cycle in the secondary-config chain.".into());
    }
    None
}

/// The file a chain entry names, if it can be found.
///
/// Exactly two rules, in order: the path as written, then the path with a YAML extension. That
/// covers the one spelling people actually use and stops well short of searching the disk —
/// a chain entry that resolves to a file the user did not name is worse than one that fails.
fn discover(path: &Path) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    let name = path.file_name()?.to_string_lossy().into_owned();
    ["yml", "yaml"]
        .into_iter()
        .map(|extension| path.with_file_name(format!("{name}.{extension}")))
        .find(|candidate| candidate.is_file())
}

fn load_node(
    path: PathBuf,
    ancestry: &[PathBuf],
    seen: &mut HashSet<PathBuf>,
    workbench_dir: Option<&Path>,
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
    // People write the entry the way they think of the file — `create-items.config` for
    // `create-items.config.yml`. Workbench does **not** resolve that (it opens the entry
    // as-is and stops with "Configuration file not found"), so the node is broken; but the
    // message names the file sitting right there rather than calling it moved or deleted.
    let path = match discover(&path) {
        Some(found) if found == path => found,
        Some(found) => {
            let name = found
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            return broken(
                found,
                label,
                format!("Workbench needs the full file name — relink it as {name}."),
            );
        }
        None => return broken(path, label, "This config was moved or deleted."),
    };

    let draft = match ConfigDraft::load(&path) {
        Ok(draft) => draft,
        Err(error) => {
            return broken(path, label, format!("Couldn't read this config: {error}"));
        }
    };
    let mut next_ancestry = ancestry.to_vec();
    next_ancestry.push(key);
    let children = draft
        .resolved_secondary_tasks(workbench_dir)
        .into_iter()
        .map(|child| load_node(child, &next_ancestry, seen, workbench_dir))
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

fn reaches_path(
    start: &Path,
    target: &Path,
    workbench_dir: Option<&Path>,
    seen: &mut HashSet<PathBuf>,
) -> bool {
    let key = identity(start);
    if !seen.insert(key) {
        return false;
    }
    let Ok(draft) = ConfigDraft::load(start) else {
        return false;
    };
    draft
        .resolved_secondary_tasks(workbench_dir)
        .into_iter()
        .any(|child| same_path(&child, target) || reaches_path(&child, target, workbench_dir, seen))
}

pub(crate) fn identity(path: &Path) -> PathBuf {
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

/// Do two paths name the same file? Canonical when they exist, absolute otherwise.
pub fn same_path(left: &Path, right: &Path) -> bool {
    identity(left) == identity(right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::test_support::TempTree;

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
        // Relative entries resolve against the Workbench directory, at every depth.
        let nodes = child_nodes(&root, Some(&tree.root));
        assert_eq!(nodes[0].label, "child");
        assert_eq!(nodes[0].children[0].label, "grand");
        assert_eq!(flattened_paths(&root, Some(&tree.root)).len(), 2);

        // From anywhere else, the same relative entries are not found — the bug P0-4 fixed
        // was the builder resolving them against the config's folder while Workbench did not.
        let elsewhere = tree.root.join("workbench");
        assert!(child_nodes(&root, Some(&elsewhere))[0].error.is_some());
    }

    /// Workbench opens an entry as written; a missing `.yml` is a broken link, but one whose
    /// message names the real file.
    #[test]
    fn an_entry_without_its_extension_is_broken_but_explained() {
        let tree = TempTree::new();
        let child = tree.write("pages.config.yml", "task: create\n");
        let root_path = tree.write(
            "root.yml",
            &format!(
                "task: create\nsecondary_tasks: [{}]\n",
                child.with_extension("").display()
            ),
        );
        let root = ConfigDraft::load(&root_path).unwrap();
        let nodes = child_nodes(&root, None);
        assert!(
            nodes[0]
                .error
                .as_deref()
                .unwrap()
                .contains("pages.config.yml")
        );
        assert_eq!(nodes[0].path, child);
    }

    #[test]
    fn direct_and_indirect_cycles_are_rejected() {
        let tree = TempTree::new();
        let root_path = tree.write("root.yml", "task: create\n");
        let child_path = tree.write(
            "child.yml",
            &format!("task: update\nsecondary_tasks: [{}]\n", root_path.display()),
        );
        let root = ConfigDraft::load(&root_path).unwrap();
        assert!(link_error(&root, &root_path, None).is_some());
        // Root → child → root is an indirect cycle even though the candidate is not root itself.
        assert!(link_error(&root, &child_path, None).is_some());
        let child = ConfigDraft::load(&child_path).unwrap();
        assert!(link_error(&child, &root_path, None).is_some());
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
        let nodes = child_nodes(&root, None);
        assert!(nodes[1].error.is_some());
        assert!(link_error(&root, &missing, None).is_none());
        assert!(link_error(&root, &child, None).is_some());
    }
}
