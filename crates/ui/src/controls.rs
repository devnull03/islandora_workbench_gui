//! Shared sizing for ordinary application controls, and the two button skins the design language
//! names but gpui-component does not ship.
//!
//! Component Spec §00 fixes control height at 30px and the compact variant at 26px, and §10 says
//! no component may set its own height. gpui-component's scale offers 24px (`Small`) and 32px
//! (`Medium`) — a literal `Size::Size(px)` never reaches input height, because `input_h` falls
//! through to `h_6`. `Medium` is the closest reachable value, it is one constant, and it keeps
//! the "no component sets its own height" rule intact. That is a deliberate 2px deviation.

use gpui::ElementId;
use gpui_component::{
    IconName, Sizable, Size,
    button::{Button, ButtonVariants as _},
};

/// Standard size for controls in forms and workflow rows (32px in gpui-component; the spec's 30).
pub const APP_CONTROL_SIZE: Size = Size::Medium;

/// The compact variant: add-row strips, row actions, inline actions (24px; the spec's 26).
pub const APP_CONTROL_SIZE_SM: Size = Size::Small;

/// Construct a normal application button at [`APP_CONTROL_SIZE`]. Variants and labels remain the
/// caller's responsibility because they communicate hierarchy, not geometry.
pub fn app_button(id: impl Into<ElementId>) -> Button {
    Button::new(id).with_size(APP_CONTROL_SIZE)
}

/// The skin §06 puts under every inline action — Edit, Test, Open, Unlink, Manage.
///
/// It exists because §00 bans bare clickable text: "No unbordered clickable text anywhere except
/// a real hyperlink." Half a dozen sites in this app were drawing a `Label` with a click handler,
/// which gives no hover surface and no focus ring.
pub fn ghost_button(id: impl Into<ElementId>) -> Button {
    Button::new(id).with_size(APP_CONTROL_SIZE_SM).ghost()
}

/// The dashed "+ Add a server" / "+ Add row" affordance that closes a list editor.
///
/// Dashed on purpose (§06): it has to read as *not a row*, or an empty editor looks like it
/// already contains one blank entry. `noun` is singular — the label is always `+ ` plus the thing
/// one press produces.
pub fn add_row_button(id: impl Into<ElementId>, noun: &str) -> Button {
    Button::new(id)
        .with_size(APP_CONTROL_SIZE_SM)
        .ghost()
        .icon(IconName::Plus)
        .label(format!("Add {noun}"))
}
