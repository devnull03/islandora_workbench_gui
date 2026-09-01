//! Reusable GPUI controls shared by application features.
//!
//! Anything that decides how the app *looks* belongs here: the theme install, the spacing and
//! geometry scales ([`tokens`]), and every element shape that more than one screen draws. A
//! second copy of a control in `app` or `config-builder` is how two adjacent buttons end up
//! different heights, which is the specific defect this crate exists to prevent.

mod app_font;
mod card;
mod chips;
mod controls;
mod field_row;
mod inline_message;
mod labeled_field;
mod labeled_select;
mod path_picker;
mod row_editor;
mod segmented;
mod select_items;
mod setting_row;
mod status_bar_button;
mod step_section;
mod summary_lines;
pub mod theme;
pub mod tokens;

pub use app_font::AppFont;
pub use card::{Card, CardTone};
pub use chips::ChipList;
pub use controls::{
    APP_CONTROL_SIZE, APP_CONTROL_SIZE_SM, add_row_button, app_button, ghost_button,
};
pub use field_row::FieldRow;
pub use inline_message::{InlineMessage, MessageLevel};
pub use labeled_field::LabeledField;
pub use labeled_select::LabeledSelect;
pub use path_picker::{PathPickFn, PathPicker, pick_into, pick_into_app};
pub use row_editor::RowEditor;
pub use segmented::{MAX_SEGMENTS, Segmented};
pub use select_items::DetailSelectItem;
pub use setting_row::SettingRow;
pub use status_bar_button::StatusBarButton;
pub use step_section::StepSection;
pub use summary_lines::SummaryLines;
