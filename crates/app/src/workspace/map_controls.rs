//! The More menu's CSV ID → node ID map tools (docs: "The CSV ID to node ID map"): Workbench's
//! own `manage_csv_to_node_id_map.py`, run against the map the selected config uses, with its
//! output in the log.

use std::path::{Path, PathBuf};

use gpui::*;
use gpui_component::{
    Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
    label::Label,
    v_flex,
};
use ui::{APP_CONTROL_SIZE, tokens::GAP_SM};
use window_wrapper::WindowLock;
use workbench_integration::{
    config::ConfigDraft,
    map_tool::{MapAction, map_db_path, map_tool_command},
    spawn_command_streaming,
};

use super::streaming::spawn_stream_to_log;
use super::{Operation, Workspace};

impl Workspace {
    /// Where the selected config's map is, or why it can't be found.
    fn map_db(&self, cx: &App) -> Result<PathBuf, String> {
        let info = super::run::workbench_info(cx).map_err(|e| e.to_string())?;
        let draft = self
            .saved_config_select
            .read(cx)
            .selected_value()
            .and_then(|path| ConfigDraft::load(Path::new(path.as_ref())).ok());
        let text = |key: &str| {
            draft
                .as_ref()?
                .values
                .get(key)?
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        };
        Ok(map_db_path(text, &info.install_path))
    }

    /// Run the map tool, its output streamed to the log. It holds the window lock like a run.
    pub(super) fn run_map_tool(
        &mut self,
        action: MapAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The save dialog is not modal everywhere, so a run may have started while it was open.
        if !self.is_idle() {
            return self.append_log(
                "[ERROR] Wait for the current run to finish before changing the CSV ID map.",
                window,
                cx,
            );
        }
        let (info, db) = match (super::run::workbench_info(cx), self.map_db(cx)) {
            (Ok(info), Ok(db)) => (info, db),
            (Err(e), _) => return self.append_log(&format!("[ERROR] {e}"), window, cx),
            (_, Err(e)) => return self.append_log(&format!("[ERROR] {e}"), window, cx),
        };
        if !db.is_file() {
            let message = format!(
                "[ERROR] There is no CSV ID map at {} yet — Workbench makes it on the first create.",
                db.display()
            );
            return self.append_log(&message, window, cx);
        }
        let process = match spawn_command_streaming(map_tool_command(&info, &db, &action)) {
            Ok(process) => process,
            Err(e) => {
                let message = format!("[ERROR] Couldn't start the map tool: {e}");
                return self.append_log(&message, window, cx);
            }
        };
        self.op = Operation::Workbench;
        WindowLock::set(true, cx);
        self.append_log(
            &format!("--- {} ({}) ---", action.label(), db.display()),
            window,
            cx,
        );
        let export = match &action {
            MapAction::ExportCsv(path) | MapAction::ExportDuplicates { csv: path, .. } => {
                Some(path.clone())
            }
            _ => None,
        };
        let workspace = cx.entity();
        spawn_stream_to_log(
            workspace,
            process.lines,
            process.stdin,
            false,
            window,
            cx,
            move |this, succeeded, cx| {
                this.op = Operation::None;
                WindowLock::set(false, cx);
                if succeeded && let Some(path) = export.filter(|p| p.is_file()) {
                    let _ = crate::helpers::open_file(&path);
                }
                cx.notify();
            },
        );
        cx.notify();
    }

    /// Export: pick where the CSV goes, then dump the map there and open it.
    pub(super) fn ask_map_export(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let directory = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let receiver = cx.prompt_for_new_path(&directory, Some("csv_id_to_node_id_map.csv"));
        cx.spawn_in(_window, async move |this, cx| {
            let Ok(Ok(Some(path))) = receiver.await else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                this.run_map_tool(MapAction::ExportCsv(path), window, cx)
            })
            .ok();
        })
        .detach();
    }

    /// Remove the entries for nodes the selected server no longer has.
    pub(super) fn ask_map_remove_deleted(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = self.server_select.read(cx).selected_value().cloned() else {
            return self.append_log("[ERROR] Choose a server first.", window, cx);
        };
        let this = cx.entity().downgrade();
        ui::confirm_dialog(
            window,
            cx,
            "Remove map entries for deleted nodes?",
            format!(
                "Workbench asks {host} about every node ID in the CSV ID map and deletes the \
                 entries for nodes it no longer has. The map is not split by server: if it also \
                 holds entries from another server, ones with the same node ID are deleted too. \
                 This can take a while."
            ),
            "Remove",
            move |window, cx| {
                let host = host.to_string();
                this.update(cx, |this, cx| {
                    this.run_map_tool(MapAction::RemoveDeletedNodes(host), window, cx)
                })
                .ok();
            },
        );
    }

    /// Remove the entries the selected config made — Workbench records the config path it was
    /// given, which for this app is the library file.
    pub(super) fn ask_map_remove_for_config(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(config) = self.saved_config_select.read(cx).selected_value().cloned() else {
            return self.append_log("[ERROR] Choose a config first.", window, cx);
        };
        // The script takes a comma-separated list, so a comma in the path would split it.
        if config.contains(',') {
            return self.append_log(
                "[ERROR] The config's path contains a comma, which the map tool cannot take.",
                window,
                cx,
            );
        }
        let this = cx.entity().downgrade();
        ui::confirm_dialog(
            window,
            cx,
            "Remove this config's map entries?",
            format!(
                "CSV ID map entries Workbench recorded for {config} — run from this app — are \
                 deleted; entries its secondary configs made are kept. Later runs can no longer \
                 find those nodes as parents."
            ),
            "Remove",
            move |window, cx| {
                let config = config.to_string();
                this.update(cx, |this, cx| {
                    this.run_map_tool(MapAction::RemoveForConfigs(config), window, cx)
                })
                .ok();
            },
        );
    }

    /// Export only the rows whose CSV ID appears more than once — the duplicates the docs warn
    /// make parent lookups ambiguous.
    pub(super) fn ask_map_export_duplicates(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let directory = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let receiver =
            cx.prompt_for_new_path(&directory, Some("csv_id_to_node_id_map_duplicates.csv"));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(csv))) = receiver.await else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                let action = MapAction::ExportDuplicates {
                    csv,
                    column: "csv_id".into(),
                };
                this.run_map_tool(action, window, cx)
            })
            .ok();
        })
        .detach();
    }

    /// Merge another map database (from another machine or folder) into this one.
    pub(super) fn ask_map_merge(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Merge this CSV ID map".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(source) = paths.into_iter().next() else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                this.confirm_merge(source, window, cx)
            })
            .ok();
        })
        .detach();
    }

    /// Merging appends every row of `source` (the table has no key), so merging a map into
    /// itself, or twice, duplicates its entries. Refuse the first, ask before the rest.
    fn confirm_merge(&mut self, source: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let target = match self.map_db(cx) {
            Ok(target) => target,
            Err(e) => return self.append_log(&format!("[ERROR] {e}"), window, cx),
        };
        let same = |a: &Path, b: &Path| match (a.canonicalize(), b.canonicalize()) {
            (Ok(a), Ok(b)) => a == b,
            _ => a == b,
        };
        if same(&source, &target) {
            return self.append_log(
                "[ERROR] That is the map this config already uses; merging it into itself would \
                 duplicate every entry.",
                window,
                cx,
            );
        }
        let this = cx.entity().downgrade();
        ui::confirm_dialog(
            window,
            cx,
            "Merge this map into the current one?",
            format!(
                "Every entry in {} is added to {}. Entries already there are not checked, so \
                 merging the same map twice adds them twice.",
                source.display(),
                target.display()
            ),
            "Merge",
            move |window, cx| {
                let source = source.clone();
                this.update(cx, |this, cx| {
                    this.run_map_tool(MapAction::MergeFrom(source), window, cx)
                })
                .ok();
            },
        );
    }

    /// Remove the entries recorded before (or after) a date the user types.
    pub(super) fn ask_map_remove_by_date(
        &mut self,
        after: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("2025-01-31"));
        let this = cx.entity().downgrade();
        let focus = input.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            let (input_for_remove, this) = (input.clone(), this.clone());
            dialog
                .title(if after {
                    "Remove map entries after a date"
                } else {
                    "Remove map entries before a date"
                })
                .w(ui::tokens::CONFIRM_DIALOG_W)
                .child(
                    v_flex()
                        .gap(GAP_SM)
                        .child(
                            Label::new(format!(
                                "Entries Workbench recorded {} this date (yyyy-mm-dd, optionally \
                                 hh:mm) are deleted from the CSV ID map.",
                                if after { "after" } else { "before" }
                            ))
                            .text_sm(),
                        )
                        .child(Input::new(&input).with_size(APP_CONTROL_SIZE)),
                )
                .footer(
                    h_flex()
                        .gap(GAP_SM)
                        .justify_end()
                        .child(
                            Button::new("map-remove-cancel")
                                .ghost()
                                .label("Cancel")
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new("map-remove-confirm")
                                .danger()
                                .label("Remove")
                                .on_click(move |_, window, cx| {
                                    let date = input_for_remove.read(cx).value().trim().to_string();
                                    if !looks_like_a_date(&date) {
                                        return;
                                    }
                                    window.close_dialog(cx);
                                    this.update(cx, |this, cx| {
                                        let action = if after {
                                            MapAction::RemoveAfter(date)
                                        } else {
                                            MapAction::RemoveBefore(date)
                                        };
                                        this.run_map_tool(action, window, cx)
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

/// `yyyy-mm-dd`, optionally followed by a time — what the script's `datetime` parsing takes.
fn looks_like_a_date(text: &str) -> bool {
    let date = text.split_whitespace().next().unwrap_or_default();
    let parts: Vec<&str> = date.split('-').collect();
    parts.len() == 3
        && parts[0].len() == 4
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::looks_like_a_date;

    #[test]
    fn dates_are_checked_before_anything_is_deleted() {
        assert!(looks_like_a_date("2025-01-31"));
        assert!(looks_like_a_date("2025-01-31 12:30"));
        assert!(!looks_like_a_date("31/01/2025"));
        assert!(!looks_like_a_date(""));
    }
}
