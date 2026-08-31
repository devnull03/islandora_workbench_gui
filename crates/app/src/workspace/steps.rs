//! The main window's three numbered steps and the log pane (mockup `2c`).
//!
//! Pure rendering: every method reads `self` and returns an element, so `mod.rs` is left holding
//! only state, wiring and the top-level layout.

use std::path::PathBuf;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme, Disableable, IconName, Sizable, button::ButtonVariants, checkbox::Checkbox,
    h_flex, input::Input, label::Label, select::Select, v_flex,
};
use settings::AppSettings;
use ui::{APP_CONTROL_SIZE, LabeledField, StepSection, app_button};

use super::sources::SOURCE_CSV;
use super::{Operation, WorkflowStage, Workspace};
use crate::helpers::{get_file, reveal_in_folder, workbench_input_data_dir};
use config_builder::open_config_builder;

impl Workspace {
    /// The one field that changes with the source: a URL to paste, or a file to browse for.
    /// Both keep their own state, so switching back and forth loses nothing.
    fn render_source_field(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let idle = self.is_idle();
        let is_csv = self.source_key(cx) == SOURCE_CSV;
        let state = self.source_field(cx).clone();

        LabeledField::new(if is_csv { "Source CSV" } else { "Sheet URL" }).child(
            h_flex()
                .w_full()
                .gap_2()
                .child(
                    div().flex_1().min_w(px(0.)).child(
                        Input::new(&state)
                            .with_size(APP_CONTROL_SIZE)
                            .w_full()
                            .disabled(!idle),
                    ),
                )
                .when(is_csv, |row| {
                    row.child(
                        app_button("browse-source-csv")
                            .icon(IconName::FolderOpen)
                            .outline()
                            .disabled(!idle)
                            .on_click(cx.listener(|this, _, window, cx| {
                                get_file(
                                    window,
                                    cx,
                                    &this.source_csv,
                                    "Select the source CSV".into(),
                                    false,
                                );
                            })),
                    )
                }),
        )
    }

    /// Step 1 — where the metadata CSV comes from. Optional: a config that already points at a
    /// prepared CSV can skip straight to step 2.
    pub(super) fn render_input_source(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let idle = self.is_idle();
        let loading = self.op == Operation::Preprocessing;
        let process_disabled = !idle || !self.process_ready(cx);
        let processed = idle && self.stage >= WorkflowStage::SourceProcessed;
        let border = cx.theme().colors.border;
        let secondary = cx.theme().colors.secondary;
        let muted = cx.theme().colors.muted_foreground;

        StepSection::new("1", "Input source")
            .note("optional step")
            .child(
                Select::new(&self.input_source_select)
                    .with_size(APP_CONTROL_SIZE)
                    .w_full()
                    .disabled(!idle),
            )
            .child(
                v_flex()
                    .w_full()
                    .gap_2()
                    .p_3()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_color(border)
                    .bg(secondary)
                    .child(self.render_source_field(cx))
                    .child(
                        LabeledField::new("Ingest dir").child(
                            h_flex()
                                .w_full()
                                .gap_2()
                                .child(
                                    div().flex_1().min_w(px(0.)).child(
                                        Input::new(&self.ingest_files_dir)
                                            .with_size(APP_CONTROL_SIZE)
                                            .disabled(true)
                                            .w_full(),
                                    ),
                                )
                                .child(
                                    app_button("browse-ingest-dir")
                                        .icon(IconName::FolderOpen)
                                        .outline()
                                        .disabled(!idle)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            get_file(
                                                window,
                                                cx,
                                                &this.ingest_files_dir,
                                                "Select directory for ingest files".into(),
                                                true,
                                            );
                                        })),
                                ),
                        ),
                    )
                    .child(
                        LabeledField::new("Preprocess script")
                            .description(
                                "Receives the whole CSV and optional config; \
                                 outputs the new metadata CSV path.",
                            )
                            .child(
                                Select::new(&self.processor_select)
                                    .with_size(APP_CONTROL_SIZE)
                                    .w_full()
                                    .disabled(!idle),
                            ),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .gap_2()
                            .child(
                                app_button("process-gdrive")
                                    .outline()
                                    .label("Process")
                                    .loading(loading)
                                    .disabled(process_disabled || loading)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.process_metadata(window, cx);
                                    })),
                            )
                            .child(
                                app_button("open-gdrive-output")
                                    .outline()
                                    .label("Open processed")
                                    .disabled(!processed)
                                    .on_click(cx.listener(|_, _, window, cx| {
                                        let Some(dir) = workbench_input_data_dir(cx) else {
                                            return;
                                        };
                                        let metadata_csv = dir.join("metadata.csv");
                                        if !metadata_csv.is_file() {
                                            return;
                                        }
                                        let entity = cx.entity().clone();
                                        cx.spawn_in(window, async move |_, cx| {
                                            if let Err(e) = reveal_in_folder(&metadata_csv) {
                                                let msg =
                                                    format!("[ERROR] Failed to reveal output: {e}");
                                                cx.update(|window, cx| {
                                                    entity.update(cx, |this, cx| {
                                                        this.append_log(&msg, window, cx);
                                                    });
                                                })
                                                .ok();
                                            }
                                        })
                                        .detach();
                                    })),
                            )
                            .child(div().flex_1())
                            .child(
                                Label::new(if processed {
                                    "last run this session"
                                } else {
                                    "last run —"
                                })
                                .text_xs()
                                .text_color(muted),
                            ),
                    ),
            )
    }

    /// Step 2 — which saved Workbench config to run, with the way into the config builder.
    pub(super) fn render_config(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let idle = self.is_idle();
        let selected = self.saved_config_select.read(cx).selected_value().is_some();
        let muted = cx.theme().colors.muted_foreground;

        StepSection::new("2", "Config")
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .child(
                        div().flex_1().min_w(px(0.)).child(
                            Select::new(&self.saved_config_select)
                                .placeholder("Select saved config…")
                                .with_size(APP_CONTROL_SIZE)
                                .disabled(!idle)
                                .w_full(),
                        ),
                    )
                    .child(
                        app_button("edit-config")
                            .outline()
                            .label("Edit")
                            .disabled(!selected)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let Some(path) = this.saved_config_select.read(cx).selected_value()
                                else {
                                    return;
                                };
                                open_config_builder(Some(PathBuf::from(path.as_ref())), cx);
                            })),
                    )
                    .child(
                        app_button("new-config")
                            .outline()
                            .label("New")
                            .on_click(|_, _, cx| open_config_builder(None, cx)),
                    ),
            )
            .child(
                Label::new(self.config_summary.clone().unwrap_or_else(|| {
                    "Workbench YAML · edit or create with the config builder".into()
                }))
                .text_xs()
                .text_color(muted),
            )
    }

    /// Step 3 — target server, plus the two actions that actually talk to it.
    pub(super) fn render_server(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let idle = self.is_idle();
        let check_loading = self.op == Operation::CheckRunning;
        let run_loading = self.op == Operation::IngestRunning;
        let actions_disabled = !idle || !self.ingest_ready(cx);
        let run_enabled = idle && self.stage == WorkflowStage::CheckPassed;
        let auto_accept = AppSettings::get(cx)
            .values
            .get("auto_accept_prompts")
            .map(|v| v.bool())
            .unwrap_or(false);

        StepSection::new("3", "Server")
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .child(
                        div().flex_1().min_w(px(0.)).child(
                            Select::new(&self.server_select)
                                .placeholder("Select server…")
                                .with_size(APP_CONTROL_SIZE)
                                .disabled(!idle)
                                .w_full(),
                        ),
                    )
                    .child(
                        app_button("manage-servers")
                            .outline()
                            .label("Manage")
                            .on_click(|_, _, cx| {
                                cx.dispatch_action(&crate::app_menus::OpenSettings);
                            }),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .child(
                        Checkbox::new("auto-accept-prompts")
                            .small()
                            .checked(auto_accept)
                            .label("Auto-accept prompts")
                            .disabled(!idle)
                            .on_click(cx.listener(|_, checked: &bool, _, cx| {
                                AppSettings::set_bool("auto_accept_prompts", *checked, cx);
                                cx.notify();
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        app_button("check")
                            .outline()
                            .label("Check")
                            .loading(check_loading)
                            .disabled(actions_disabled || check_loading || run_loading)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.run_ingest(true, window, cx);
                            })),
                    )
                    .child(
                        app_button("run-ingest")
                            .primary()
                            .label("Run Ingest")
                            .loading(run_loading)
                            .disabled(!run_enabled || run_loading)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.run_ingest(false, window, cx);
                            })),
                    ),
            )
    }
}
