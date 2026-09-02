//! A path, and the button that opens a picker for it.
//!
//! Component Spec §04. One component for the whole app: the editable input plus a `Browse…`
//! button at the same height, with the button never truncated.
//!
//! This replaces the two shapes that existed before — a `PathPicker` struct literal with a
//! disabled field and a callback, used twice in the settings window, and six hand-built
//! `FieldRow` + `Button` pairs elsewhere that had drifted to three different button sizes and two
//! different labels. The difference between them was only ever *where the picked path goes*, so
//! that is the one thing this parameterises.

use std::path::PathBuf;
use std::sync::Arc;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    Disableable as _, IconName, Sizable as _, Size,
    button::Button,
    input::{Input, InputState},
};

use crate::{APP_CONTROL_SIZE, FieldRow};

/// Where a picked path goes when the input is not the sink — a list to append to, a draft field
/// that is not backed by this input.
pub type PathPickFn = Arc<dyn Fn(SharedString, &mut App) + Send + Sync>;

#[derive(IntoElement)]
pub struct PathField {
    id: SharedString,
    input: Entity<InputState>,
    prompt: SharedString,
    /// Pick a folder rather than a file.
    directories: bool,
    size: Size,
    /// The field shows the value but is not typed into. Used where the only legal way to set the
    /// path is the dialog.
    readonly: bool,
    disabled: bool,
    /// Set when the picked path must go somewhere other than `input`.
    on_pick: Option<PathPickFn>,
    /// Optional visible action label. The compact settings surface uses the icon-only default;
    /// wider form rows can opt into the explicit `Browse…` treatment from the builder mockup.
    button_label: Option<SharedString>,
}

impl PathField {
    pub fn new(id: impl Into<SharedString>, input: &Entity<InputState>) -> Self {
        Self {
            id: id.into(),
            input: input.clone(),
            prompt: "Select a path".into(),
            directories: false,
            size: APP_CONTROL_SIZE,
            readonly: false,
            disabled: false,
            on_pick: None,
            button_label: None,
        }
    }

    pub fn prompt(mut self, prompt: impl Into<SharedString>) -> Self {
        self.prompt = prompt.into();
        self
    }

    pub fn directories(mut self, directories: bool) -> Self {
        self.directories = directories;
        self
    }

    pub fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }

    pub fn readonly(mut self, readonly: bool) -> Self {
        self.readonly = readonly;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Send the picked path somewhere other than the input.
    pub fn on_pick(mut self, on_pick: PathPickFn) -> Self {
        self.on_pick = Some(on_pick);
        self
    }

    pub fn button_label(mut self, label: impl Into<SharedString>) -> Self {
        self.button_label = Some(label.into());
        self
    }
}

impl RenderOnce for PathField {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let prompt = self.prompt.clone();
        let directories = self.directories;
        let input = self.input.clone();
        let on_pick = self.on_pick.clone();

        let picker = Button::new(self.id)
            .with_size(self.size)
            .outline()
            .map(|button| match self.button_label {
                Some(label) => button.label(label),
                None => button.icon(IconName::FolderOpen),
            })
            .tooltip(if directories {
                "Choose a folder"
            } else {
                "Choose a file"
            })
            .flex_none()
            .disabled(self.disabled)
            .on_click(move |_, window, cx| match &on_pick {
                // The picker writes into the input asynchronously and the input's own Change
                // event is what commits, so the common case has nothing to do here.
                None => {
                    crate::pick_into_app(window, cx, input.clone(), prompt.clone(), directories)
                }
                Some(sink) => {
                    let receiver = cx.prompt_for_paths(PathPromptOptions {
                        files: !directories,
                        directories,
                        multiple: false,
                        prompt: Some(prompt.clone()),
                    });
                    let sink = Arc::clone(sink);
                    cx.spawn(async move |cx| {
                        if let Ok(Ok(Some(paths))) = receiver.await
                            && let Some(path) = paths.first()
                        {
                            let picked: SharedString =
                                PathBuf::from(path).to_string_lossy().to_string().into();
                            cx.update(|cx| sink(picked, cx));
                        }
                    })
                    .detach();
                }
            });

        FieldRow::new(
            Input::new(&self.input)
                .with_size(self.size)
                .disabled(self.readonly || self.disabled)
                .w_full(),
        )
        .child(picker)
    }
}
