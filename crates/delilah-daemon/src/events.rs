//! `GET /api/v1/events`: tells clients, over a WebSocket, what changed, so they
//! fetch it again at once instead of polling. Messages are just topics, like
//! `{"type":"tasks"}`; the data stays in the regular endpoints, so a client that
//! missed something only has to fetch again (and a lagging one gets `resync`).

use std::time::Duration;

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use serde::Serialize;
use tokio::sync::broadcast;

use crate::state::AppState;

/// What changed. Each topic maps to an endpoint the client fetches again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Topic {
    /// `/tasks`: a task was created or changed status or phase.
    Tasks,
    /// `/sessions`: a session opened, closed, started, stopped or changed state.
    Sessions,
    /// `/inbox`: something now waits on the user, or no longer does.
    Inbox,
    /// `/specs`: a task's spec moved on.
    Specs,
    /// `/usage`: the usage windows changed.
    Usage,
    /// `/projects`
    Projects,
    /// The client missed messages: fetch everything.
    Resync,
}

/// The daemon's change feed. Cheap to clone; sending with nobody listening is fine.
#[derive(Clone)]
pub struct Events(broadcast::Sender<Topic>);

impl Default for Events {
    fn default() -> Self {
        Self(broadcast::channel(256).0)
    }
}

impl Events {
    pub fn emit(&self, topic: Topic) {
        let _ = self.0.send(topic);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Topic> {
        self.0.subscribe()
    }

    /// Whether a client (the app) follows the feed right now.
    pub fn listening(&self) -> bool {
        self.0.receiver_count() > 0
    }
}

/// Keeps idle connections from being dropped and notices dead ones.
const PING: Duration = Duration::from_secs(25);

pub async fn stream(State(app): State<AppState>, ws: WebSocketUpgrade) -> Response {
    let rx = app.events.subscribe();
    ws.on_upgrade(move |socket| forward(socket, rx))
}

async fn forward(mut socket: WebSocket, mut rx: broadcast::Receiver<Topic>) {
    let mut ping = tokio::time::interval(PING);
    ping.tick().await;
    loop {
        let topic = tokio::select! {
            received = rx.recv() => match received {
                Ok(topic) => topic,
                Err(broadcast::error::RecvError::Lagged(_)) => Topic::Resync,
                Err(broadcast::error::RecvError::Closed) => return,
            },
            _ = ping.tick() => {
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                    return;
                }
                continue;
            }
            incoming = socket.recv() => match incoming {
                // Clients only listen; anything but a close is ignored.
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                Some(Ok(_)) => continue,
            },
        };
        let text = serde_json::to_string(&topic).expect("a topic serializes");
        if socket.send(Message::Text(text.into())).await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::Message as WsMessage;

    use super::*;
    use crate::config::Config;
    use crate::store::Store;

    #[tokio::test]
    async fn clients_hear_about_a_new_task() {
        let config = Config {
            home: std::env::temp_dir().join(format!("delilah-events-{}", uuid::Uuid::new_v4())),
            addr: "127.0.0.1:0".parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
        };
        let state = AppState::new(Store::in_memory().await.unwrap(), config);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(axum::serve(listener, crate::api::router(state.clone())).into_future());

        let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/api/v1/events"))
            .await
            .unwrap();
        let created = post_task(addr, r#"{"key":"PROJ-9","title":"t"}"#).await;
        assert!(created.starts_with("HTTP/1.1 201"), "{created}");

        let msg = tokio::time::timeout(Duration::from_secs(3), ws.next())
            .await
            .expect("no event within 3 s")
            .unwrap()
            .unwrap();
        assert_eq!(msg, WsMessage::Text(r#"{"type":"tasks"}"#.into()));
    }

    /// A bare HTTP POST to /api/v1/tasks, to keep test dependencies down.
    async fn post_task(addr: std::net::SocketAddr, body: &str) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
        let req = format!(
            "POST /api/v1/tasks HTTP/1.1\r\nHost: 127.0.0.1\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        s.write_all(req.as_bytes()).await.unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).await.unwrap();
        out
    }
}
