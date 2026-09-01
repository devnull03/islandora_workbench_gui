//! The design language's spacing and geometry scales.
//!
//! Ported from the Claude Design canvas `4c5759a2` — `Design Language` §04/§05, normalised by
//! `Component Spec` §00, which is the tie-breaker wherever the two disagree. Colour and
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

/// Chips, tags and nav items — the smallest of the four radii §00 allows. The other three are
/// theme roles (`cx.theme().radius`, `radius_lg`) and must be read from there, not from here.
pub const RADIUS_SM: Pixels = px(3.);

/// Input, select, button, segmented control, table row.
pub const CONTROL_H: Pixels = px(30.);
/// The compact variant: add-row affordances, run-order chips.
pub const CONTROL_H_SM: Pixels = px(26.);
/// Fixed width of a setting row's label column, so labels align down a whole page regardless of
/// what control sits beside them.
pub const LABEL_COL_W: Pixels = px(180.);
/// Settings / page navigation sidebar.
pub const SIDEBAR_W: Pixels = px(176.);
/// Custom title bar, owned by `window-wrapper`.
pub const TITLE_BAR_H: Pixels = px(34.);
/// The setting picker's trigger, one step taller than an ordinary control because it is the only
/// thing on its row (Component Spec §08).
pub const PICKER_TRIGGER_H: Pixels = px(34.);
/// How far the picker's result panel may grow before it scrolls.
pub const PICKER_MAX_H: Pixels = px(360.);
/// A removable chip in a chip list, or a run-order chip.
pub const CHIP_H: Pixels = px(22.);

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
pub const KEY_COL_W: Pixels = px(100.);
/// One chip in a map-of-lists row.
pub const LIST_CELL_W: Pixels = px(90.);
/// Numeric field. Wide enough for six digits, narrow enough that a port number does not read as
/// a sentence-length field.
pub const NUMBER_FIELD_W: Pixels = px(120.);
/// Single-character field: centred, one grapheme, no room to suggest otherwise.
pub const CHAR_FIELD_W: Pixels = px(44.);
/// The builder's trailing value-shape column. Builder only — Component Spec §00 calls this the
/// one legitimate difference between the builder and the settings window.
pub const TYPE_COL_W: Pixels = px(56.);
/// The glyph slot in front of a validation message, fixed so a warning's text and an error's text
/// start at the same x.
pub const GLYPH_COL_W: Pixels = px(14.);

// -- Windows ---------------------------------------------------------------------------------

/// Narrowest supported window. Every label and control row must survive this width without
/// clipping; three call sites used to spell it out independently.
pub const MIN_WINDOW_W: Pixels = px(520.);
