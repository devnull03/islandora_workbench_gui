use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::{
    ActiveTheme, IconName, Sizable, StyledExt, TitleBar,
    button::{Button, ButtonVariants},
    h_flex,
    label::Label,
    scroll::ScrollableElement,
    setting::{Settings, SettingPage, SettingGroup, SettingItem, SettingField},
    v_flex,
};

#[derive(Clone)]
pub struct TaskConfig {
    pub label: String,
    pub task_name: String,
    pub file_path: String,
}

#[derive(Clone)]
pub struct AppSettings {
    pub workbench_path: SharedString,
    pub python_path: SharedString,
    pub uv_path: SharedString,
    pub use_uv: bool,
    pub task_configs: Vec<TaskConfig>,
    // Temp fields for new task input
    pub new_task_label: SharedString,
    pub new_task_name: SharedString,
    pub new_task_path: SharedString,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            workbench_path: "".into(),
            python_path: "".into(),
            uv_path: "".into(),
            use_uv: false,
            task_configs: vec![],
            new_task_label: "".into(),
            new_task_name: "".into(),
            new_task_path: "".into(),
        }
    }
}

impl Global for AppSettings {}

impl AppSettings {
    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    pub fn global_mut(cx: &mut App) -> &mut Self {
        cx.global_mut::<Self>()
    }

    pub fn add_task_config(cx: &mut App) {
        let settings = Self::global(cx);
        let label = settings.new_task_label.to_string();
        let task_name = settings.new_task_name.to_string();
        let file_path = settings.new_task_path.to_string();

        if label.is_empty() || task_name.is_empty() || file_path.is_empty() {
            return;
        }

        let settings = Self::global_mut(cx);
        settings.task_configs.push(TaskConfig {
            label,
            task_name,
            file_path,
        });
        settings.new_task_label = "".into();
        settings.new_task_name = "".into();
        settings.new_task_path = "".into();
    }

    pub fn remove_task_config(index: usize, cx: &mut App) {
        let settings = Self::global_mut(cx);
        if index < settings.task_configs.len() {
            settings.task_configs.remove(index);
        }
    }
}

pub struct SettingsWindow;

impl SettingsWindow {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .child(
                TitleBar::new()
                    .child(Label::new("Settings").font_semibold())
            )
            .child(
                div()
                    .flex_1()
                    .overflow_y_scrollbar()
                    .child(
                        Settings::new("app-settings")
                            .pages(vec![
                                SettingPage::new("Paths")
                                    .default_open(true)
                                    .groups(vec![
                                        SettingGroup::new()
                                            .title("Workbench Installation")
                                            .items(vec![
                                                SettingItem::new(
                                                    "Workbench Path",
                                                    SettingField::input(
                                                        |cx: &App| AppSettings::global(cx).workbench_path.clone(),
                                                        |val: SharedString, cx: &mut App| {
                                                            AppSettings::global_mut(cx).workbench_path = val;
                                                        },
                                                    )
                                                )
                                                .description("Path to Islandora Workbench installation directory"),
                                            ]),
                                        SettingGroup::new()
                                            .title("Python Environment")
                                            .items(vec![
                                                SettingItem::new(
                                                    "Python Path",
                                                    SettingField::input(
                                                        |cx: &App| AppSettings::global(cx).python_path.clone(),
                                                        |val: SharedString, cx: &mut App| {
                                                            AppSettings::global_mut(cx).python_path = val;
                                                        },
                                                    )
                                                )
                                                .description("Path to Python executable"),
                                                SettingItem::new(
                                                    "UV Path",
                                                    SettingField::input(
                                                        |cx: &App| AppSettings::global(cx).uv_path.clone(),
                                                        |val: SharedString, cx: &mut App| {
                                                            AppSettings::global_mut(cx).uv_path = val;
                                                        },
                                                    )
                                                )
                                                .description("Path to UV package manager (optional)"),
                                                SettingItem::new(
                                                    "Use UV",
                                                    SettingField::switch(
                                                        |cx: &App| AppSettings::global(cx).use_uv,
                                                        |val: bool, cx: &mut App| {
                                                            AppSettings::global_mut(cx).use_uv = val;
                                                        },
                                                    )
                                                )
                                                .description("Use UV instead of pip for running workbench"),
                                            ]),
                                    ]),
                                SettingPage::new("Task Configs")
                                    .default_open(true)
                                    .groups(vec![
                                        SettingGroup::new()
                                            .title("Saved Configurations")
                                            .items(vec![
                                                SettingItem::new(
                                                    "Configurations",
                                                    SettingField::render(|_options, _window, cx| {
                                                        let configs = AppSettings::global(cx).task_configs.clone();
                                                        
                                                        v_flex()
                                                            .gap_2()
                                                            .w_full()
                                                            .children(
                                                                configs.iter().enumerate().map(|(idx, config)| {
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
                                                                                        .child(
                                                                                            Label::new(config.label.clone())
                                                                                                .font_semibold()
                                                                                        )
                                                                                )
                                                                                .child(
                                                                                    div()
                                                                                        .max_w(px(250.))
                                                                                        .overflow_hidden()
                                                                                        .text_ellipsis()
                                                                                        .child(
                                                                                            Label::new(format!("Task: {}", config.task_name))
                                                                                                .text_xs()
                                                                                                .text_color(cx.theme().colors.muted_foreground)
                                                                                        )
                                                                                )
                                                                                .child(
                                                                                    div()
                                                                                        .max_w(px(250.))
                                                                                        .overflow_hidden()
                                                                                        .text_ellipsis()
                                                                                        .child(
                                                                                            Label::new(config.file_path.clone())
                                                                                                .text_xs()
                                                                                                .text_color(cx.theme().colors.muted_foreground)
                                                                                        )
                                                                                )
                                                                        )
                                                                        .child(
                                                                            Button::new(SharedString::from(format!("remove-{}", idx)))
                                                                                .icon(IconName::Close)
                                                                                .ghost()
                                                                                .xsmall()
                                                                                .on_click(move |_, _, cx| {
                                                                                    AppSettings::remove_task_config(idx, cx);
                                                                                })
                                                                        )
                                                                })
                                                            )
                                                            .when(configs.is_empty(), |this| {
                                                                this.child(
                                                                    Label::new("No configurations added yet.")
                                                                        .text_color(cx.theme().colors.muted_foreground)
                                                                )
                                                            })
                                                    })
                                                )
                                                .layout(Axis::Vertical),
                                            ]),
                                        SettingGroup::new()
                                            .title("Add New Configuration")
                                            .items(vec![
                                                SettingItem::new(
                                                    "Label",
                                                    SettingField::input(
                                                        |cx: &App| AppSettings::global(cx).new_task_label.clone(),
                                                        |val: SharedString, cx: &mut App| {
                                                            AppSettings::global_mut(cx).new_task_label = val;
                                                        },
                                                    )
                                                )
                                                .description("Display name for this configuration"),
                                                SettingItem::new(
                                                    "Task Name",
                                                    SettingField::dropdown(
                                                        vec![
                                                            ("create".into(), "create - Create new nodes".into()),
                                                            ("create_from_files".into(), "create_from_files - Create from files".into()),
                                                            ("update".into(), "update - Update existing nodes".into()),
                                                            ("delete".into(), "delete - Delete nodes".into()),
                                                            ("add_media".into(), "add_media - Add media to nodes".into()),
                                                            ("update_media".into(), "update_media - Update existing media".into()),
                                                            ("delete_media".into(), "delete_media - Delete media".into()),
                                                            ("delete_media_by_node".into(), "delete_media_by_node - Delete media by node".into()),
                                                            ("export_csv".into(), "export_csv - Export content to CSV".into()),
                                                            ("get_data_from_view".into(), "get_data_from_view - Get data from view".into()),
                                                        ],
                                                        |cx: &App| AppSettings::global(cx).new_task_name.clone(),
                                                        |val: SharedString, cx: &mut App| {
                                                            AppSettings::global_mut(cx).new_task_name = val;
                                                        },
                                                    )
                                                )
                                                .description("Workbench task to perform"),
                                                SettingItem::new(
                                                    "Config File",
                                                    SettingField::render(|options, _window, cx| {
                                                        let current_path = AppSettings::global(cx).new_task_path.clone();
                                                        
                                                        h_flex()
                                                            .gap_2()
                                                            .w_full()
                                                            .child(
                                                                                div()
                                                                                    .flex_1()
                                                                                    .max_w(px(250.))
                                                                                    .px_2()
                                                                                    .py_1()
                                                                                    .rounded_md()
                                                                                    .border_1()
                                                                                    .border_color(cx.theme().colors.border)
                                                                                    .bg(cx.theme().colors.background)
                                                                                    .overflow_hidden()
                                                                                    .text_ellipsis()
                                                                                    .child(
                                                                                        Label::new(if current_path.is_empty() {
                                                                                            "No file selected...".into()
                                                                                        } else {
                                                                                            current_path
                                                                                        })
                                                                                        .text_color(if AppSettings::global(cx).new_task_path.is_empty() {
                                                                                            cx.theme().colors.muted_foreground
                                                                                        } else {
                                                                                            cx.theme().colors.foreground
                                                                                        })
                                                                                    )
                                                                            )
                                                            .child(
                                                                Button::new("browse-config-file")
                                                                    .icon(IconName::FolderOpen)
                                                                    .outline()
                                                                    .with_size(options.size)
                                                                    .on_click(|_, _, cx| {
                                                                        let receiver = cx.prompt_for_paths(PathPromptOptions {
                                                                            files: true,
                                                                            directories: false,
                                                                            multiple: false,
                                                                            prompt: Some("Select config file".into()),
                                                                        });
                                                                        
                                                                        cx.spawn(async move |cx| {
                                                                            if let Ok(Ok(Some(paths))) = receiver.await {
                                                                                if let Some(path) = paths.first() {
                                                                                    cx.update(|cx| {
                                                                                        AppSettings::global_mut(cx).new_task_path = 
                                                                                            path.to_string_lossy().to_string().into();
                                                                                    }).ok();
                                                                                }
                                                                            }
                                                                        }).detach();
                                                                    })
                                                            )
                                                    })
                                                )
                                                .description("Path to YAML configuration file"),
                                                SettingItem::new(
                                                    "",
                                                    SettingField::render(|_options, _window, _cx| {
                                                        h_flex()
                                                            .justify_end()
                                                            .w_full()
                                                            .child(
                                                                Button::new("add-config")
                                                                    .primary()
                                                                    .small()
                                                                    .label("Add Configuration")
                                                                    .on_click(|_, _, cx| {
                                                                        AppSettings::add_task_config(cx);
                                                                    })
                                                            )
                                                    })
                                                ),
                                            ]),
                                    ]),
                            ])
                    )
            )
            .child(
                h_flex()
                    .p_4()
                    .justify_end()
                    .gap_2()
                    .border_t_1()
                    .border_color(cx.theme().colors.border)
                    .child(
                        Button::new("close")
                            .outline()
                            .label("Close")
                            .on_click(move |_, window, _cx| {
                                window.remove_window();
                            })
                    )
            )
    }
}
