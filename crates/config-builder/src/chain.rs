//! Recursive `secondary_tasks` editor (mockups `1d` and `2a`).

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable, Size, StyledExt, alert::Alert, h_flex, label::Label,
    v_flex,
};
use ui::tokens::{CHEVRON_SLOT, GAP_MD, GAP_SM, INDENT_STEP};
use ui::{Card, CardTone, app_button, app_tag, ghost_button};
use workbench_integration::config::{self, chain::SecondaryConfigNode};

use super::{BuilderChromeEvent, ConfigBuilder, open_config_builder_in_chain};

struct NodePosition {
    parent: Option<PathBuf>,
    ancestors: Vec<PathBuf>,
    depth: usize,
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
            self.chain_nodes = config::chain::child_nodes(&self.draft);
            self.last_chain_scan = Some(now);
        }

        let (count, max_depth) = chain_stats(&self.chain_nodes, 1);
        let mut current_ancestry = self.ancestors.clone();
        if let Some(path) = self.draft.path.clone() {
            current_ancestry.push(path);
        }
        let mut rows = Vec::new();
        for (index, node) in self.chain_nodes.iter().enumerate() {
            self.render_node(
                node,
                format!("{}", index + 1),
                NodePosition {
                    parent: self.draft.path.clone(),
                    ancestors: current_ancestry.clone(),
                    depth: 0,
                },
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
                            .text_color(cx.theme().colors.warning),
                    )
                    .child(
                        Label::new("CHAIN")
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().colors.warning),
                    )
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
                    .gap(GAP_MD)
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
                                    let mut ancestors = this.ancestors.clone();
                                    ancestors.push(parent);
                                    open_config_builder_in_chain(None, ancestors, cx);
                                } else {
                                    this.chrome.transition(BuilderChromeEvent::Failed {
                                        title: "Save required".into(),
                                        detail: "Save this config before creating a nested config."
                                            .into(),
                                    });
                                    cx.notify();
                                }
                            })),
                    ),
            );

        v_flex()
            .w_full()
            .gap(GAP_MD)
            .pt_2()
            .child(chain_panel)
            // §08 warns at depth 4 and beyond and does not block. A chain that deep is
            // usually a mistake and occasionally exactly what someone meant; the app is not
            // in a position to tell which.
            .when(max_depth >= 4, |this| {
                let colors = cx.theme().colors;
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
                let colors = cx.theme().colors;
                let mono = cx.theme().mono_font_family.clone();
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
                        // is the one you most need to read the end of.
                        .child(h_flex().w_full().gap(GAP_SM).flex_wrap().children(
                            run_order.into_iter().enumerate().map(|(index, label)| {
                                app_tag()
                                    .gap(GAP_SM)
                                    .flex_none()
                                    .when(index == 0, |tag| tag.border_color(colors.warning))
                                    .bg(colors.background)
                                    .child(
                                        Label::new(format!("{}", index + 1))
                                            .text_xs()
                                            .font_family(mono.clone())
                                            .text_color(colors.muted_foreground),
                                    )
                                    .child(Label::new(label).text_xs().font_family(mono.clone()))
                            }),
                        )),
                )
            })
    }

    fn render_node(
        &self,
        node: &SecondaryConfigNode,
        numbering: String,
        position: NodePosition,
        rows: &mut Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) {
        let NodePosition {
            parent,
            ancestors,
            depth,
        } = position;
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
        let colors = cx.theme().colors;
        let mut row = h_flex()
            .w_full()
            .min_w(px(0.))
            .gap(GAP_SM)
            .items_center()
            .p_2()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(if error || depth == 0 {
                if error { colors.danger } else { colors.warning }
            } else {
                colors.border
            })
            .bg(colors.background)
            .child(
                Label::new(numbering.clone())
                    .text_xs()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(colors.muted_foreground),
            );
        if has_children {
            let toggle_path = path.clone();
            row = row.child(
                app_button(SharedString::from(format!(
                    "toggle-chain-{}",
                    path.display()
                )))
                .compact()
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
                    .child(
                        h_flex()
                            .gap(GAP_SM)
                            .items_center()
                            .child(Label::new(node.label.clone()).text_sm().font_semibold())
                            .when(!error, |this| {
                                this.child(
                                    Label::new("● OPEN")
                                        .text_xs()
                                        .font_semibold()
                                        .text_color(colors.warning),
                                )
                            }),
                    )
                    .child(
                        Label::new(metadata)
                            .text_xs()
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_color(if error {
                                colors.danger
                            } else {
                                colors.muted_foreground
                            }),
                    ),
            )
            .when(!error, |this| {
                let open_path = child_path.clone();
                let open_ancestors = ancestors.clone();
                let add_under_path = open_path.clone();
                let mut child_ancestors = ancestors.clone();
                child_ancestors.push(add_under_path.clone());
                this.child(
                    ghost_button(SharedString::from(format!(
                        "open-chain-{}",
                        open_path.display()
                    )))
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
                    ghost_button(SharedString::from(format!(
                        "add-under-{}",
                        add_under_path.display()
                    )))
                    .label("Add under")
                    .on_click(move |_, _, cx| {
                        open_config_builder_in_chain(None, child_ancestors.clone(), cx)
                    }),
                )
            })
            .child(
                ghost_button(SharedString::from(format!(
                    "unlink-chain-{}",
                    child_path.display()
                )))
                .label("Unlink")
                .tooltip("Unlink — the config stays in the library")
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.unlink_node(parent_path.clone(), child_path.clone(), cx)
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
        if open {
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
                    },
                    rows,
                    cx,
                );
            }
        }
    }

    fn unlink_node(&mut self, parent: Option<PathBuf>, child: PathBuf, cx: &mut Context<Self>) {
        if let Some(parent) = parent {
            let Ok(mut draft) = config::ConfigDraft::load(&parent) else {
                self.chrome.transition(BuilderChromeEvent::Failed {
                    title: "Unlink failed".into(),
                    detail: format!("Couldn't open {} to unlink the child.", parent.display())
                        .into(),
                });
                cx.notify();
                return;
            };
            let mut tasks = draft.secondary_tasks();
            tasks.retain(|path| !same_path(&draft, path, &child));
            draft.set_secondary_tasks(&tasks);
            if let Err(error) = draft.save(&parent) {
                self.chrome.transition(BuilderChromeEvent::Failed {
                    title: "Unlink failed".into(),
                    detail: format!("Couldn't save {}: {error}", parent.display()).into(),
                });
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
            self.chrome.transition(BuilderChromeEvent::Failed {
                title: "Link failed".into(),
                detail: error.into(),
            });
            cx.notify();
            return;
        }
        let mut tasks = self.draft.secondary_tasks();
        if tasks
            .iter()
            .any(|existing| same_path(&self.draft, existing, &path))
        {
            self.chrome.transition(BuilderChromeEvent::Failed {
                title: "Link blocked".into(),
                detail: "That config already runs in this chain.".into(),
            });
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
