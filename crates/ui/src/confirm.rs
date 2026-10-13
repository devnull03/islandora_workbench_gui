//! ConfirmDialog (Component Spec §09): the one gate in front of an action that loses work.
//!
//! A thin layer over gpui-component's `AlertDialog`, which already supplies the modal, focus
//! trap, Esc-to-cancel and footer. What this adds is the spec's shape, so every caller asks the
//! same way: 420 px, the title is a question, the body says what is kept, Cancel is a ghost
//! button, and the action is the danger skin with a verb on it ("Discard", never "OK").

use gpui::{App, SharedString, Window};
use gpui_component::{WindowExt as _, button::ButtonVariant, dialog::DialogButtonProps};

use crate::tokens::CONFIRM_DIALOG_W;

/// Ask before `on_confirm` runs. The dialog shows in `window`, whose view must render
/// `Root::render_dialog_layer`.
pub fn confirm_dialog(
    window: &mut Window,
    cx: &mut App,
    title: impl Into<SharedString>,
    body: impl Into<SharedString>,
    action: impl Into<SharedString>,
    on_confirm: impl Fn(&mut Window, &mut App) + 'static,
) {
    let title = title.into();
    let body = body.into();
    let action = action.into();
    let on_confirm = std::rc::Rc::new(on_confirm);
    window.open_alert_dialog(cx, move |alert, _, _| {
        let on_confirm = on_confirm.clone();
        alert
            .width(CONFIRM_DIALOG_W)
            .title(title.clone())
            .description(body.clone())
            .button_props(
                DialogButtonProps::default()
                    .ok_text(action.clone())
                    .ok_variant(ButtonVariant::Danger)
                    .cancel_text("Cancel")
                    .cancel_variant(ButtonVariant::Ghost)
                    .show_cancel(true),
            )
            .on_ok(move |_, window, cx| {
                on_confirm(window, cx);
                true
            })
    });
}
