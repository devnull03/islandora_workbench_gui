use gpui::*;
use gpui_component::{Icon, IconName, Sizable, StyledExt, spinner::Spinner};
use settings::AppSettings;

pub struct ServerPingIndicator {
    state: ServerPingState,
    url: String,
    last_server_label: Option<SharedString>,
    _subscription: Subscription,
}

pub enum ServerPingState {
    NotSet,
    Reachable,
    Unreachable,
    Pinging,
}

impl ServerPingIndicator {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let subscription = cx.observe_global::<AppSettings>(|this, cx| {
            let new_label = AppSettings::get(cx).default_server.clone();
            if new_label == this.last_server_label {
                return;
            }
            this.last_server_label = new_label.clone();
            let url = new_label
                .as_ref()
                .and_then(|label| {
                    AppSettings::get(cx)
                        .server_configs
                        .iter()
                        .find(|s| s.label == *label)
                        .map(|s| s.server_url.to_string())
                })
                .unwrap_or_default();
            this.update_url(url, cx);
        });

        Self {
            state: ServerPingState::NotSet,
            url: "".into(),
            last_server_label: None,
            _subscription: subscription,
        }
    }

    fn update_url(&mut self, url: String, cx: &mut Context<Self>) {
        self.url = url;
        if self.url.is_empty() {
            self.state = ServerPingState::NotSet;
            cx.notify();
            return;
        }
        self.ping(cx);
    }

    fn ping(&mut self, cx: &mut Context<Self>) {
        self.state = ServerPingState::Pinging;
        cx.notify();

        let url = self.url.clone();
        cx.spawn(async |this, cx| {
            let reachable = cx
                .background_executor()
                .spawn(async move { check_http(&url) })
                .await;

            _ = cx.update(|cx| {
                _ = this.update(cx, |indicator, cx| {
                    indicator.state = if reachable {
                        ServerPingState::Reachable
                    } else {
                        ServerPingState::Unreachable
                    };
                    cx.notify();
                });
            });
        })
        .detach();
    }
}

// Any HTTP response (even 4xx/5xx) means the server is up.
// Only a transport/connection error means it's unreachable.
fn check_http(url: &str) -> bool {
    match ureq::head(url)
        .timeout(std::time::Duration::from_secs(5))
        .call()
    {
        Ok(_) => true,
        Err(ureq::Error::Status(_, _)) => true,
        Err(_) => false,
    }
}

impl Render for ServerPingIndicator {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let (icon, text) = match self.state {
            ServerPingState::Reachable => {
                (div().child(Icon::new(IconName::CircleCheck)), "Reachable")
            }
            ServerPingState::Unreachable => {
                (div().child(Icon::new(IconName::CircleX)), "Unreachable")
            }
            ServerPingState::Pinging => (
                div().child(Spinner::new().small().icon(IconName::LoaderCircle)),
                "Pinging",
            ),
            ServerPingState::NotSet => (div().child(Icon::new(IconName::Globe)), "Not Set"),
        };
        div()
            .h_flex()
            .items_center()
            .justify_center()
            .gap_2()
            .child(icon)
            .child(text)
    }
}
