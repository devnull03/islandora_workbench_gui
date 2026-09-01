//! Recursive `secondary_tasks` editor (mockups `1d` and `2a`).

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{ActiveTheme, IconName, StyledExt, h_flex, label::Label, v_flex};
use ui::tokens::{CHEVRON_SLOT, GAP_XS, INDENT_STEP};
use ui::{Card, CardTone, app_button, ghost_button};
use workbench_integration::config::{self, chain::SecondaryConfigNode};

use super::{ConfigBuilder, open_child_config_builder, open_config_builder};

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
            self.chain_nodes = config::chain::child_nodes(&self.draft);
            self.last_chain_scan = Some(now);
        }

        let (count, max_depth) = chain_stats(&self.chain_nodes, 1);
        let mut rows = Vec::new();
        for (index, node) in self.chain_nodes.iter().enumerate() {
            self.render_node(
                node,
                format!("{}", index + 1),
                self.draft.path.clone(),
                0,
                &mut rows,
                cx,
            );
        }
        // The graph was loaded above on its two-second cadence; derive this display from the
        // cached nodes so rendering does not re-read every child YAML.
        let run_order = flattened_labels(&self.chain_nodes);
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

        v_flex()
            .w_full()
            .gap_2()
            .pt_2()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(Label::new("Secondary configs").text_sm().font_semibold())
                    .child(
                        Label::new(summary)
                            .text_xs()
                            .text_color(cx.theme().muted_foreground),
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
                    .gap_2()
                    .child(
                        app_button("link-config")
                            .outline()
                            .label("Link an existing config")
                            .on_click(
                                cx.listener(|this, _, window, cx| this.link_existing(window, cx)),
                            ),
                    )
                    .child(
                        app_button("create-child-config")
                            .outline()
                            .label("Create a new one")
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(parent) = this.draft.path.clone() {
                                    open_child_config_builder(parent, cx);
                                } else {
                                    this.notice = Some(
                                        "Save this config before creating a nested config.".into(),
                                    );
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .when(!run_order.is_empty(), |this| {
                this.child(
                    Card::new()
                        .tone(CardTone::Filled)
                        .gap(GAP_XS)
                        .child(Label::new("Run order").text_xs().font_semibold())
                        .child(
                            Label::new(run_order.join("  →  "))
                                .text_xs()
                                .text_color(cx.theme().muted_foreground),
                        ),
                )
            })
    }

    fn render_node(
        &self,
        node: &SecondaryConfigNode,
        numbering: String,
        parent: Option<PathBuf>,
        depth: usize,
        rows: &mut Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) {
        let path = node.path.clone();
        let open = !self.collapsed_chain.contains(&path);
        let has_children = !node.children.is_empty();
        let metadata = node.error.clone().unwrap_or_else(|| {
            let task = node.task.as_deref().unwrap_or("task not set");
            format!(
                "{} · {task} · {} setting{}{}",
                node.path.display(),
                node.settings,
                if node.settings == 1 { "" } else { "s" },
                if node.children.is_empty() {
                    String::new()
                } else {
                    format!(
                        " · {} child{}",
                        node.children.len(),
                        if node.children.len() == 1 { "" } else { "ren" }
                    )
                }
            )
        });
        let error = node.error.is_some();
        let child_path = node.path.clone();
        let parent_path = parent.clone();
        let mut row = h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .pl(INDENT_STEP * depth as f32)
            .p_2()
            .rounded(cx.theme().radius)
            .bg(if depth == 0 {
                cx.theme().colors.secondary
            } else {
                cx.theme().colors.background
            })
            .child(
                Label::new(numbering.clone())
                    .text_xs()
                    .text_color(cx.theme().muted_foreground),
            );
        if has_children {
            let toggle_path = path.clone();
            row = row.child(
                app_button(SharedString::from(format!(
                    "toggle-chain-{}",
                    path.display()
                )))
                .icon(if open {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                })
                .on_click(
                    cx.listener(move |this, _, _, cx| this.toggle_chain(toggle_path.clone(), cx)),
                ),
            );
        } else {
            row = row.child(div().w(CHEVRON_SLOT));
        }
        row = row
            .child(
                v_flex()
                    .flex_1()
                    .min_w(px(0.))
                    .child(Label::new(node.label.clone()).text_sm().font_semibold())
                    .child(Label::new(metadata).text_xs().text_color(if error {
                        cx.theme().colors.danger
                    } else {
                        cx.theme().muted_foreground
                    })),
            )
            .when(!error, |this| {
                let open_path = child_path.clone();
                let add_under_path = open_path.clone();
                this.child(
                    ghost_button(SharedString::from(format!(
                        "open-chain-{}",
                        open_path.display()
                    )))
                    .label("Open")
                    .on_click(move |_, _, cx| open_config_builder(Some(open_path.clone()), cx)),
                )
                .child(
                    ghost_button(SharedString::from(format!(
                        "add-under-{}",
                        add_under_path.display()
                    )))
                    .label("Add under")
                    .on_click(move |_, _, cx| {
                        open_child_config_builder(add_under_path.clone(), cx)
                    }),
                )
            })
            .child(
                ghost_button(SharedString::from(format!(
                    "unlink-chain-{}",
                    child_path.display()
                )))
                .icon(IconName::Close)
                .tooltip("Unlink — the config stays in the library")
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.unlink_node(parent_path.clone(), child_path.clone(), cx)
                })),
            );
        rows.push(row.into_any_element());
        if open {
            for (index, child) in node.children.iter().enumerate() {
                self.render_node(
                    child,
                    format!("{numbering}.{}", index + 1),
                    Some(node.path.clone()),
                    depth + 1,
                    rows,
                    cx,
                );
            }
        }
    }

    fn unlink_node(&mut self, parent: Option<PathBuf>, child: PathBuf, cx: &mut Context<Self>) {
        if let Some(parent) = parent {
            let Ok(mut draft) = config::ConfigDraft::load(&parent) else {
                self.notice =
                    Some(format!("Couldn't open {} to unlink the child.", parent.display()).into());
                cx.notify();
                return;
            };
            let mut tasks = draft.secondary_tasks();
            tasks.retain(|path| !same_path(&draft, path, &child));
            draft.set_secondary_tasks(&tasks);
            if let Err(error) = draft.save(&parent) {
                self.notice = Some(format!("Couldn't save {}: {error}", parent.display()).into());
            }
        } else {
            let mut tasks = self.draft.secondary_tasks();
            tasks.retain(|path| !same_path(&self.draft, path, &child));
            self.draft.set_secondary_tasks(&tasks);
            self.forget_widgets("secondary_tasks");
            self.revalidate(cx);
        }
        self.last_chain_scan = None;
        cx.notify();
    }

    fn link_existing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select a config to run after this one".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(path) = paths.first().cloned()
            {
                this.update(cx, |this, cx| this.push_link(path, cx)).ok();
            }
        })
        .detach();
    }

    fn push_link(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if let Some(error) = config::chain::link_error(&self.draft, &path) {
            self.notice = Some(error.into());
            cx.notify();
            return;
        }
        let mut tasks = self.draft.secondary_tasks();
        if tasks
            .iter()
            .any(|existing| same_path(&self.draft, existing, &path))
        {
            self.notice = Some("That config already runs in this chain.".into());
            cx.notify();
            return;
        }
        tasks.push(path);
        self.draft.set_secondary_tasks(&tasks);
        self.forget_widgets("secondary_tasks");
        self.last_chain_scan = None;
        self.revalidate(cx);
    }

    fn toggle_chain(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if !self.collapsed_chain.remove(&path) {
            self.collapsed_chain.insert(path);
        }
        cx.notify();
    }
}

fn chain_stats(nodes: &[SecondaryConfigNode], depth: usize) -> (usize, usize) {
    nodes
        .iter()
        .fold((0, depth.saturating_sub(1)), |(count, max_depth), node| {
            let (children, child_depth) = chain_stats(&node.children, depth + 1);
            (count + 1 + children, max_depth.max(child_depth))
        })
}

fn collect_paths(node: &SecondaryConfigNode, collapsed: &mut std::collections::HashSet<PathBuf>) {
    collapsed.insert(node.path.clone());
    for child in &node.children {
        collect_paths(child, collapsed);
    }
}

fn flattened_labels(nodes: &[SecondaryConfigNode]) -> Vec<String> {
    fn visit(node: &SecondaryConfigNode, labels: &mut Vec<String>) {
        if node.error.is_none() {
            labels.push(node.label.clone());
            for child in &node.children {
                visit(child, labels);
            }
        }
    }
    let mut labels = Vec::new();
    for node in nodes {
        visit(node, &mut labels);
    }
    labels
}

fn same_path(owner: &config::ConfigDraft, left: &Path, right: &Path) -> bool {
    let left = if left.is_absolute() {
        left.to_path_buf()
    } else {
        owner
            .path
            .as_deref()
            .and_then(Path::parent)
            .map_or_else(|| left.to_path_buf(), |base| base.join(left))
    };
    std::fs::canonicalize(&left).unwrap_or(left)
        == std::fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf())
}
