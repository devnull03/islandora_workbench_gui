//! Reusable path-picker controls. Callers own the selected-path behavior.

use std::path::PathBuf;
use std::sync::Arc;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    AxisExt as _, IconName, Sizable, Size,
    button::Button,
    h_flex,
    input::{Input, InputState},
};

/// Callback after the user picks a path.
pub type PathPickFn = Arc<dyn Fn(SharedString, &mut App) + Send + Sync>;

#[derive(IntoElement)]
pub struct PathPicker {
    pub layout: Axis,
    pub field_size: Size,
    pub button_size: Option<Size>,
    pub button_id: SharedString,
    pub files: bool,
    pub directories: bool,
    pub prompt: SharedString,
    pub input: Entity<InputState>,
    pub on_pick: PathPickFn,
}

impl RenderOnce for PathPicker {
    fn render(self, _: &mut Window, _cx: &mut App) -> impl IntoElement {
        let prompt = self.prompt.clone();
        let files = self.files;
        let directories = self.directories;
        let mut btn = Button::new(self.button_id)
            .icon(IconName::FolderOpen)
            .outline();

        btn = match self.button_size {
            Some(s) => btn.with_size(s),
            None => btn.small(),
        };

        let on_pick = Arc::clone(&self.on_pick);
        let layout = self.layout;
        let field_size = self.field_size;

        h_flex()
            .gap_2()
            .w_full()
            .child(
                Input::new(&self.input)
                    .disabled(true)
                    .with_size(field_size)
                    .map(move |this| {
                        if layout.is_horizontal() {
                            this.w_64()
                        } else {
                            this.w_full()
                        }
                    }),
            )
            .child(btn.on_click(move |_, _, cx| {
                let receiver = cx.prompt_for_paths(PathPromptOptions {
                    files,
                    directories,
                    multiple: false,
                    prompt: Some(prompt.clone()),
                });
                let on_pick = Arc::clone(&on_pick);
                cx.spawn(async move |cx| {
                    if let Ok(Ok(Some(paths))) = receiver.await
                        && let Some(path) = paths.first()
                    {
                        let selected: SharedString = path.to_string_lossy().to_string().into();
                        cx.update(|cx| on_pick(selected, cx));
                    }
                })
                .detach();
            }))
    }
}

/// Browse for a file or folder and write the chosen path into `input`.
///
/// Two entry points rather than one because the two call shapes are genuinely different: a view
/// renders from a `Context<T>`, while `SettingItem::render` closures only ever get a bare
/// `&mut App`. The dialog options and the write are shared; only the spawn differs.
///
/// ponytail: the `oneshot::Receiver` these hand back is never named, so `ui` needs neither
/// `futures` nor `anyhow` as a dependency. That costs one duplicated `PathPromptOptions` literal.
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
