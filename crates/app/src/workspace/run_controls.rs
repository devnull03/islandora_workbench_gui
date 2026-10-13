//! Step 3's run controls: Check / Run Ingest / Cancel, the run options, the "More" menu of
//! Workbench's other modes, and the strip that says what the last run left behind.
//!
//! Mockup `2c` draws only Check and Run Ingest. Everything else Workbench can do from its
//! command line (docs: "Checking configuration and input data", "Rolling back", "Recovery mode",
//! "Quick delete", "CSV file templates", "Contact sheet", "Cancelling") is reachable from here
//! without adding a second row of primary buttons: options fold away, the rarer modes live in
//! one menu, and follow-ups appear only when the last run produced something to act on.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme, Disableable, IconName, Sizable, WindowExt as _,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    h_flex,
    input::Input,
    label::Label,
    menu::{DropdownMenu, PopupMenuItem},
    v_flex,
};
use ui::tokens::{CARD_PAD, GAP_MD, GAP_SM, NUMBER_FIELD_W};
use ui::{APP_CONTROL_SIZE, Card, CardTone, app_button, ghost_button};
use workbench_integration::WorkbenchAction;

use super::run::{LastRun, RunTarget};
use super::{Operation, WorkflowStage, Workspace};
use crate::helpers::{open_file, reveal_in_folder};

/// What a "More" menu entry does to the workspace.
type MenuAction = Rc<dyn Fn(&mut Workspace, &mut Window, &mut Context<Workspace>)>;

impl Workspace {
    /// The action row: options, the menu, and the two (or, while running, three) buttons.
    pub(super) fn render_run_actions(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let idle = self.is_idle();
        let ready = self.workbench_ready(cx);
        let running = self.running.as_ref().map(|s| s.action.clone());
        let check_loading = matches!(running, Some(WorkbenchAction::Check { .. }));
        let run_loading = matches!(running, Some(WorkbenchAction::Run));
        let run_enabled = idle && self.stage == WorkflowStage::CheckPassed;
        let cancelling = self.running.as_ref().is_some_and(|s| s.cancelling);
        let this = cx.entity().downgrade();

        h_flex()
            .gap_2()
            .items_center()
            .child(
                ghost_button("run-options")
                    .label("Options")
                    .icon(if self.options_open {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .tooltip("Row range, prompts and contact sheet for the next run")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.options_open = !this.options_open;
                        cx.notify();
                    })),
            )
            .child(
                app_button("more-workbench")
                    .outline()
                    .icon(IconName::Ellipsis)
                    .tooltip("More Workbench actions")
                    .disabled(!idle || !ready)
                    .dropdown_menu(move |menu, _, _| {
                        let item = |label: &str, action: MenuAction| {
                            let this = this.clone();
                            PopupMenuItem::new(label.to_string()).on_click(move |_, window, cx| {
                                let action = action.clone();
                                this.update(cx, |workspace, cx| action(workspace, window, cx))
                                    .ok();
                            })
                        };
                        menu.item(item(
                            "Get CSV template",
                            Rc::new(|this, window, cx| {
                                this.start_workbench(
                                    WorkbenchAction::CsvTemplate,
                                    RunTarget::default(),
                                    window,
                                    cx,
                                )
                            }),
                        ))
                        .item(item(
                            "Print effective config",
                            Rc::new(|this, window, cx| {
                                this.start_workbench(
                                    WorkbenchAction::PrintConfig,
                                    RunTarget::default(),
                                    window,
                                    cx,
                                )
                            }),
                        ))
                        .separator()
                        .item(item(
                            "Quick delete a node…",
                            Rc::new(|this, window, cx| this.ask_quick_delete(false, window, cx)),
                        ))
                        .item(item(
                            "Quick delete a media…",
                            Rc::new(|this, window, cx| this.ask_quick_delete(true, window, cx)),
                        ))
                        .separator()
                        .item(item(
                            "Export the CSV ID map…",
                            Rc::new(|this, window, cx| this.ask_map_export(window, cx)),
                        ))
                        .item(item(
                            "Export duplicate IDs from the map…",
                            Rc::new(|this, window, cx| this.ask_map_export_duplicates(window, cx)),
                        ))
                        .item(item(
                            "Merge another map into this one…",
                            Rc::new(|this, window, cx| this.ask_map_merge(window, cx)),
                        ))
                        .item(item(
                            "Remove map entries before a date…",
                            Rc::new(|this, window, cx| {
                                this.ask_map_remove_by_date(false, window, cx)
                            }),
                        ))
                        .item(item(
                            "Remove map entries after a date…",
                            Rc::new(|this, window, cx| {
                                this.ask_map_remove_by_date(true, window, cx)
                            }),
                        ))
                        .item(item(
                            "Remove this config's map entries…",
                            Rc::new(|this, window, cx| this.ask_map_remove_for_config(window, cx)),
                        ))
                        .item(item(
                            "Remove map entries for deleted nodes…",
                            Rc::new(|this, window, cx| this.ask_map_remove_deleted(window, cx)),
                        ))
                        .separator()
                        .item(item(
                            "Open the Workbench folder",
                            Rc::new(|this, window, cx| {
                                if let Ok(info) = super::run::workbench_info(cx) {
                                    this.reveal(&info.install_path, window, cx);
                                }
                            }),
                        ))
                    }),
            )
            .when_some(self.running.is_some().then_some(()), |row, _| {
                row.child(
                    app_button("cancel-run")
                        .outline()
                        .danger()
                        .label(if cancelling { "Stop now" } else { "Cancel" })
                        .tooltip(if cancelling {
                            "Stop Workbench immediately"
                        } else {
                            "Stop Workbench (its ctrl-c): it logs where it got to and exits"
                        })
                        .on_click(cx.listener(|this, _, window, cx| this.cancel_run(window, cx))),
                )
            })
            .child(
                app_button("check")
                    .outline()
                    .label("Check")
                    .loading(check_loading)
                    .disabled(!idle || !ready)
                    .tooltip(if ready {
                        "Run Workbench's --check: validates the config and the CSV, changes nothing"
                    } else {
                        "Pick a config and a server first"
                    })
                    .on_click(cx.listener(|this, _, window, cx| this.run_check(window, cx))),
            )
            .child(
                app_button("run-ingest")
                    .primary()
                    .label("Run Ingest")
                    .loading(run_loading)
                    .disabled(!run_enabled)
                    .tooltip(if run_enabled {
                        "Run the task against the server"
                    } else {
                        "Unlocked by a Check that passes"
                    })
                    .on_click(cx.listener(|this, _, window, cx| this.run_task(window, cx))),
            )
    }

    /// The options a run takes on Workbench's command line rather than from the config.
    pub(super) fn render_run_options(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.options_open {
            return None;
        }
        let idle = self.is_idle();
        let muted = cx.theme().colors.muted_foreground;
        Some(
            Card::new()
                .tone(CardTone::Filled)
                .padding(CARD_PAD)
                .gap(GAP_MD)
                .child(
                    h_flex()
                        .gap(GAP_SM)
                        .items_center()
                        .flex_wrap()
                        .child(Label::new("CSV rows").text_sm())
                        .child(
                            div().w(NUMBER_FIELD_W).child(
                                Input::new(&self.start_row)
                                    .with_size(APP_CONTROL_SIZE)
                                    .disabled(!idle),
                            ),
                        )
                        .child(Label::new("to").text_sm().text_color(muted))
                        .child(
                            div().w(NUMBER_FIELD_W).child(
                                Input::new(&self.stop_row)
                                    .with_size(APP_CONTROL_SIZE)
                                    .disabled(!idle),
                            ),
                        )
                        .child(
                            Label::new("empty = all rows")
                                .text_xs()
                                .text_color(muted),
                        ),
                )
                .child(
                    Checkbox::new("skip-user-prompts")
                        .small()
                        .checked(self.skip_user_prompts)
                        .label("Skip the config's own prompts (user_prompts, remind_user_to_run_check)")
                        .disabled(!idle)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            this.skip_user_prompts = *checked;
                            cx.notify();
                        })),
                )
                .child(
                    Checkbox::new("contact-sheet")
                        .small()
                        .checked(self.contact_sheet)
                        .label("Write a contact sheet when checking (create tasks)")
                        .disabled(!idle)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            this.contact_sheet = *checked;
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }

    /// One line on how the last run ended, and the follow-ups it makes possible. Everything
    /// here was resolved when the run ended; rendering reads no files.
    pub(super) fn render_last_run(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.op != Operation::None {
            return None;
        }
        let run = self.last_run.as_ref()?;
        let colors = cx.theme().colors;
        let tone = if run.cancelled {
            colors.warning
        } else if run.succeeded {
            colors.success
        } else {
            colors.danger
        };
        let mut actions: Vec<AnyElement> = Vec::new();

        if let Some(sheet) = &run.report.contact_sheet {
            actions.push(self.open_button("open-contact-sheet", "Open contact sheet", sheet, cx));
        }
        if let Some(template) = &run.report.csv_template {
            actions.push(self.open_button("open-csv-template", "Open CSV template", template, cx));
        }
        if let Some(export) = run
            .report
            .exported_csv
            .as_ref()
            .filter(|_| run.output_csv.is_none())
        {
            actions.push(self.open_button("open-exported-csv", "Open exported CSV", export, cx));
        }
        if let Some(output) = &run.output_csv {
            actions.push(self.open_button("open-output-csv", "Open output CSV", output, cx));
        }
        if let Some(rollback) = run.rollback_config.clone() {
            let server = run.server_url.clone();
            actions.push(
                ghost_button("roll-back")
                    .label("Roll back…")
                    .tooltip("Delete the nodes and media this run created")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.ask_rollback(rollback.clone(), server.clone(), window, cx)
                    }))
                    .into_any_element(),
            );
        }
        if let Some(node) = run.resumable_from() {
            let config = run.config_path.clone();
            actions.push(
                ghost_button("resume-recovery")
                    .label(format!("Resume from node {node}…"))
                    .tooltip("Recovery mode: rerun the task, skipping rows already created")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.ask_resume(node, config.clone(), window, cx)
                    }))
                    .into_any_element(),
            );
        }
        if let Some(log) = &run.log {
            actions.push(self.open_button("open-workbench-log", "Open log", log, cx));
        }

        Some(
            v_flex()
                .gap(GAP_SM)
                .child(Label::new(last_run_summary(run)).text_xs().text_color(tone))
                .when(!actions.is_empty(), |this| {
                    this.child(h_flex().gap(GAP_SM).flex_wrap().children(actions))
                })
                .into_any_element(),
        )
    }

    fn open_button(
        &self,
        id: &'static str,
        label: &'static str,
        path: &Path,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let path = path.to_path_buf();
        ghost_button(id)
            .label(label)
            .tooltip(path.display().to_string())
            .on_click(cx.listener(move |this, _, window, cx| {
                if let Err(e) = open_file(&path) {
                    this.append_log(
                        &format!("[ERROR] Couldn't open {}: {e}", path.display()),
                        window,
                        cx,
                    );
                }
            }))
            .into_any_element()
    }

    pub(super) fn reveal(&self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(e) = reveal_in_folder(path) {
            self.append_log(
                &format!("[ERROR] Couldn't show {}: {e}", path.display()),
                window,
                cx,
            );
        }
    }

    /// Roll back: run the rollback config Workbench wrote (a `delete` task), after asking.
    fn ask_rollback(
        &mut self,
        config: PathBuf,
        server: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The rollback deletes node IDs from the server that created them. Selecting another
        // server clears the last run, so this is a second line of defence, not the first.
        let selected = self
            .server_select
            .read(cx)
            .selected_value()
            .map(|s| s.to_string());
        if selected.as_deref() != Some(server.as_str()) {
            self.append_log(
                &format!("[ERROR] That run was against {server}; select it again to roll back."),
                window,
                cx,
            );
            return;
        }
        let this = cx.entity().downgrade();
        let name = config
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        ui::confirm_dialog(
            window,
            cx,
            "Roll back this run?",
            format!(
                "Workbench deletes every node, and its media, listed by {name}. Nodes created \
                 by other runs are not touched."
            ),
            "Roll back",
            move |window, cx| {
                let config = config.clone();
                let target = RunTarget {
                    config: Some(config.clone()),
                    recovery_from: None,
                };
                this.update(cx, |this, cx| {
                    this.start_workbench(WorkbenchAction::Run, target, window, cx)
                })
                .ok();
            },
        );
    }

    /// Recovery mode: rerun the same task from the first node it created.
    fn ask_resume(
        &mut self,
        node: u64,
        config: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let this = cx.entity().downgrade();
        ui::confirm_dialog(
            window,
            cx,
            format!("Resume from node {node}?"),
            "Workbench reruns the task and skips every CSV row it already created a node for, \
             using its CSV ID to node ID map. Check the last node before the interruption \
             afterwards; it can occasionally be created twice.",
            "Resume",
            move |window, cx| {
                // The config that ran, not whatever is selected now.
                let target = RunTarget {
                    config: Some(config.clone()),
                    recovery_from: Some(node),
                };
                this.update(cx, |this, cx| {
                    this.start_workbench(WorkbenchAction::Run, target, window, cx)
                })
                .ok();
            },
        );
    }

    /// Quick delete: one node (with its media) or one media, by URL.
    fn ask_quick_delete(&mut self, media: bool, window: &mut Window, cx: &mut Context<Self>) {
        let input = self.quick_delete_url.clone();
        let this = cx.entity().downgrade();
        let (title, hint): (SharedString, SharedString) = if media {
            (
                "Delete one media".into(),
                "The media's URL, e.g. https://example.org/media/552/edit. Its files are deleted \
                 too."
                    .into(),
            )
        } else {
            (
                "Delete one node".into(),
                "The node's URL or alias, e.g. https://example.org/node/393. Its media and files \
                 are deleted too, unless the config says delete_media_with_nodes: false."
                    .into(),
            )
        };
        let focus = input.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            let input_for_delete = input.clone();
            let this = this.clone();
            dialog
                .title(title.clone())
                .w(ui::tokens::CONFIRM_DIALOG_W)
                .child(
                    v_flex()
                        .gap(GAP_SM)
                        .child(Label::new(hint.clone()).text_sm())
                        .child(Input::new(&input).with_size(APP_CONTROL_SIZE)),
                )
                .footer(
                    h_flex()
                        .gap(GAP_SM)
                        .justify_end()
                        .child(
                            Button::new("quick-delete-cancel")
                                .ghost()
                                .label("Cancel")
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new("quick-delete-confirm")
                                .danger()
                                .label("Delete")
                                .on_click(move |_, window, cx| {
                                    let url = input_for_delete.read(cx).value().trim().to_string();
                                    if url.is_empty() {
                                        return;
                                    }
                                    window.close_dialog(cx);
                                    let action = if media {
                                        WorkbenchAction::QuickDeleteMedia(url)
                                    } else {
                                        WorkbenchAction::QuickDeleteNode(url)
                                    };
                                    this.update(cx, |this, cx| {
                                        this.start_workbench(
                                            action,
                                            RunTarget::default(),
                                            window,
                                            cx,
                                        )
                                    })
                                    .ok();
                                }),
                        ),
                )
        });
        // Once, after the dialog has taken focus for itself.
        window.defer(cx, move |window, cx| {
            focus.update(cx, |state, cx| state.focus(window, cx));
        });
    }
}

/// "Check passed", "Run stopped after 2 nodes — cancelled", … in words, not exit codes.
pub(super) fn last_run_summary(run: &LastRun) -> String {
    let nodes = match run.report.nodes_created {
        0 => String::new(),
        1 => " · 1 node created".to_string(),
        n => format!(" · {n} nodes created"),
    };
    let what = match &run.action {
        WorkbenchAction::Check { .. } => "Check",
        WorkbenchAction::Run => "Run",
        WorkbenchAction::CsvTemplate => "CSV template",
        WorkbenchAction::PrintConfig => "Print config",
        WorkbenchAction::QuickDeleteNode(_) | WorkbenchAction::QuickDeleteMedia(_) => {
            "Quick delete"
        }
    };
    let outcome = if run.cancelled {
        "cancelled"
    } else if run.succeeded {
        match run.action {
            WorkbenchAction::Check { .. } => "passed",
            _ => "finished",
        }
    } else {
        "failed — see the log"
    };
    format!("{what} {outcome}{nodes}")
}

#[cfg(test)]
mod tests {
    // Not `use super::*`: that glob-imports gpui, whose `test` macro shadows the built-in one.
    use super::{LastRun, WorkbenchAction, last_run_summary};
    use std::path::PathBuf;
    use workbench_integration::outcome::RunReport;

    fn run(action: WorkbenchAction, succeeded: bool, cancelled: bool, nodes: usize) -> LastRun {
        LastRun {
            action,
            succeeded,
            cancelled,
            report: RunReport {
                nodes_created: nodes,
                ..Default::default()
            },
            task: None,
            server_url: String::new(),
            config_path: PathBuf::new(),
            recovery_from: None,
            log: None,
            rollback_config: None,
            output_csv: None,
        }
    }

    #[test]
    fn the_summary_says_what_happened_in_words() {
        assert_eq!(
            last_run_summary(&run(
                WorkbenchAction::Check {
                    contact_sheet: false
                },
                true,
                false,
                0
            )),
            "Check passed"
        );
        assert_eq!(
            last_run_summary(&run(WorkbenchAction::Run, false, true, 2)),
            "Run cancelled · 2 nodes created"
        );
        assert_eq!(
            last_run_summary(&run(WorkbenchAction::Run, false, false, 1)),
            "Run failed — see the log · 1 node created"
        );
    }
}
