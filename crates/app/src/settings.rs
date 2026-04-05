use gpui::*;
use gpui_component::{
    StyledExt, TitleBar,
    button::{Button, ButtonVariants},
    h_flex,
    label::Label,
    v_flex,
};

pub struct SettingsWindow;

impl SettingsWindow {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .child(
                TitleBar::new()
                    .child(Label::new("Settings").font_semibold())
            )
            .child(
                v_flex()
                    .flex_1()
                    .p_4()
                    .gap_4()
                    .child(
                        v_flex()
                            .gap_2()
                            .child(Label::new("Workbench Path"))
                            .child(Label::new("TODO: Add settings fields here"))
                    )
                    .child(
                        h_flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                Button::new("cancel")
                                    .outline()
                                    .label("Cancel")
                                    .on_click(move |_, window, _cx| {
                                        window.remove_window();
                                    })
                            )
                            .child(
                                Button::new("save")
                                    .primary()
                                    .label("Save")
                            )
                    )
            )
    }
}
