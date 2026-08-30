use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

use gpui::*;
use gpui_component::{
    WindowExt,
    button::{Button, ButtonVariants},
    label::Label,
};
use workbench_integration::{StdinSink, StreamLine, format_stream_line};

use super::Workspace;

/// Drains a [`StreamLine`] receiver on a background task and appends each line to the workspace
/// log. When an [`StreamLine::InputRequired`] prompt is received:
/// - If `auto_accept` is true, sends "y" immediately.
/// - Otherwise opens a Y/N dialog overlay on the workspace window.
pub fn spawn_stream_to_log(
    workspace: Entity<Workspace>,
    rx: Receiver<StreamLine>,
    stdin_sink: StdinSink,
    auto_accept: bool,
    window: &mut Window,
    cx: &mut Context<Workspace>,
    on_complete: impl FnOnce(&mut Workspace, &mut Context<Workspace>) + Send + 'static,
) {
    let rx = Arc::new(Mutex::new(rx));
    cx.spawn_in(window, async move |_, cx| {
        loop {
            let rx2 = rx.clone();
            let line = cx
                .background_spawn(async move { rx2.lock().unwrap().recv() })
                .await;
            let Ok(line) = line else {
                break;
            };

            match &line {
                StreamLine::InputRequired(prompt) => {
                    let prompt_text = prompt.clone();
                    let prompt_msg = format!("[PROMPT] {}", prompt_text);
                    log::debug!("stream: prompt detected: {prompt_text}");

                    if auto_accept {
                        stdin_sink.send("y");
                        let accept_msg = "[AUTO-ACCEPT] Sent: y".to_string();
                        cx.update(|window, cx| {
                            workspace.update(cx, |this, cx| {
                                this.append_log(&prompt_msg, window, cx);
                                this.append_log(&accept_msg, window, cx);
                                cx.notify();
                            });
                        })
                        .ok();
                    } else {
                        log::debug!("stream: opening dialog for prompt {prompt_text:?}");
                        let sink_for_dialog = stdin_sink.clone();
                        let workspace_ref = workspace.clone();
                        let prompt_log = prompt_msg.clone();
                        let prompt_display = prompt_text.clone();
                        cx.update(move |window, cx| {
                            workspace_ref.update(cx, |this, cx| {
                                this.append_log(&prompt_log, window, cx);
                                cx.notify();
                            });
                            window.open_dialog(cx, move |dialog, _, _| {
                                let sink_yes = sink_for_dialog.clone();
                                let sink_no = sink_for_dialog.clone();
                                dialog
                                    .title("Workbench prompt")
                                    .overlay(true)
                                    .overlay_closable(false)
                                    .keyboard(false)
                                    .child(Label::new(prompt_display.clone()))
                                    .footer(move |_, _window, _cx, _| {
                                        let sy = sink_yes.clone();
                                        let sn = sink_no.clone();
                                        vec![
                                            Button::new("yes").label("Yes").primary().on_click(
                                                move |_, window, cx| {
                                                    sy.send("y");
                                                    window.close_dialog(cx);
                                                },
                                            ),
                                            Button::new("no").label("No").on_click(
                                                move |_, window, cx| {
                                                    sn.send("n");
                                                    window.close_dialog(cx);
                                                },
                                            ),
                                        ]
                                    })
                            });
                        })
                        .ok();
                    }
                    // Don't break — keep draining while the dialog is open.
                }
                _ => {
                    let should_break = matches!(&line, StreamLine::Done(_) | StreamLine::Error(_));
                    let msg = format_stream_line(&line);
                    log::trace!("stream: received {msg}");

                    if cx
                        .update(|window, cx| {
                            workspace.update(cx, |this, cx| {
                                this.append_log(&msg, window, cx);
                                cx.notify();
                            });
                        })
                        .is_err()
                    {
                        log::warn!("stream: cx.update failed (window closed?)");
                        break;
                    }

                    if should_break {
                        break;
                    }
                }
            }
        }

        log::debug!("stream: loop exited, calling on_complete");
        cx.update(|_, cx| {
            workspace.update(cx, |this, cx| {
                on_complete(this, cx);
            });
        })
        .ok();
    })
    .detach();
}
