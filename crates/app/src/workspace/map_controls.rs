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
    radio::RadioGroup,
    v_flex,
};
use ui::{APP_CONTROL_SIZE, tokens::GAP_SM};
use window_wrapper::WindowLock;
use workbench_integration::{
    config::ConfigDraft,
    config::chain::recorded_config_files,
    map_tool::{MapAction, map_db_path, map_tool_command},
    spawn_command_streaming,
};

use super::streaming::spawn_stream_to_log;
use super::{Operation, Workspace};

/// The map a config uses and the names its runs are recorded under, found off the UI thread.
struct MapSite {
    db: PathBuf,
    exists: bool,
    /// The config and its secondary configs, as Workbench records them.
    recorded: Vec<String>,
}

impl MapSite {
    /// Load the selected config (if any) and work out where its map is.
    fn locate(config: Option<PathBuf>, workbench_dir: PathBuf) -> MapSite {
        let draft = config
            .as_deref()
            .and_then(|path| ConfigDraft::load(path).ok());
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
        let db = map_db_path(text, &workbench_dir);
        // A config that no longer loads still has its own entries, under its path; only its
        // secondary configs need it read.
        let recorded = match (&draft, &config) {
            (Some(draft), Some(path)) => {
                recorded_config_files(draft, &path.display().to_string(), Some(&workbench_dir))
            }
            (None, Some(path)) => vec![path.display().to_string()],
            _ => Vec::new(),
        };
        MapSite {
            exists: db.is_file(),
            db,
            recorded,
        }
    }
}

impl Workspace {
    /// Find the selected config's map off the UI thread, then carry on with `then`. The
    /// workspace stays busy meanwhile, so a run cannot start underneath.
    fn with_map_site(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, MapSite, &mut Window, &mut Context<Self>) + 'static,
    ) {
        // The save dialog is not modal everywhere, so a run may have started while it was open.
        if !self.is_idle() {
            return self.append_log(
                "[ERROR] Wait for the current run to finish before changing the CSV ID map.",
                window,
                cx,
            );
        }
        let workbench_dir = match super::run::workbench_info(cx) {
            Ok(info) => info.install_path,
            Err(e) => return self.append_log(&format!("[ERROR] {e}"), window, cx),
        };
        let config = self
            .saved_config_select
            .read(cx)
            .selected_value()
            .map(|path| PathBuf::from(path.as_ref()));
        self.op = Operation::Preparing;
        WindowLock::set_preparing(true, cx);
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let site = cx
                .background_spawn(async move { MapSite::locate(config, workbench_dir) })
                .await;
            this.update_in(cx, |this, window, cx| {
                this.op = Operation::None;
                WindowLock::set_preparing(false, cx);
                cx.notify();
                then(this, site, window, cx)
            })
            .ok();
        })
        .detach();
    }

    /// Run the map tool, its output streamed to the log. It holds the window lock like a run.
    pub(super) fn run_map_tool(
        &mut self,
        action: MapAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_map_site(window, cx, move |this, site, window, cx| {
            this.start_map_tool(action, site, window, cx)
        });
    }

    fn start_map_tool(
        &mut self,
        action: MapAction,
        site: MapSite,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let info = match super::run::workbench_info(cx) {
            Ok(info) => info,
            Err(e) => return self.append_log(&format!("[ERROR] {e}"), window, cx),
        };
        let db = site.db;
        if !site.exists {
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

    /// Remove the entries the selected config's runs made — Workbench records the config path
    /// it was given (for this app, the library file) and each secondary config as its
    /// `secondary_tasks` entry is written.
    pub(super) fn ask_map_remove_for_config(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.saved_config_select.read(cx).selected_value().is_none() {
            return self.append_log("[ERROR] Choose a config first.", window, cx);
        }
        self.with_map_site(window, cx, |this, site, window, cx| {
            // The script takes a comma-separated list, so a comma in a name would split it.
            if let Some(name) = site.recorded.iter().find(|name| name.contains(',')) {
                let message =
                    format!("[ERROR] {name} contains a comma, which the map tool cannot take.");
                return this.append_log(&message, window, cx);
            }
            if site.recorded.is_empty() {
                return this.append_log("[ERROR] Couldn't read the selected config.", window, cx);
            }
            let list = site.recorded.join(",");
            let secondaries = &site.recorded[1..];
            let body = if secondaries.is_empty() {
                format!(
                    "CSV ID map entries Workbench recorded for {} — run from this app — are \
                     deleted. Later runs can no longer find those nodes as parents.",
                    site.recorded[0]
                )
            } else {
                format!(
                    "CSV ID map entries Workbench recorded for {} — run from this app — and \
                     for its secondary configs ({}) are deleted. Later runs can no longer find \
                     those nodes as parents. A secondary config is recorded by its entry as \
                     written, so another chain's entry spelled the same way loses its entries \
                     too.",
                    site.recorded[0],
                    secondaries.join(", ")
                )
            };
            let entity = cx.entity().downgrade();
            ui::confirm_dialog(
                window,
                cx,
                "Remove this config's map entries?",
                body,
                "Remove",
                move |window, cx| {
                    let list = list.clone();
                    entity
                        .update(cx, |this, cx| {
                            this.run_map_tool(MapAction::RemoveForConfigs(list), window, cx)
                        })
                        .ok();
                },
            );
        });
    }

    /// Export only the rows whose value in a chosen column appears more than once — duplicate
    /// CSV IDs make parent lookups ambiguous; a duplicate node ID means one node was recorded
    /// for two rows.
    pub(super) fn ask_map_export_duplicates(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let chosen = cx.new(|_| 0usize);
        let this = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let selected = *chosen.read(cx);
            let (chosen_for_click, chosen_for_export, this) =
                (chosen.clone(), chosen.clone(), this.clone());
            dialog
                .title("Export duplicate IDs from the CSV ID map")
                .w(ui::tokens::CONFIRM_DIALOG_W)
                .child(
                    v_flex()
                        .gap(GAP_SM)
                        .child(
                            Label::new(
                                "Rows whose value in this column appears more than once are \
                                 saved to a CSV.",
                            )
                            .text_sm(),
                        )
                        .child(
                            RadioGroup::vertical("map-duplicate-column")
                                .children(DUPLICATE_COLUMNS.iter().map(|(_, label)| *label))
                                .selected_index(Some(selected))
                                .on_click(move |index, _, cx| {
                                    chosen_for_click.update(cx, |chosen, cx| {
                                        *chosen = *index;
                                        cx.notify();
                                    });
                                }),
                        ),
                )
                .footer(
                    h_flex()
                        .gap(GAP_SM)
                        .justify_end()
                        .child(
                            Button::new("map-duplicates-cancel")
                                .ghost()
                                .label("Cancel")
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new("map-duplicates-export")
                                .primary()
                                .label("Choose file…")
                                .on_click(move |_, window, cx| {
                                    let column = DUPLICATE_COLUMNS[*chosen_for_export.read(cx)].0;
                                    window.close_dialog(cx);
                                    this.update(cx, |this, cx| {
                                        this.export_duplicates(column, window, cx)
                                    })
                                    .ok();
                                }),
                        ),
                )
        });
    }

    fn export_duplicates(&mut self, column: &str, window: &mut Window, cx: &mut Context<Self>) {
        let directory = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let name = format!("csv_id_to_node_id_map_duplicate_{column}.csv");
        let receiver = cx.prompt_for_new_path(&directory, Some(&name));
        let column = column.to_string();
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(csv))) = receiver.await else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                this.run_map_tool(MapAction::ExportDuplicates { csv, column }, window, cx)
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
        self.with_map_site(window, cx, move |this, site, window, cx| {
            let target = site.db;
            let same = |a: &Path, b: &Path| match (a.canonicalize(), b.canonicalize()) {
                (Ok(a), Ok(b)) => a == b,
                _ => a == b,
            };
            if same(&source, &target) {
                return this.append_log(
                    "[ERROR] That is the map this config already uses; merging it into itself \
                     would duplicate every entry.",
                    window,
                    cx,
                );
            }
            let entity = cx.entity().downgrade();
            ui::confirm_dialog(
                window,
                cx,
                "Merge this map into the current one?",
                format!(
                    "Every entry in {} is added to {}. Entries already there are not checked, \
                     so merging the same map twice adds them twice.",
                    source.display(),
                    target.display()
                ),
                "Merge",
                move |window, cx| {
                    let source = source.clone();
                    entity
                        .update(cx, |this, cx| {
                            this.run_map_tool(MapAction::MergeFrom(source), window, cx)
                        })
                        .ok();
                },
            );
        });
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

/// The map's columns a duplicate means something in (`manage_csv_to_node_id_map.py
/// --nonunique`), with how the dialog names them. The rest repeat by design: every top-level
/// row has empty parent columns, siblings share a parent, and `timestamp`, `config_file` and
/// `host` are shared by a whole run.
const DUPLICATE_COLUMNS: [(&str, &str); 2] = [("csv_id", "CSV ID"), ("node_id", "Node ID")];

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
