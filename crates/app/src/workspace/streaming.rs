use std::sync::{Arc, Mutex};
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
    let rx = Arc::new(Mutex::new(rx));
    cx.spawn_in(window, async move |_, cx| {
        loop {
            let rx2 = rx.clone();
            let line = cx.background_spawn(async move { rx2.lock().unwrap().recv() }).await;
            let Ok(line) = line else { break; };
            let should_break = matches!(line, StreamLine::Done(_) | StreamLine::Error(_));
            let msg = format_stream_line(&line);
            println!("[stream] received: {}", msg);

            if cx.update(|window, cx| {
                workspace.update(cx, |this, cx| {
                    this.append_log(&msg, window, cx);
                    cx.notify();
                });
            }).is_err() {
                println!("[stream] cx.update failed (window closed?)");
                break;
            }

            if should_break {
                break;
            }
        }
        println!("[stream] loop exited, calling on_complete");
        cx.update(|_, cx| {
            workspace.update(cx, |this, cx| {
                on_complete(this, cx);
            });
        })
        .ok();
    })
    .detach();
}
