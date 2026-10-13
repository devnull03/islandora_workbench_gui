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
            MapAction::ExportCsv(path) => Some(path.clone()),
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
                "Every entry whose node no longer exists on {host} is deleted from the CSV ID \
                 map. This asks the server about each node, so it can take a while."
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

    /// Remove the entries older than a date the user types.
    pub(super) fn ask_map_remove_before(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("2025-01-31"));
        let this = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, window, cx| {
            input.update(cx, |state, cx| state.focus(window, cx));
            let (input_for_remove, this) = (input.clone(), this.clone());
            dialog
                .title("Remove map entries before a date")
                .w(ui::tokens::CONFIRM_DIALOG_W)
                .child(
                    v_flex()
                        .gap(GAP_SM)
                        .child(
                            Label::new(
                                "Entries Workbench recorded before this date (yyyy-mm-dd, \
                                 optionally hh:mm) are deleted from the CSV ID map.",
                            )
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
                                        this.run_map_tool(MapAction::RemoveBefore(date), window, cx)
                                    })
                                    .ok();
                                }),
                        ),
                )
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
