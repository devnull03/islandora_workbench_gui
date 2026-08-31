//! The server list and the config library: lists whose rows expand in place to edit, with
//! `+ Add` as the last row (mockup `3b`).
//!
//! **A list and its add form are one thing.** The old shape had a read-only list in one group
//! and a separate "Add New …" group writing `new_server_*` staging keys into `AppSettings`, so
//! each page read as two unrelated forms and a typo meant delete-and-retype. Here the add row is
//! the edit row with no index behind it, so one renderer covers both and there are no staging
//! keys to leak.
//!
//! Per-row widget state is `window.use_keyed_state`, keyed by the row's identity. A settings
//! item's render closure gets `&mut Window, &mut App` and is called fresh every frame, so it can
//! create entities but cannot hold them — keyed state is how gpui-component's own path picker
//! solves this, and it means no global edit state to keep in sync.

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme, IconName, Sizable,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputState},
    label::Label,
    select::{Select, SelectState},
    setting::SettingItem,
    v_flex,
};

use std::path::PathBuf;

use settings::{AppSettings, CheckResult, ServerConfig, TaskConfig};
use workbench_integration::check_server;

use config_builder::open_config_builder;
use ui::DetailSelectItem;

pub const TASK_OPTIONS: &[(&str, &str)] = &[
    ("create", "create - Create new nodes"),
    ("create_from_files", "create_from_files - Create from files"),
    ("update", "update - Update existing nodes"),
    ("delete", "delete - Delete nodes"),
    ("add_media", "add_media - Add media to nodes"),
    ("update_media", "update_media - Update existing media"),
    ("delete_media", "delete_media - Delete media"),
    (
        "delete_media_by_node",
        "delete_media_by_node - Delete media by node",
    ),
    ("export_csv", "export_csv - Export content to CSV"),
    (
        "get_data_from_view",
        "get_data_from_view - Get data from view",
    ),
];

/// Which row is expanded, if any. `None` in the map means the add row.
#[derive(Default)]
struct OpenRow(Option<Option<usize>>);

impl OpenRow {
    fn is(&self, index: Option<usize>) -> bool {
        self.0 == Some(index)
    }
    fn toggle(&mut self, index: Option<usize>) {
        self.0 = if self.is(index) { None } else { Some(index) };
    }
}

/// One text field of an expanded row, seeded once from the stored value.
fn row_input(
    key: impl Into<SharedString>,
    seed: &str,
    placeholder: &'static str,
    window: &mut Window,
    cx: &mut App,
) -> Entity<InputState> {
    let seed = seed.to_string();
    window.use_keyed_state(key.into(), cx, move |window, cx| {
        InputState::new(window, cx)
            .placeholder(placeholder)
            .default_value(seed)
    })
}

fn value_of(input: &Entity<InputState>, cx: &App) -> SharedString {
    input.read(cx).value().trim().to_string().into()
}

fn row_shell() -> ui::Card {
    ui::Card::new().tone(ui::CardTone::Filled)
}

// --- Config library ---------------------------------------------------------------------

fn config_row(
    index: Option<usize>,
    config: &TaskConfig,
    open: &Entity<OpenRow>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let id = index.map_or_else(|| "new".to_string(), |i| i.to_string());
    let expanded = open.read(cx).is(index);

    if !expanded {
        let toggle = open.clone();
        if index.is_none() {
            return Button::new("add-config")
                .ghost()
                .w_full()
                .label("+ Add configuration")
                .on_click(move |_, _, cx| {
                    toggle.update(cx, |state, cx| {
                        state.toggle(None);
                        cx.notify();
                    })
                })
                .into_any_element();
        }
        let idx = index.unwrap_or(0);
        let path = PathBuf::from(config.file_path.as_ref());
        return row_shell()
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .items_center()
                    .child(
                        ui::SummaryLines::new(config.label.clone())
                            .line(format!("Task: {}", config.task_name))
                            .line(config.file_path.clone()),
                    )
                    .child(
                        Button::new(SharedString::from(format!("edit-{id}")))
                            .icon(IconName::Settings2)
                            .ghost()
                            .with_size(ui::APP_CONTROL_SIZE)
                            .tooltip("Open in the config builder")
                            .on_click(move |_, _, cx| open_config_builder(Some(path.clone()), cx)),
                    )
                    .child(
                        Button::new(SharedString::from(format!("rename-{id}")))
                            .label("Edit")
                            .ghost()
                            .with_size(ui::APP_CONTROL_SIZE)
                            .on_click(move |_, _, cx| {
                                toggle.update(cx, |state, cx| {
                                    state.toggle(Some(idx));
                                    cx.notify();
                                })
                            }),
                    )
                    .child(
                        Button::new(SharedString::from(format!("remove-{id}")))
                            .icon(IconName::Close)
                            .ghost()
                            .with_size(ui::APP_CONTROL_SIZE)
                            .tooltip("Remove from the library")
                            .on_click(move |_, _, cx| AppSettings::remove_task_config(idx, cx)),
                    ),
            )
            .into_any_element();
    }

    let label = row_input(
        format!("cfg-label-{id}"),
        &config.label,
        "Label",
        window,
        cx,
    );
    // A dropdown, not a text field: `task` is the one setting Workbench demands and only these
    // ten values are legal, so there is no reason to let someone mistype one.
    let task = {
        let seed = config.task_name.clone();
        window.use_keyed_state(
            SharedString::from(format!("cfg-task-{id}")),
            cx,
            move |window, cx| {
                let items: Vec<DetailSelectItem> = TASK_OPTIONS
                    .iter()
                    .enumerate()
                    .map(|(i, (value, label))| DetailSelectItem {
                        label: (*value).into(),
                        subtitle: (*label).into(),
                        value: (*value).into(),
                        divider_above: i > 0,
                    })
                    .collect();
                let mut state = SelectState::new(items, None, window, cx);
                if !seed.is_empty() {
                    state.set_selected_value(&seed, window, cx);
                }
                state
            },
        )
    };
    let path = row_input(
        format!("cfg-path-{id}"),
        &config.file_path,
        "Path to the YAML config",
        window,
        cx,
    );

    let (l, t, p) = (label.clone(), task.clone(), path.clone());
    let close = open.clone();
    let cancel = open.clone();
    let browse = path.clone();

    row_shell()
        .child(Input::new(&label).small().w_full())
        .child(Select::new(&task).placeholder("Task").w_full())
        .child(
            ui::FieldRow::new(Input::new(&path).small().w_full()).child(
                Button::new(SharedString::from(format!("cfg-browse-{id}")))
                    .icon(IconName::FolderOpen)
                    .outline()
                    .with_size(ui::APP_CONTROL_SIZE)
                    .on_click(move |_, window, cx| {
                        ui::pick_into_app(window, cx, browse.clone(), "Select config file", false)
                    }),
            ),
        )
        .child(
            h_flex()
                .w_full()
                .gap_2()
                .justify_end()
                .child(
                    Button::new(SharedString::from(format!("cfg-cancel-{id}")))
                        .ghost()
                        .with_size(ui::APP_CONTROL_SIZE)
                        .label("Cancel")
                        .on_click(move |_, _, cx| {
                            cancel.update(cx, |state, cx| {
                                state.toggle(index);
                                cx.notify();
                            })
                        }),
                )
                .child(
                    Button::new(SharedString::from(format!("cfg-save-{id}")))
                        .primary()
                        .with_size(ui::APP_CONTROL_SIZE)
                        .label(if index.is_some() { "Save" } else { "Add" })
                        .on_click(move |_, _, cx| {
                            AppSettings::upsert_task_config(
                                index,
                                TaskConfig {
                                    label: value_of(&l, cx),
                                    task_name: t
                                        .read(cx)
                                        .selected_value()
                                        .cloned()
                                        .unwrap_or_default(),
                                    file_path: value_of(&p, cx),
                                },
                                cx,
                            );
                            close.update(cx, |state, cx| {
                                state.0 = None;
                                cx.notify();
                            });
                        }),
                ),
        )
        .into_any_element()
}

pub fn saved_configs_field() -> SettingItem {
    SettingItem::render(|_options, window, cx| {
        let open = window.use_keyed_state(SharedString::from("config-open-row"), cx, |_, _| {
            OpenRow::default()
        });
        let configs = AppSettings::get(cx).task_configs.clone();
        let mut rows: Vec<AnyElement> = configs
            .iter()
            .enumerate()
            .map(|(i, c)| config_row(Some(i), c, &open, window, cx))
            .collect();
        rows.push(config_row(None, &TaskConfig::default(), &open, window, cx));
        v_flex().gap_2().w_full().children(rows)
    })
}

// --- Servers ----------------------------------------------------------------------------

/// The `✓ / ! / ✕` line under a server, or nothing when it has never been tested.
fn check_line(config: &ServerConfig, cx: &App) -> Option<impl IntoElement> {
    let check = config.last_check.as_ref()?;
    let color = if check.is_ok() {
        cx.theme().colors.success
    } else if check.reachable {
        cx.theme().colors.warning
    } else {
        cx.theme().colors.danger
    };
    Some(
        Label::new(format!("Last checked {} — {}", check.age(), check.message))
            .text_xs()
            .text_color(color),
    )
}

fn server_row(
    index: Option<usize>,
    config: &ServerConfig,
    open: &Entity<OpenRow>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let id = index.map_or_else(|| "new".to_string(), |i| i.to_string());
    let expanded = open.read(cx).is(index);

    if !expanded {
        let toggle = open.clone();
        if index.is_none() {
            return Button::new("add-server")
                .ghost()
                .w_full()
                .label("+ Add server")
                .on_click(move |_, _, cx| {
                    toggle.update(cx, |state, cx| {
                        state.toggle(None);
                        cx.notify();
                    })
                })
                .into_any_element();
        }
        let idx = index.unwrap_or(0);
        let url = config.server_url.to_string();
        let creds = config.credentials_file.to_string();
        let duplicate = config.clone();
        return row_shell()
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .items_center()
                    .child(
                        ui::SummaryLines::new(config.label.clone())
                            .line(config.server_url.clone())
                            .line(if config.credentials_file.is_empty() {
                                SharedString::from("No credentials file")
                            } else {
                                config.credentials_file.clone()
                            }),
                    )
                    .when(config.needs_confirmation, |row| {
                        row.child(
                            Label::new("confirms")
                                .text_xs()
                                .text_color(cx.theme().colors.warning),
                        )
                    })
                    .child(
                        Button::new(SharedString::from(format!("test-{id}")))
                            .label("Test")
                            .ghost()
                            .with_size(ui::APP_CONTROL_SIZE)
                            .tooltip("Check the host, then the credentials")
                            .on_click(move |_, _, cx| {
                                test_server(idx, url.clone(), creds.clone(), cx)
                            }),
                    )
                    .child(
                        Button::new(SharedString::from(format!("srv-edit-{id}")))
                            .label("Edit")
                            .ghost()
                            .with_size(ui::APP_CONTROL_SIZE)
                            .on_click(move |_, _, cx| {
                                toggle.update(cx, |state, cx| {
                                    state.toggle(Some(idx));
                                    cx.notify();
                                })
                            }),
                    )
                    .child(
                        Button::new(SharedString::from(format!("srv-dup-{id}")))
                            .icon(IconName::Copy)
                            .ghost()
                            .with_size(ui::APP_CONTROL_SIZE)
                            .tooltip("Duplicate")
                            .on_click(move |_, _, cx| {
                                // A copy has not been tested, whatever the original knows.
                                let mut copy = duplicate.clone();
                                copy.label = format!("{} (copy)", copy.label).into();
                                copy.last_check = None;
                                AppSettings::upsert_server_config(None, copy, cx);
                            }),
                    )
                    .child(
                        Button::new(SharedString::from(format!("srv-remove-{id}")))
                            .icon(IconName::Close)
                            .ghost()
                            .with_size(ui::APP_CONTROL_SIZE)
                            .on_click(move |_, _, cx| AppSettings::remove_server_config(idx, cx)),
                    ),
            )
            .children(check_line(config, cx))
            .into_any_element();
    }

    let label = row_input(
        format!("srv-label-{id}"),
        &config.label,
        "Label",
        window,
        cx,
    );
    let url = row_input(
        format!("srv-url-{id}"),
        &config.server_url,
        "https://islandora.example.org",
        window,
        cx,
    );
    let creds = row_input(
        format!("srv-creds-{id}"),
        &config.credentials_file,
        "Path to the credentials YAML",
        window,
        cx,
    );
    let confirm = window.use_keyed_state(SharedString::from(format!("srv-confirm-{id}")), cx, {
        let seed = config.needs_confirmation;
        move |_, _| seed
    });

    let (l, u, c, cf) = (label.clone(), url.clone(), creds.clone(), confirm.clone());
    let close = open.clone();
    let cancel = open.clone();
    let browse = creds.clone();
    let confirm_now = *confirm.read(cx);

    row_shell()
        .child(Input::new(&label).small().w_full())
        .child(Input::new(&url).small().w_full())
        .child(
            ui::FieldRow::new(Input::new(&creds).small().w_full()).child(
                Button::new(SharedString::from(format!("srv-browse-{id}")))
                    .icon(IconName::FolderOpen)
                    .outline()
                    .with_size(ui::APP_CONTROL_SIZE)
                    .on_click(move |_, window, cx| {
                        ui::pick_into_app(
                            window,
                            cx,
                            browse.clone(),
                            "Select credentials file",
                            false,
                        )
                    }),
            ),
        )
        .child(
            Checkbox::new(SharedString::from(format!("srv-confirm-box-{id}")))
                .small()
                .checked(confirm_now)
                .label("Always confirm before a destructive run on this server")
                .on_click({
                    let confirm = confirm.clone();
                    move |checked: &bool, _, cx| {
                        let value = *checked;
                        confirm.update(cx, |state, cx| {
                            *state = value;
                            cx.notify();
                        });
                    }
                }),
        )
        .child(
            h_flex()
                .w_full()
                .gap_2()
                .justify_end()
                .child(
                    Button::new(SharedString::from(format!("srv-cancel-{id}")))
                        .ghost()
                        .with_size(ui::APP_CONTROL_SIZE)
                        .label("Cancel")
                        .on_click(move |_, _, cx| {
                            cancel.update(cx, |state, cx| {
                                state.toggle(index);
                                cx.notify();
                            })
                        }),
                )
                .child(
                    Button::new(SharedString::from(format!("srv-save-{id}")))
                        .primary()
                        .with_size(ui::APP_CONTROL_SIZE)
                        .label(if index.is_some() { "Save" } else { "Add" })
                        .on_click(move |_, _, cx| {
                            AppSettings::upsert_server_config(
                                index,
                                ServerConfig {
                                    label: value_of(&l, cx),
                                    server_url: value_of(&u, cx),
                                    credentials_file: value_of(&c, cx),
                                    needs_confirmation: *cf.read(cx),
                                    // `upsert` carries the old result over when the URL and
                                    // credentials file are unchanged.
                                    last_check: None,
                                },
                                cx,
                            );
                            close.update(cx, |state, cx| {
                                state.0 = None;
                                cx.notify();
                            });
                        }),
                ),
        )
        .into_any_element()
}

pub fn saved_servers_field() -> SettingItem {
    SettingItem::render(|_options, window, cx| {
        let open = window.use_keyed_state(SharedString::from("server-open-row"), cx, |_, _| {
            OpenRow::default()
        });
        let servers = AppSettings::get(cx).server_configs.clone();
        let mut rows: Vec<AnyElement> = servers
            .iter()
            .enumerate()
            .map(|(i, s)| server_row(Some(i), s, &open, window, cx))
            .collect();
        rows.push(server_row(
            None,
            &ServerConfig::default(),
            &open,
            window,
            cx,
        ));
        v_flex().gap_2().w_full().children(rows)
    })
}

// --- shared actions ---------------------------------------------------------------------

/// Run the two-stage check off the main thread and record what it found.
fn test_server(index: usize, url: String, credentials_file: String, cx: &mut App) {
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move {
                let path = (!credentials_file.is_empty()).then(|| PathBuf::from(credentials_file));
                check_server(&url, path.as_deref())
            })
            .await;
        cx.update(|cx| {
            AppSettings::set_server_check(
                index,
                CheckResult::now(result.reachable, result.credentials_ok, result.message),
                cx,
            );
        });
    })
    .detach();
}
