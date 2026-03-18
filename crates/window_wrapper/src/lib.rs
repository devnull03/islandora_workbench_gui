pub mod title_bar;
pub mod status_bar;


use gpui::*;
pub use gpui_component::TitleBar;

use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = this_app)]
#[serde(deny_unknown_fields)]
pub struct OpenBrowser {
    pub url: String,
}
