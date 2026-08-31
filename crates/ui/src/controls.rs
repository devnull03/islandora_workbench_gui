//! Shared sizing for ordinary application controls.
//!
//! GPUI Component defaults buttons, inputs, and selects to `Medium`, but most of this app's
//! workflow UI is the denser desktop layout shown in the design. Keeping the size here prevents
//! an input and the button beside it from quietly drifting to different heights.

use gpui::ElementId;
use gpui_component::{Sizable, Size, button::Button};

/// Standard size for controls in forms and workflow rows (24 px in gpui-component).
pub const APP_CONTROL_SIZE: Size = Size::Small;

/// Construct a normal application button at [`APP_CONTROL_SIZE`]. Variants and labels remain the
/// caller's responsibility because they communicate hierarchy, not geometry.
pub fn app_button(id: impl Into<ElementId>) -> Button {
    Button::new(id).with_size(APP_CONTROL_SIZE)
}
