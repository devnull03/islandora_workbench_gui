//! Reusable GPUI controls shared by application features.
//!
//! Anything that decides how the app *looks* belongs here: the theme install, the spacing and
//! geometry scales ([`tokens`]), and every element shape that more than one screen draws. A
//! second copy of a control in `app` or `config-builder` is how two adjacent buttons end up
//! different heights, which is the specific defect this crate exists to prevent.

mod controls;
mod field_row;
mod labeled_field;
mod labeled_select;
mod path_picker;
mod select_items;
mod step_section;
pub mod theme;
pub mod tokens;

pub use controls::{APP_CONTROL_SIZE, app_button};
pub use field_row::FieldRow;
pub use labeled_field::LabeledField;
pub use labeled_select::LabeledSelect;
pub use path_picker::{PathPickFn, PathPicker, pick_into, pick_into_app};
pub use select_items::DetailSelectItem;
pub use step_section::StepSection;
