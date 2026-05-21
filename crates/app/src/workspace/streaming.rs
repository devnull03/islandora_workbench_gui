use std::sync::mpsc::Receiver;

use gpui::*;
use workbench_integration::{StreamLine, format_stream_line};

use super::Workspace;

/// Drains a [`StreamLine`] receiver on a background task and appends each line to the workspace log.
pub fn spawn_stream_to_log(
    workspace: Entity<Workspace>,
    rx: Receiver<StreamLine>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
    on_complete: impl FnOnce(&mut Workspace, &mut Context<Workspace>) + Send + 'static,
) {
    cx.spawn_in(window, async move |_, cx| {
        while let Ok(line) = rx.recv() {
            let should_break = matches!(line, StreamLine::Done(_) | StreamLine::Error(_));
            let msg = format_stream_line(&line);

            cx.update(|window, cx| {
                workspace.update(cx, |this, cx| {
                    this.append_log(&msg, window, cx);
                });
            })
            .ok();

            if should_break {
                break;
            }
        }
        cx.update(|_, cx| {
            workspace.update(cx, |this, cx| {
                on_complete(this, cx);
            });
        })
        .ok();
    })
    .detach();
}
