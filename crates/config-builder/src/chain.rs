//! `secondary_tasks` as the flat ordered list of mockup `1d`.
//!
//! The parent keeps the chain: numbered, unlinkable, with a broken link called out in place
//! rather than at save time. Opening a child opens another builder window on that file; host
//! and credentials are inherited from the parent at run time, so they are never set twice.
//!
//! Nesting deeper than one level, the run-order strip and the loop guardrails are Stage 6 —
//! see `docs/plans/stage-6-chain-map.md`.

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme, IconName, Sizable, StyledExt, button::ButtonVariants, h_flex, label::Label, v_flex,
};

use super::{ConfigBuilder, open_config_builder};
use ui::app_button;

impl ConfigBuilder {
    pub(super) fn render_chain(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let tasks = self.draft.resolved_secondary_tasks();
        let now = Instant::now();
        if self
            .last_chain_scan
            .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(2))
        {
            self.chain_file_states = tasks
                .iter()
                .map(|path| (path.clone(), path.is_file()))
                .collect();
            self.last_chain_scan = Some(now);
        }
        let links: Vec<AnyElement> = tasks
            .iter()
            .enumerate()
            .map(|(i, path)| self.render_link(i, path, cx).into_any_element())
            .collect();

        v_flex()
            .w_full()
            .gap_2()
            .pt_2()
            .child(
                Label::new("Secondary configs run after this one finishes")
                    .text_sm()
                    .text_color(cx.theme().muted_foreground),
            )
            .children(links)
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        app_button("link-config")
                            .outline()
                            .small()
                            .label("Link an existing config")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.link_existing(window, cx);
                            })),
                    )
                    .child(
                        app_button("create-child-config")
                            .outline()
                            .small()
                            .label("Create a new one")
                            .on_click(|_, _, cx| open_config_builder(None, cx)),
                    ),
            )
    }

    fn render_link(&self, index: usize, path: &Path, cx: &mut Context<Self>) -> impl IntoElement {
        let exists = self.chain_file_states.get(path).copied().unwrap_or(false);
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let owned = path.to_path_buf();

        h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .p_2()
            .rounded(cx.theme().radius)
            .bg(cx.theme().colors.secondary)
            .child(
                Label::new(format!("{}", index + 1))
                    .text_xs()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(
                v_flex()
                    .flex_1()
                    .child(Label::new(name).text_sm().font_semibold())
                    .child(
                        Label::new(if exists {
                            path.to_string_lossy().into_owned()
                        } else {
                            format!(
                                "{} was moved or deleted — relink it or remove this step.",
                                path.display()
                            )
                        })
                        .text_xs()
                        .text_color(if exists {
                            cx.theme().muted_foreground
                        } else {
                            cx.theme().colors.danger
                        }),
                    ),
            )
            .when(exists, |this| {
                this.child(
                    app_button(SharedString::from(format!("open-child-{index}")))
                        .ghost()
                        .xsmall()
                        .label("Open")
                        .on_click(move |_, _, cx| open_config_builder(Some(owned.clone()), cx)),
                )
            })
            .child(
                app_button(SharedString::from(format!("unlink-child-{index}")))
                    .ghost()
                    .xsmall()
                    .icon(IconName::Close)
                    .tooltip("Unlink — the config stays in the library")
                    .on_click(cx.listener(move |this, _, _, cx| this.unlink(index, cx))),
            )
    }

    fn unlink(&mut self, index: usize, cx: &mut Context<Self>) {
        let mut tasks = self.draft.secondary_tasks();
        if index < tasks.len() {
            tasks.remove(index);
        }
        self.draft.set_secondary_tasks(&tasks);
        self.forget_widgets("secondary_tasks");
        self.revalidate(cx);
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
        let mut tasks = self.draft.secondary_tasks();
        // A config that runs itself never terminates; `validate` also catches this, but there
        // is no reason to let the link be made in the first place.
        if self.draft.path.as_deref() == Some(path.as_path()) || tasks.contains(&path) {
            self.notice = Some("That config already runs in this chain.".into());
            cx.notify();
            return;
        }
        self.chain_file_states.insert(path.clone(), path.is_file());
        tasks.push(path);
        self.draft.set_secondary_tasks(&tasks);
        self.forget_widgets("secondary_tasks");
        self.revalidate(cx);
    }
}
