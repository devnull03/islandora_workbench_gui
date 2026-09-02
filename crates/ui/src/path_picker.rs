//! Opening the platform file dialog and putting the answer somewhere.
//!
//! The chrome — field plus Browse button — is [`crate::PathField`]. This is only the dialog, kept
//! separate because two callers drive it from places that have no field to write into.
//!
//! ponytail: the `oneshot::Receiver` these hand back is never named, so `ui` needs neither
//! `futures` nor `anyhow` as a dependency. That costs one duplicated `PathPromptOptions` literal.

use std::path::PathBuf;

use gpui::*;
use gpui_component::input::InputState;

pub fn pick_into<T: 'static>(
    window: &mut Window,
    cx: &mut Context<T>,
    input: &Entity<InputState>,
    prompt: impl Into<SharedString>,
    is_folder: bool,
) {
    let receiver = cx.prompt_for_paths(PathPromptOptions {
        files: !is_folder,
        directories: is_folder,
        multiple: false,
        prompt: Some(prompt.into()),
    });
    let input = input.clone();
    cx.spawn_in(window, async move |_, cx| {
        if let Some(path) = picked(receiver.await) {
            set_input_path(&input, path, cx);
        }
    })
    .detach();
}

/// [`pick_into`] for callers holding only an `App`.
pub fn pick_into_app(
    window: &mut Window,
    cx: &mut App,
    input: Entity<InputState>,
    prompt: impl Into<SharedString>,
    is_folder: bool,
) {
    let receiver = cx.prompt_for_paths(PathPromptOptions {
        files: !is_folder,
        directories: is_folder,
        multiple: false,
        prompt: Some(prompt.into()),
    });
    window
        .spawn(cx, async move |cx| {
            if let Some(path) = picked(receiver.await) {
                set_input_path(&input, path, cx);
            }
        })
        .detach();
}

/// A cancelled dialog, a dropped channel, and a platform error all mean "the user picked
/// nothing", so they collapse into one `None`.
fn picked<E, C>(result: Result<Result<Option<Vec<PathBuf>>, E>, C>) -> Option<PathBuf> {
    result.ok()?.ok()?.into_iter().flatten().next()
}

fn set_input_path(input: &Entity<InputState>, path: PathBuf, cx: &mut AsyncWindowContext) {
    let value = path.to_string_lossy().to_string();
    cx.update(|window, cx| {
        input.update(cx, |state, cx| state.set_value(value, window, cx));
    })
    .ok();
}
