//! The design language's spacing and geometry scales.
//!
//! Ported from the Claude Design canvas `4c5759a2` (`Design Language`, §04 and §05). Colour and
//! radius do *not* live here — they are theme roles, read as `cx.theme().radius` and
//! `cx.theme().colors.*`, so that they follow the light/dark mode. These are the values that do
//! not vary by mode.
//!
//! This is a desktop tool that shows ~140 settings at once, so the scale is deliberately tighter
//! than a web app's: the largest gap in a settings pane is 18px.

use gpui::{Pixels, px};

// -- Spacing (4px grid) ----------------------------------------------------------------------

/// Label to its description line.
pub const GAP_XS: Pixels = px(3.);
/// Inside one compound control; cells of a key/value row.
pub const GAP_SM: Pixels = px(6.);
/// Control to the button beside it; icon to text.
pub const GAP_MD: Pixels = px(8.);
/// Rows within one setting group.
pub const GAP_LG: Pixels = px(11.);
/// Label column to control column; cards in a grid.
pub const GAP_XL: Pixels = px(14.);
/// Group to group.
pub const GAP_2XL: Pixels = px(18.);
/// Horizontal padding of a settings page body.
pub const PAD_PAGE: Pixels = px(22.);

// -- Control geometry ------------------------------------------------------------------------

/// Input, select, button, segmented control, table row.
pub const CONTROL_H: Pixels = px(30.);
/// The compact variant: add-row affordances, run-order chips.
pub const CONTROL_H_SM: Pixels = px(26.);
/// Fixed width of a setting row's label column, so labels align down a whole page regardless of
/// what control sits beside them.
pub const LABEL_COL_W: Pixels = px(180.);
/// Settings / page navigation sidebar.
pub const SIDEBAR_W: Pixels = px(168.);
/// Custom title bar, owned by `window-wrapper`.
pub const TITLE_BAR_H: Pixels = px(34.);

// -- Tree and table columns ------------------------------------------------------------------

/// One level of indent in the secondary-task chain.
pub const INDENT_STEP: Pixels = px(18.);
/// The expand/collapse chevron slot, reserved even on leaf rows so siblings stay aligned.
pub const CHEVRON_SLOT: Pixels = px(24.);
/// Trailing remove-glyph column in a row editor, revealed on row hover.
pub const ROW_ACTION_W: Pixels = px(22.);
/// Leading drag-handle column in a row editor.
pub const DRAG_HANDLE_W: Pixels = px(16.);
/// Key column of a map editor.
pub const KEY_COL_W: Pixels = px(140.);

// -- Windows ---------------------------------------------------------------------------------

/// Narrowest supported window. Every label and control row must survive this width without
/// clipping; three call sites used to spell it out independently.
pub const MIN_WINDOW_W: Pixels = px(520.);
