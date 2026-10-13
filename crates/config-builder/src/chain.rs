//! Recursive `secondary_tasks` editor (mockups `1d` and `2a`).
//!
//! Every edit goes through [`ConfigBuilder::apply_chain_edit`], so a change to a config that is
//! open in another window lands in that window's draft rather than behind it on disk.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme, Disableable, Icon, IconName, Sizable, Size, StyledExt, alert::Alert, h_flex,
    label::Label, v_flex,
};
use ui::tokens::{CHEVRON_SLOT, GAP_MD, GAP_SM, INDENT_STEP};
use ui::{Card, CardTone, app_button, app_tag, ghost_button};
use workbench_integration::config::{self, ConfigDraft, chain::SecondaryConfigNode};

use super::{ConfigBuilder, ConfigBuilderWindows, open_config_builder_in_chain, workbench_dir};

/// Where a node sits: who runs it (`parent`, `None` for a top-level node of a never-saved
/// draft), the chain above it, its depth, and — for top-level nodes, the ones this window can
/// reorder — its index and the number of siblings.
struct NodePosition {
    parent: Option<PathBuf>,
    ancestors: Vec<PathBuf>,
    depth: usize,
    top_level: Option<(usize, usize)>,
}

impl ConfigBuilder {
    pub(super) fn render_chain(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let now = Instant::now();
        if self
            .last_chain_scan
            .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(2))
        {
            self.chain_nodes =
                config::chain::child_nodes(&self.draft, workbench_dir(cx).as_deref());
            self.last_chain_scan = Some(now);
        }

        let (count, max_depth) = chain_stats(&self.chain_nodes, 1);
        let mut current_ancestry = self.ancestors.clone();
        if let Some(path) = self.draft.path.clone() {
            current_ancestry.push(path);
        }
        // Which configs have a builder window. Plain path equality, no file-system call: this
        // runs every frame.
        let open_paths: HashSet<PathBuf> = cx.global::<ConfigBuilderWindows>().open_paths();
        let siblings = self.chain_nodes.len();
        let mut rows = Vec::new();
        for (index, node) in self.chain_nodes.iter().enumerate() {
            self.render_node(
                node,
                format!("{}", index + 1),
                NodePosition {
                    parent: self.draft.path.clone(),
                    ancestors: current_ancestry.clone(),
                    depth: 0,
                    top_level: Some((index, siblings)),
                },
                &open_paths,
                &mut rows,
                cx,
            );
        }
        // The graph was loaded above on its two-second cadence; derive this display from the
        // cached nodes so rendering does not re-read every child YAML.
        let run_order = flattened(&self.chain_nodes);
        let summary: SharedString = if count == 0 {
            "run after this config finishes".into()
        } else {
            format!(
                "{count} config{} · {max_depth} level{}",
                if count == 1 { "" } else { "s" },
                if max_depth == 1 { "" } else { "s" }
            )
            .into()
        };
        let colors = cx.theme().colors;

        let chain_panel = Card::new()
            .tone(CardTone::Filled)
            .padding(GAP_MD)
            .gap(GAP_MD)
            .child(
                h_flex()
                    .gap(GAP_SM)
                    .items_center()
                    .child(
                        Label::new("↳")
                            .text_xs()
                            .font_semibold()
                            .text_color(colors.warning),
                    )
                    .child(
                        Label::new("CHAIN")
                            .text_xs()
                            .font_semibold()
                            .text_color(colors.warning),
                    )
                    .child(
                        Label::new(summary)
                            .text_xs()
                            .text_color(colors.muted_foreground),
                    )
                    .child(div().flex_1())
                    .when(count > 0, |this| {
                        this.child(
                            ghost_button("collapse-all-chain")
                                .label("Collapse all")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    for node in &this.chain_nodes {
                                        collect_paths(node, &mut this.collapsed_chain);
                                    }
                                    cx.notify();
                                })),
                        )
                    }),
            )
            .children(rows)
            .child(
                h_flex()
                    .gap(GAP_MD)
                    .child(
                        app_button("link-config")
                            .outline()
                            .label("Link an existing config")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.pick_and_link(None, window, cx)
                            })),
                    )
                    .child(
                        app_button("create-child-config")
                            .outline()
                            .label("Create a new one")
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(parent) = this.draft.path.clone() {
                                    let mut ancestors = this.ancestors.clone();
                                    ancestors.push(parent);
                                    open_config_builder_in_chain(None, ancestors, cx);
                                } else {
                                    this.chain_error = Some(
                                        "Save this config before creating one to run after it."
                                            .into(),
                                    );
                                    cx.notify();
                                }
                            })),
                    ),
            )
            // `2a`: a link that cannot be made is explained in the chain, beside what was
            // attempted, not in the title bar.
            .children(self.chain_error.clone().map(ui::InlineMessage::error));

        v_flex()
            .w_full()
            .gap(GAP_MD)
            .pt_2()
            .child(chain_panel)
            // §08 warns at depth 4 and beyond and does not block. A chain that deep is
            // usually a mistake and occasionally exactly what someone meant; the app is not
            // in a position to tell which.
            .when(max_depth >= 4, |this| {
                this.child(
                    Alert::warning(
                        "chain-depth-warning",
                        format!(
                            "This chain is {max_depth} levels deep. Each level runs every config \
                             below it, so the run order is longer than it looks."
                        ),
                    )
                    .with_size(Size::Small)
                    .icon(Icon::new(IconName::TriangleAlert).text_color(colors.warning))
                    .bg(colors.table_head)
                    .border_color(colors.border)
                    .text_color(colors.muted_foreground),
                )
            })
            .when(!run_order.is_empty(), |this| {
                let mono = cx.theme().mono_font_family.clone();
                let ancestry = current_ancestry.clone();
                this.child(
                    Card::new()
                        .tone(CardTone::Filled)
                        .gap(GAP_SM)
                        .child(
                            Label::new("RUN ORDER")
                                .text_xs()
                                .font_semibold()
                                .text_color(colors.muted_foreground),
                        )
                        // §08: the flattened depth-first order as chips that wrap, rather than
                        // one joined string that truncates. A chain you cannot read the end of
                        // is the one you most need to read the end of. Each chip opens its
                        // config.
                        .child(
                            h_flex().w_full().gap(GAP_SM).flex_wrap().children(
                                run_order
                                    .into_iter()
                                    .enumerate()
                                    .map(|(index, (label, path))| {
                                        let ancestors = ancestry.clone();
                                        app_button(SharedString::from(format!("run-order-{index}")))
                                            .with_size(Size::Small)
                                            .outline()
                                            .tooltip(path.display().to_string())
                                            .when(index == 0, |chip| {
                                                chip.border_color(colors.warning)
                                            })
                                            .child(
                                                h_flex()
                                                    .gap(GAP_SM)
                                                    .child(
                                                        Label::new(format!("{}", index + 1))
                                                            .text_xs()
                                                            .font_family(mono.clone())
                                                            .text_color(colors.muted_foreground),
                                                    )
                                                    .child(
                                                        Label::new(label)
                                                            .text_xs()
                                                            .font_family(mono.clone()),
                                                    ),
                                            )
                                            .on_click(move |_, _, cx| {
                                                open_config_builder_in_chain(
                                                    Some(path.clone()),
                                                    ancestors.clone(),
                                                    cx,
                                                )
                                            })
                                    }),
                            ),
                        ),
                )
            })
    }

    fn render_node(
        &self,
        node: &SecondaryConfigNode,
        numbering: String,
        position: NodePosition,
        open_paths: &HashSet<PathBuf>,
        rows: &mut Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) {
        let NodePosition {
            parent,
            ancestors,
            depth,
            top_level,
        } = position;
        let path = node.path.clone();
        let expanded = !self.collapsed_chain.contains(&path);
        let has_children = !node.children.is_empty();
        let error = node.error.is_some();
        let metadata = node.error.clone().unwrap_or_else(|| node_metadata(node));
        let colors = cx.theme().colors;
        let mono = cx.theme().mono_font_family.clone();
        // Ids must differ between appearances of one config (`×2`), so the numbering is part.
        let key = format!("{numbering}-{}", path.display());

        let mut row = h_flex()
            .w_full()
            .min_w(px(0.))
            .gap(GAP_SM)
            .items_center()
            .p_2()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(if error {
                colors.danger
            } else if depth == 0 {
                colors.warning
            } else {
                colors.border
            })
            .bg(colors.background)
            .child(
                Label::new(numbering.clone())
                    .text_xs()
                    .font_family(mono.clone())
                    .text_color(colors.muted_foreground),
            );
        if has_children {
            let toggle_path = path.clone();
            row = row.child(
                app_button(SharedString::from(format!("toggle-chain-{key}")))
                    .compact()
                    .icon(if expanded {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .tooltip(if expanded { "Collapse" } else { "Expand" })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_chain(toggle_path.clone(), cx)
                    })),
            );
        } else {
            row = row.child(div().w(CHEVRON_SLOT));
        }

        let title = h_flex()
            .gap(GAP_SM)
            .items_center()
            .child(Label::new(node.label.clone()).text_sm().font_semibold())
            // `×2`: Workbench runs this config once per appearance in the chain (§08, Q6).
            .when(node.times > 1, |this| {
                this.child(
                    app_tag().child(
                        Label::new(format!("×{}", node.times))
                            .text_xs()
                            .font_family(mono.clone()),
                    ),
                )
            })
            // "● OPEN" marks a config that has a builder window right now (`2a`), not every
            // healthy node.
            .when(open_paths.contains(&path), |this| {
                this.child(
                    Label::new("● OPEN")
                        .text_xs()
                        .font_semibold()
                        .text_color(colors.warning),
                )
            });
        row = row.child(
            v_flex().flex_1().min_w(px(0.)).child(title).child(
                Label::new(metadata)
                    .text_xs()
                    .font_family(mono.clone())
                    .text_color(if error {
                        colors.danger
                    } else {
                        colors.muted_foreground
                    }),
            ),
        );

        if let Some((index, siblings)) = top_level {
            row = row
                .child(
                    ghost_button(SharedString::from(format!("move-up-{key}")))
                        .icon(IconName::ArrowUp)
                        .tooltip("Run earlier")
                        .disabled(index == 0)
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.move_top_level(index, true, cx)),
                        ),
                )
                .child(
                    ghost_button(SharedString::from(format!("move-down-{key}")))
                        .icon(IconName::ArrowDown)
                        .tooltip("Run later")
                        .disabled(index + 1 >= siblings)
                        .on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.move_top_level(index, false, cx)
                            }),
                        ),
                );
        }

        if error {
            // `1d`: "moved or deleted — relink it or remove this step".
            let relink_parent = parent.clone();
            let old = path.clone();
            row = row.child(
                ghost_button(SharedString::from(format!("relink-{key}")))
                    .label("Relink…")
                    .tooltip("Point this step at the config's new location")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.pick_and_relink(relink_parent.clone(), old.clone(), window, cx)
                    })),
            );
        } else {
            let open_path = path.clone();
            let open_ancestors = ancestors.clone();
            let mut child_ancestors = ancestors.clone();
            child_ancestors.push(path.clone());
            let link_owner = path.clone();
            row = row
                .child(
                    ghost_button(SharedString::from(format!("open-chain-{key}")))
                        .label("Open")
                        .on_click(move |_, _, cx| {
                            open_config_builder_in_chain(
                                Some(open_path.clone()),
                                open_ancestors.clone(),
                                cx,
                            )
                        }),
                )
                .child(
                    ghost_button(SharedString::from(format!("add-under-{key}")))
                        .label("New under")
                        .tooltip("Create a config that runs after this one")
                        .on_click(move |_, _, cx| {
                            open_config_builder_in_chain(None, child_ancestors.clone(), cx)
                        }),
                )
                .child(
                    ghost_button(SharedString::from(format!("link-under-{key}")))
                        .label("Link under…")
                        .tooltip("Run an existing config after this one")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.pick_and_link(Some(link_owner.clone()), window, cx)
                        })),
                );
        }
        let unlink_parent = parent.clone();
        let unlink_child = path.clone();
        row = row.child(
            ghost_button(SharedString::from(format!("unlink-chain-{key}")))
                .label("Unlink")
                .tooltip("Unlink — the config stays in the library")
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.unlink_node(unlink_parent.clone(), unlink_child.clone(), cx)
                })),
        );

        rows.push(
            div()
                .w_full()
                .pl(INDENT_STEP * depth as f32)
                .when(depth > 0, |this| {
                    this.border_l_1().border_color(colors.border)
                })
                .child(row)
                .into_any_element(),
        );
        if expanded {
            for (index, child) in node.children.iter().enumerate() {
                let mut child_ancestors = ancestors.clone();
                child_ancestors.push(node.path.clone());
                self.render_node(
                    child,
                    format!("{numbering}.{}", index + 1),
                    NodePosition {
                        parent: Some(node.path.clone()),
                        ancestors: child_ancestors,
                        depth: depth + 1,
                        top_level: None,
                    },
                    open_paths,
                    rows,
                    cx,
                );
            }
        }
    }

    /// Apply a `secondary_tasks` edit to `owner` (this window's draft when `None`), and report
    /// failure in the chain panel. Success clears the last error.
    fn edit_chain(
        &mut self,
        owner: Option<PathBuf>,
        edit: impl FnOnce(&mut ConfigDraft, Option<&Path>) -> bool,
        cx: &mut Context<Self>,
    ) {
        let result = match owner {
            Some(owner) => self.apply_chain_edit(&owner, edit, cx),
            None => {
                let base = workbench_dir(cx);
                if edit(&mut self.draft, base.as_deref()) {
                    self.forget_widgets("secondary_tasks");
                    self.last_chain_scan = None;
                    self.revalidate(cx);
                }
                Ok(())
            }
        };
        self.chain_error = result.err().map(Into::into);
        cx.notify();
    }

    /// Unlink `child` from whichever config runs it.
    fn unlink_node(&mut self, parent: Option<PathBuf>, child: PathBuf, cx: &mut Context<Self>) {
        self.edit_chain(
            parent,
            move |draft, base| draft.unlink_secondary_task(&child, base),
            cx,
        );
    }

    /// Reorder this window's own top-level steps.
    fn move_top_level(&mut self, index: usize, up: bool, cx: &mut Context<Self>) {
        self.edit_chain(
            None,
            move |draft, _| draft.move_secondary_task(index, up),
            cx,
        );
    }

    /// Ask for a config file, then run `then` with it.
    fn pick_config(
        &mut self,
        prompt: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, PathBuf, &mut Context<Self>) + 'static,
    ) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(prompt.to_string().into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(path) = paths.first().cloned()
            {
                this.update(cx, |this, cx| then(this, path, cx)).ok();
            }
        })
        .detach();
    }

    /// "Link an existing config" (under this config when `owner` is `None`) and "Link under…"
    /// (under a node).
    fn pick_and_link(
        &mut self,
        owner: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pick_config(
            "Select a config to run after this one",
            window,
            cx,
            move |this, path, cx| this.link_into(owner, path, cx),
        );
    }

    fn link_into(&mut self, owner: Option<PathBuf>, path: PathBuf, cx: &mut Context<Self>) {
        let base = workbench_dir(cx);
        // Judge the link against the owner as it stands — this draft, the owner's open
        // window, or its file.
        let own = owner.as_deref().is_none_or(|owner| self.owns(owner));
        let owner_draft = if own {
            Some(self.draft.clone())
        } else {
            owner.as_deref().and_then(|owner| {
                cx.global::<ConfigBuilderWindows>()
                    .view_for(owner)
                    .map(|view| view.read(cx).draft.clone())
                    .or_else(|| ConfigDraft::load(owner).ok())
            })
        };
        let Some(owner_draft) = owner_draft else {
            self.chain_error = Some("Couldn't read the config to link under.".into());
            cx.notify();
            return;
        };
        if let Some(error) = config::chain::link_error(&owner_draft, &path, base.as_deref()) {
            self.chain_error = Some(error.into());
            cx.notify();
            return;
        }
        let already = owner_draft
            .resolved_secondary_tasks(base.as_deref())
            .iter()
            .any(|existing| config::chain::same_path(existing, &path));
        if already {
            self.chain_error = Some("That config already runs right after this one.".into());
            cx.notify();
            return;
        }
        let owner = if own { None } else { owner };
        // Stored absolute, like a link made by "Create a new one" (plan 1.4).
        self.edit_chain(
            owner,
            move |draft, base| draft.link_secondary_task(&path, base),
            cx,
        );
    }

    /// Relink a broken step: replace the entry for `old` with a picked file.
    fn pick_and_relink(
        &mut self,
        parent: Option<PathBuf>,
        old: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pick_config(
            "Select the config's new location",
            window,
            cx,
            move |this, new, cx| {
                let parent = parent.filter(|parent| !this.owns(parent));
                this.edit_chain(
                    parent,
                    move |draft, base| draft.replace_secondary_task(&old, &new, base),
                    cx,
                );
            },
        );
    }

    fn owns(&self, path: &Path) -> bool {
        self.draft
            .path
            .as_deref()
            .is_some_and(|own| config::chain::same_path(own, path))
    }

    fn toggle_chain(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if !self.collapsed_chain.remove(&path) {
            self.collapsed_chain.insert(path);
        }
        cx.notify();
    }
}

/// `pages.yml · add_media · 4 settings · 2 children` under a node's name.
fn node_metadata(node: &SecondaryConfigNode) -> String {
    let task = node.task.as_deref().unwrap_or("task not set");
    let children = match node.children.len() {
        0 => String::new(),
        1 => " · 1 child".to_string(),
        n => format!(" · {n} children"),
    };
    format!(
        "{} · {task} · {} setting{}{children}",
        node.path.display(),
        node.settings,
        if node.settings == 1 { "" } else { "s" },
    )
}

fn chain_stats(nodes: &[SecondaryConfigNode], depth: usize) -> (usize, usize) {
    nodes
        .iter()
        .fold((0, depth.saturating_sub(1)), |(count, max_depth), node| {
            let (children, child_depth) = chain_stats(&node.children, depth + 1);
            (count + 1 + children, max_depth.max(child_depth))
        })
}

fn collect_paths(node: &SecondaryConfigNode, collapsed: &mut HashSet<PathBuf>) {
    collapsed.insert(node.path.clone());
    for child in &node.children {
        collect_paths(child, collapsed);
    }
}

/// The run order: `(label, path)` depth-first, broken nodes and what is under them left out.
fn flattened(nodes: &[SecondaryConfigNode]) -> Vec<(String, PathBuf)> {
    fn visit(node: &SecondaryConfigNode, out: &mut Vec<(String, PathBuf)>) {
        if node.error.is_none() {
            out.push((node.label.clone(), node.path.clone()));
            for child in &node.children {
                visit(child, out);
            }
        }
    }
    let mut out = Vec::new();
    for node in nodes {
        visit(node, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    // Not `use super::*`: that glob-imports gpui, whose `test` macro shadows the built-in one.
    use super::{SecondaryConfigNode, flattened, node_metadata};
    use std::path::PathBuf;

    fn node(label: &str, error: bool, children: Vec<SecondaryConfigNode>) -> SecondaryConfigNode {
        SecondaryConfigNode {
            path: PathBuf::from(format!("/c/{label}.yml")),
            label: label.into(),
            task: Some("create".into()),
            settings: 1,
            children,
            error: error.then(|| "moved".to_string()),
            times: 1,
        }
    }

    #[test]
    fn run_order_skips_broken_branches_and_keeps_paths() {
        let tree = vec![
            node("a", false, vec![node("a1", false, vec![])]),
            node("b", true, vec![node("b1", false, vec![])]),
            node("c", false, vec![]),
        ];
        let order: Vec<String> = flattened(&tree).into_iter().map(|(l, _)| l).collect();
        assert_eq!(order, ["a", "a1", "c"]);
        assert_eq!(flattened(&tree)[2].1, PathBuf::from("/c/c.yml"));
    }

    #[test]
    fn metadata_counts_children_in_words() {
        let one = node("a", false, vec![node("x", false, vec![])]);
        assert_eq!(
            node_metadata(&one),
            "/c/a.yml · create · 1 setting · 1 child"
        );
    }
}
