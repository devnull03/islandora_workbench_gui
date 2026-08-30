use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme, IconName, Sizable, StyledExt,
    button::{Button, ButtonVariants},
    h_flex,
    label::Label,
    setting::{SettingField, SettingItem},
    v_flex,
};

use std::path::PathBuf;

use settings::{AppSettings, ServerConfig, TaskConfig};

use config_builder::open_config_builder;

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

// --- Task Config UI ---

fn task_config_row(idx: usize, config: &TaskConfig, cx: &App) -> impl IntoElement {
    h_flex()
        .gap_2()
        .items_center()
        .p_2()
        .w_full()
        .rounded_md()
        .bg(cx.theme().colors.secondary)
        .child(
            v_flex()
                .flex_1()
                .overflow_hidden()
                .child(
                    div()
                        .max_w(px(250.))
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(Label::new(config.label.clone()).font_semibold()),
                )
                .child(
                    div()
                        .max_w(px(250.))
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(
                            Label::new(format!("Task: {}", config.task_name))
                                .text_xs()
                                .text_color(cx.theme().colors.muted_foreground),
                        ),
                )
                .child(
                    div()
                        .max_w(px(250.))
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(
                            Label::new(config.file_path.clone())
                                .text_xs()
                                .text_color(cx.theme().colors.muted_foreground),
                        ),
                ),
        )
        .child({
            // Minimum hook-up into the config builder. The richer Edit / New beside the config
            // picker in the main window is Stage 2 — docs/plans/stage-2-main-window.md.
            let path = PathBuf::from(config.file_path.as_ref());
            Button::new(SharedString::from(format!("edit-{}", idx)))
                .icon(IconName::Settings2)
                .ghost()
                .xsmall()
                .tooltip("Edit in the config builder")
                .on_click(move |_, _, cx| open_config_builder(Some(path.clone()), cx))
        })
        .child(
            Button::new(SharedString::from(format!("remove-{}", idx)))
                .icon(IconName::Close)
                .ghost()
                .xsmall()
                .on_click(move |_, _, cx| AppSettings::remove_task_config(idx, cx)),
        )
}

pub fn saved_configs_field() -> SettingItem {
    SettingItem::new(
        "Configurations",
        SettingField::render(|_options, _window, cx| {
            let configs = &AppSettings::get(cx).task_configs;
            v_flex()
                .gap_2()
                .w_full()
                .children(
                    configs
                        .iter()
                        .enumerate()
                        .map(|(idx, config)| task_config_row(idx, config, cx)),
                )
                .when(configs.is_empty(), |this| {
                    this.child(
                        Label::new("No configurations added yet.")
                            .text_color(cx.theme().colors.muted_foreground),
                    )
                })
        }),
    )
    .layout(Axis::Vertical)
}

pub fn add_config_button() -> SettingItem {
    SettingItem::new(
        "",
        SettingField::render(|_options, _window, _cx| {
            h_flex().justify_end().w_full().child(
                Button::new("add-config")
                    .primary()
                    .small()
                    .label("Add Configuration")
                    .on_click(|_, _, cx| AppSettings::add_task_config(cx)),
            )
        }),
    )
}

// --- Server Config UI ---

fn server_config_row(idx: usize, config: &ServerConfig, cx: &App) -> impl IntoElement {
    h_flex()
        .gap_2()
        .items_center()
        .p_2()
        .w_full()
        .rounded_md()
        .bg(cx.theme().colors.secondary)
        .child(
            v_flex()
                .flex_1()
                .overflow_hidden()
                .child(
                    div()
                        .max_w(px(250.))
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(Label::new(config.label.clone()).font_semibold()),
                )
                .child(
                    div()
                        .max_w(px(250.))
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(
                            Label::new(config.server_url.clone())
                                .text_xs()
                                .text_color(cx.theme().colors.muted_foreground),
                        ),
                )
                .child(
                    div()
                        .max_w(px(250.))
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(
                            Label::new(config.credentials_file.clone())
                                .text_xs()
                                .text_color(cx.theme().colors.muted_foreground),
                        ),
                ),
        )
        .child(
            Button::new(SharedString::from(format!("remove-server-{}", idx)))
                .icon(IconName::Close)
                .ghost()
                .xsmall()
                .on_click(move |_, _, cx| AppSettings::remove_server_config(idx, cx)),
        )
}

pub fn saved_servers_field() -> SettingItem {
    SettingItem::new(
        "Servers",
        SettingField::render(|_options, _window, cx| {
            let configs = &AppSettings::get(cx).server_configs;
            v_flex()
                .gap_2()
                .w_full()
                .children(
                    configs
                        .iter()
                        .enumerate()
                        .map(|(idx, config)| server_config_row(idx, config, cx)),
                )
                .when(configs.is_empty(), |this| {
                    this.child(
                        Label::new("No servers added yet.")
                            .text_color(cx.theme().colors.muted_foreground),
                    )
                })
        }),
    )
    .layout(Axis::Vertical)
}

pub fn add_server_button() -> SettingItem {
    SettingItem::new(
        "",
        SettingField::render(|_options, _window, _cx| {
            h_flex().justify_end().w_full().child(
                Button::new("add-server")
                    .primary()
                    .small()
                    .label("Add Server")
                    .on_click(|_, _, cx| AppSettings::add_server_config(cx)),
            )
        }),
    )
}
