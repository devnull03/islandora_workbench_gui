use std::time::Duration;

use gpui::*;
use gpui_component::{
    ActiveTheme,
    IconName,
    Sizable,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputState},
    label::Label,
    v_flex,
};
use workbench_integration::{run_command_streaming, StreamLine};

pub struct Workspace {
    config_path: Entity<InputState>,
    working_directory: Entity<InputState>,
    log_state: Entity<InputState>,
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let config_path = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Path to config file...")
        });

        let working_directory = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Working directory...")
        });

        let log_state = cx.new(|cx| {
            InputState::new(window, cx).multi_line(true)
        });

        Self {
            config_path,
            working_directory,
            log_state,
        }
    }

    fn get_file(&self, window: &mut Window, cx: &mut Context<Self>, input: &Entity<InputState>, prompt: SharedString, is_folder: bool) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: !is_folder,
            directories: is_folder,
            multiple: false,
            prompt: Some(prompt),
        });

        let input = input.clone();
        cx.spawn_in(window, async move |_, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await {
                if let Some(path) = paths.first() {
                    cx.update(|window, cx| {
                        input.update(cx, |state, cx| {
                            state.set_value(path.to_string_lossy().to_string(), window, cx);
                        });
                    }).ok();
                }
            }
        }).detach();
    }

    fn append_log(&self, message: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.log_state.update(cx, |state, cx| {
            let current = state.value();
            let new_value = if current.is_empty() {
                message.to_string()
            } else {
                format!("{}\n{}", current, message)
            };
            state.set_value(new_value, window, cx);
        });
    }

    fn clear_logs(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.log_state.update(cx, |state, cx| {
            state.set_value("".to_string(), window, cx);
        });
    }

    fn run_dummy_ingest(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_logs(window, cx);
        self.append_log("[INFO] Starting ingest...", window, cx);

        let dummy_messages = vec![
            (300, "[INFO] Loading configuration..."),
            (500, "[INFO] Config loaded from: /path/to/config.yml"),
            (400, "[INFO] Validating input files..."),
            (600, "[WARN] Found 3 files with missing metadata"),
            (800, "[INFO] Processing batch 1 of 5..."),
            (500, "[INFO] Successfully processed 25 items"),
            (700, "[INFO] Processing batch 2 of 5..."),
            (400, "[INFO] Successfully processed 25 items"),
            (600, "[INFO] Processing batch 3 of 5..."),
            (900, "[ERROR] Failed to process item ID: 12345 - Connection timeout"),
            (500, "[INFO] Retrying failed items..."),
            (700, "[INFO] Processing batch 4 of 5..."),
            (400, "[INFO] Successfully processed 25 items"),
            (600, "[INFO] Processing batch 5 of 5..."),
            (500, "[INFO] Successfully processed 24 items"),
            (300, "[INFO] Processing complete. Total: 125 items, Success: 124, Failed: 1"),
        ];

        let entity = cx.entity().clone();
        cx.spawn_in(window, async move |_, cx| {
            for (delay_ms, message) in dummy_messages {
                cx.background_executor()
                    .timer(Duration::from_millis(delay_ms))
                    .await;

                let msg = message.to_string();
                cx.update(|window, cx| {
                    entity.update(cx, |this, cx| {
                        this.append_log(&msg, window, cx);
                    });
                }).ok();
            }
        }).detach();
    }

    /// Run a command and stream its output to logs
    fn run_command(&self, program: &str, args: &[&str], window: &mut Window, cx: &mut Context<Self>) {
        self.clear_logs(window, cx);
        self.append_log(&format!("[INFO] Running: {} {}", program, args.join(" ")), window, cx);

        let rx = match run_command_streaming(program, args) {
            Ok(rx) => rx,
            Err(e) => {
                self.append_log(&format!("[ERROR] Failed to start: {}", e), window, cx);
                return;
            }
        };

        let entity = cx.entity().clone();
        cx.spawn_in(window, async move |_, cx| {
            while let Ok(line) = rx.recv() {
                let should_break = matches!(line, StreamLine::Done(_) | StreamLine::Error(_));
                
                let msg = match line {
                    StreamLine::Stdout(msg) => msg,
                    StreamLine::Stderr(msg) => format!("[STDERR] {}", msg),
                    StreamLine::Done(code) => format!("[INFO] Process exited with code: {}", code),
                    StreamLine::Error(e) => format!("[ERROR] {}", e),
                };

                cx.update(|window, cx| {
                    entity.update(cx, |this, cx| {
                        this.append_log(&msg, window, cx);
                    });
                }).ok();

                if should_break {
                    break;
                }
            }
        }).detach();
    }
}

impl Render for Workspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .p_4()
            .justify_around()
            .child(
                v_flex()
                    .h_1_2()
                    .gap_3()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(Label::new("Config File"))
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(div().flex_1().child(Input::new(&self.config_path)))
                                    .child(
                                        Button::new("browse-config")
                                            .icon(IconName::FolderOpen)
                                            .outline()
                                            .small()
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.get_file(window, cx, &this.config_path, "Select config file".into(), false);
                                            }))
                                    )
                            )
                            .child(
                                Label::new("YAML configuration file for the ingest")
                                    .text_xs()
                                    .text_color(cx.theme().colors.muted_foreground)
                            ),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(Label::new("Working Directory"))
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(div().flex_1().child(Input::new(&self.working_directory)))
                                    .child(
                                        Button::new("browse-dir")
                                            .icon(IconName::FolderOpen)
                                            .outline()
                                            .small()
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.get_file(window, cx, &this.working_directory, "Select working directory".into(), true);
                                            }))
                                    )
                            )
                            .child(
                                Label::new("Directory containing assets to ingest")
                                    .text_xs()
                                    .text_color(cx.theme().colors.muted_foreground)
                            ),
                    )
                    .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("check")
                            .outline()
                            .label("Check")
                            .on_click(|_, _, _| {
                                println!("Check clicked");
                            }),
                    )
                    .child(
                        Button::new("run-ingest")
                            .primary()
                            .label("Run Ingest")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.run_dummy_ingest(window, cx);
                            })),
                    ),
            ) 
            ) 
            .child(
                v_flex()
                    .h_1_2()
                    .gap_1()
                    .child(Label::new("Logs"))
                    .child(
                        Input::new(&self.log_state)
                            .disabled(true)
                            .flex_1()
                    ),
            )
    }
}
