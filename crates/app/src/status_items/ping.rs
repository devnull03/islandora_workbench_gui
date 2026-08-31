use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine as _;
use gpui::*;
use gpui_component::ActiveTheme;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{Icon, IconName, Sizable, StyledExt, spinner::Spinner};
use settings::AppSettings;
use workbench_integration::read_credentials;

pub struct PingLogEvent(pub String);

pub struct ServerPingIndicator {
    state: ServerPingState,
    last_server_url: Option<SharedString>,
    _subscription: Subscription,
}

impl EventEmitter<PingLogEvent> for ServerPingIndicator {}

pub enum ServerPingState {
    NotSet,
    Reachable(u16),
    Unreachable(Option<u16>),
    Pinging,
}

impl ServerPingIndicator {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let initial_url = AppSettings::get(cx).default_server.clone();
        let subscription = cx.observe_global::<AppSettings>(|this, cx| {
            let new_url = AppSettings::get(cx).default_server.clone();
            if new_url == this.last_server_url {
                return;
            }
            this.last_server_url = new_url.clone();

            if new_url.is_none() {
                this.state = ServerPingState::NotSet;
                cx.notify();
                return;
            }

            this.ping(cx);
        });

        let mut this = Self {
            state: ServerPingState::NotSet,
            last_server_url: initial_url.clone(),
            _subscription: subscription,
        };
        if initial_url.is_some() {
            this.ping(cx);
        }
        this
    }

    fn ping(&mut self, cx: &mut Context<Self>) {
        let settings = AppSettings::get(cx);
        let Some(url) = settings.default_server.clone() else {
            return;
        };
        let credentials_file = settings
            .server_configs
            .iter()
            .find(|s| s.server_url == url)
            .filter(|s| !s.credentials_file.is_empty())
            .map(|s| PathBuf::from(s.credentials_file.as_ref()));
        let url = url.to_string();

        self.state = ServerPingState::Pinging;
        cx.notify();

        cx.spawn(async |this, cx| {
            let log_url = url.clone();
            let result = cx
                .background_executor()
                .spawn(async move { ping_server(&url, credentials_file.as_deref()) })
                .await;

            _ = cx.update(|cx| {
                _ = this.update(cx, |indicator, cx| {
                    let log_msg = match &result {
                        ServerPingState::Unreachable(Some(401)) => Some(format!(
                            "[Server] {log_url}: Unauthorized (401) — check credentials file"
                        )),
                        ServerPingState::Unreachable(Some(403)) => Some(format!(
                            "[Server] {log_url}: Forbidden (403) — user lacks permissions"
                        )),
                        ServerPingState::Unreachable(Some(code)) => {
                            Some(format!("[Server] {log_url}: Error response (HTTP {code})"))
                        }
                        ServerPingState::Unreachable(None) => {
                            Some(format!("[Server] {log_url}: Connection failed"))
                        }
                        _ => None,
                    };
                    indicator.state = result;
                    cx.notify();
                    if let Some(msg) = log_msg {
                        cx.emit(PingLogEvent(msg));
                    }
                });
            });
        })
        .detach();
    }
}

fn ping_server(url: &str, credentials_file: Option<&Path>) -> ServerPingState {
    let ping_url = format!(
        "{}/islandora_workbench_integration/version",
        url.trim_end_matches('/')
    );

    let creds = credentials_file.and_then(|p| read_credentials(p).ok());

    let mut req = ureq::get(&ping_url).timeout(Duration::from_secs(5));
    if let Some(c) = &creds {
        let encoded = base64::engine::general_purpose::STANDARD
            .encode(format!("{}:{}", c.username, c.password));
        req = req.set("Authorization", &format!("Basic {encoded}"));
    }

    match req.call() {
        Ok(resp) => ServerPingState::Reachable(resp.status()),
        Err(ureq::Error::Status(code, _)) => ServerPingState::Unreachable(Some(code)),
        Err(_) => ServerPingState::Unreachable(None),
    }
}

impl Render for ServerPingIndicator {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (icon, text) = match &self.state {
            ServerPingState::Reachable(code) => (
                div().child(Icon::new(IconName::CircleCheck).text_color(cx.theme().green)),
                SharedString::from(format!("Reachable {code}")),
            ),
            ServerPingState::Unreachable(Some(code)) => (
                div().child(Icon::new(IconName::CircleX).text_color(cx.theme().red)),
                SharedString::from(format!("Unreachable {code}")),
            ),
            ServerPingState::Unreachable(None) => (
                div().child(Icon::new(IconName::CircleX).text_color(cx.theme().red)),
                SharedString::from("Unreachable"),
            ),
            ServerPingState::Pinging => (
                div().child(Spinner::new().small()),
                SharedString::from("Pinging"),
            ),
            ServerPingState::NotSet => (
                div().child(Icon::new(IconName::Globe)),
                SharedString::from("Not Set"),
            ),
        };
        div()
            .h_flex()
            .items_center()
            .justify_center()
            .gap_1()
            .child(icon)
            .child(
                Button::new("ping-btn")
                    .label(text)
                    .ghost()
                    .xsmall()
                    .p_0()
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _window, cx| {
                        if matches!(this.state, ServerPingState::Pinging) {
                            return;
                        }
                        this.ping(cx);
                    })),
            )
    }
}
