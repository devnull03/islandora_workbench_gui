//! Select dropdown rows with a subtitle line and optional top divider.

use std::path::Path;

use gpui::{App, IntoElement, ParentElement, SharedString, Styled, StyledText, Window, div, px};
use gpui_component::{ActiveTheme, select::SelectItem, v_flex};
use settings::{ServerConfig, TaskConfig};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetailSelectItem {
    pub label: SharedString,
    pub subtitle: SharedString,
    pub value: SharedString,
    pub divider_above: bool,
}

impl SelectItem for DetailSelectItem {
    type Value = SharedString;

    fn title(&self) -> SharedString {
        self.label.clone()
    }
    fn value(&self) -> &Self::Value {
        &self.value
    }

    fn render(&self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let body = v_flex()
            .gap_0p5()
            .py_0p5()
            .child(
                div()
                    .text_color(cx.theme().foreground)
                    .child(StyledText::new(&self.label)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(StyledText::new(&self.subtitle))
                    .overflow_x_hidden(),
            );
        if self.divider_above {
            v_flex()
                .w_full()
                .child(div().h(px(1.)).bg(cx.theme().colors.border))
                .child(body)
        } else {
            body
        }
    }

    fn matches(&self, query: &str) -> bool {
        let q = query.to_lowercase();
        self.label.to_lowercase().contains(&q) || self.subtitle.to_lowercase().contains(&q)
    }
}

impl From<(usize, &ServerConfig)> for DetailSelectItem {
    fn from((index, item): (usize, &ServerConfig)) -> Self {
        Self {
            label: item.label.clone(),
            subtitle: item.server_url.clone(),
            value: item.server_url.clone(),
            divider_above: index > 0,
        }
    }
}

impl From<(usize, &TaskConfig)> for DetailSelectItem {
    fn from((index, item): (usize, &TaskConfig)) -> Self {
        let path_str = item.file_path.to_string();
        let file_name = Path::new(&path_str)
            .file_name()
            .and_then(|s| s.to_str())
            .map(str::to_owned)
            .unwrap_or(path_str);
        Self {
            label: item.label.clone(),
            subtitle: file_name.into(),
            value: item.file_path.clone(),
            divider_above: index > 0,
        }
    }
}
