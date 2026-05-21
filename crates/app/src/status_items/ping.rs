use gpui::*;

pub struct ServerPingIndicator {
    state: ServerPingState,
    url: String,
}

pub enum ServerPingState {
    NotSet,
    Reachable,
    Unreachable,
    Pinging
}

impl ServerPingIndicator {
    pub fn new() -> Self {
        Self {
            state: ServerPingState::NotSet,
            url: "".into(),
        }
    }

    pub fn update_url(&mut self, url: String) {
        self.url = url;
        if self.url.is_empty() {
            self.state = ServerPingState::NotSet;
            return;
        }
        self.ping();
    }

    pub fn ping(&mut self) {
        self.state = ServerPingState::Pinging;

        todo!();
        
        let result = true;

        self.state = if result {
            ServerPingState::Reachable
        } else {
            ServerPingState::Unreachable
        };
    }
}

impl Render for ServerPingIndicator {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        match self.state {
            ServerPingState::Reachable => {
                div()
            }
            ServerPingState::Unreachable => {
                div()
            }
            ServerPingState::Pinging => {
                div()
            }
            ServerPingState::NotSet => {
                div()
            }
        }
    }
}
