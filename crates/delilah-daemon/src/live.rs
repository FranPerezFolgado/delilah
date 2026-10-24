//! `GET /api/v1/sessions/{id}/live`: the text a chat-mode agent is writing, as it comes.
//!
//! The server sends `{"message": "<msg id>", "text": "<delta>"}` for each piece and
//! `{"message": "<msg id>", "done": true}` when the message is complete. Nothing is
//! replayed: a client that connects late sees what comes after. A terminal session
//! closes at once with code 4409.

use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::Response;
use delilah_agents::stream::StreamEvent;
use serde_json::json;
use tokio::sync::broadcast::{self, error::RecvError};

use crate::api::ApiError;
use crate::state::AppState;

/// Close code for a session that is not in chat mode.
const NOT_IN_CHAT_MODE: u16 = 4409;

pub async fn session_live(
    ws: WebSocketUpgrade,
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let live = app
        .sessions
        .get(&id)
        .ok_or_else(|| ApiError::not_found(format!("session {id} is not running")))?;
    // Subscribed before the upgrade finishes, so nothing said after the request is lost.
    let events = live.runtime.as_stream().map(|s| s.subscribe());
    Ok(ws.on_upgrade(move |mut socket| async move {
        match events {
            Some(events) => relay(socket, events).await,
            None => {
                let _ = socket
                    .send(Message::Close(Some(CloseFrame {
                        code: NOT_IN_CHAT_MODE,
                        reason: "not in chat mode".into(),
                    })))
                    .await;
            }
        }
    }))
}

async fn relay(mut socket: WebSocket, mut events: broadcast::Receiver<StreamEvent>) {
    loop {
        tokio::select! {
            event = events.recv() => {
                let text = match event {
                    Ok(StreamEvent::TextDelta { message, text }) => {
                        json!({ "message": message, "text": text })
                    }
                    Ok(StreamEvent::MessageDone { message, .. }) => {
                        json!({ "message": message, "done": true })
                    }
                    Ok(StreamEvent::Exited { .. }) | Err(RecvError::Closed) => break,
                    Ok(_) | Err(RecvError::Lagged(_)) => continue,
                };
                if socket.send(Message::Text(text.to_string().into())).await.is_err() {
                    return;
                }
            }
            msg = socket.recv() => match msg {
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return,
                Some(Ok(_)) => {}
            },
        }
    }
    let _ = socket.send(Message::Close(None)).await;
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use delilah_agents::stream::StreamSession;
    use futures_util::StreamExt;
    use portable_pty::CommandBuilder;
    use tokio_tungstenite::tungstenite::Message as WsMessage;

    use crate::config::Config;
    use crate::state::AppState;
    use crate::store::Store;
    use crate::store_sessions::SessionKind;

    async fn serve() -> (AppState, String) {
        let config = Config {
            home: std::env::temp_dir().join(format!("delilah-live-{}", uuid::Uuid::new_v4())),
            addr: "127.0.0.1:0".parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            claude_bin: "claude".into(),
            agent_program: None,
            claude_projects: Config::scratch_projects(),
        };
        let state = AppState::new(Store::in_memory().await.unwrap(), config);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = crate::api::router(state.clone());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (state, format!("ws://{addr}"))
    }

    /// A chat-mode session "s-1" of PROJ-1 running the fake stream agent.
    fn insert_stream(state: &AppState) -> std::sync::Arc<StreamSession> {
        let script = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../delilah-agents/tests/fixtures/fake-claude-stream.sh"
        );
        let (session, _rx) = StreamSession::spawn(tokio::process::Command::new(script)).unwrap();
        let session = std::sync::Arc::new(session);
        state
            .sessions
            .insert("s-1", "PROJ-1", SessionKind::Claude, true, session.clone());
        session
    }

    async fn next_json(
        ws: &mut (
                 impl StreamExt<Item = Result<WsMessage, tokio_tungstenite::tungstenite::Error>> + Unpin
             ),
    ) -> serde_json::Value {
        let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("a message")
            .unwrap()
            .unwrap();
        match msg {
            WsMessage::Text(t) => serde_json::from_str(t.as_str()).unwrap(),
            other => panic!("not a text message: {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_stream_session_relays_deltas_and_the_end_of_the_message() {
        let (state, base) = serve().await;
        let session = insert_stream(&state);
        let (mut ws, _) =
            tokio_tungstenite::connect_async(format!("{base}/api/v1/sessions/s-1/live"))
                .await
                .unwrap();
        session.send_user("hola").await.unwrap();
        let first = next_json(&mut ws).await;
        let second = next_json(&mut ws).await;
        let done = next_json(&mut ws).await;
        assert_eq!(first["text"], "Hel");
        assert_eq!(second["text"], "lo");
        assert_eq!(first["message"], "msg_1");
        assert_eq!(second["message"], "msg_1");
        assert_eq!(
            done,
            serde_json::json!({ "message": "msg_1", "done": true })
        );
    }

    #[tokio::test]
    async fn nothing_is_replayed_to_a_late_client() {
        let (state, base) = serve().await;
        let session = insert_stream(&state);
        let mut events = session.subscribe();
        session.send_user("hola").await.unwrap();
        // The whole turn has been said before the client arrives.
        while !matches!(
            tokio::time::timeout(Duration::from_secs(5), events.recv())
                .await
                .unwrap()
                .unwrap(),
            delilah_agents::stream::StreamEvent::TurnResult { .. }
        ) {}
        let (mut ws, _) =
            tokio_tungstenite::connect_async(format!("{base}/api/v1/sessions/s-1/live"))
                .await
                .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(400), ws.next())
                .await
                .is_err(),
            "no message for a turn that is over"
        );
        // What comes next does arrive, as its own message.
        session.send_user("otra").await.unwrap();
        let next = next_json(&mut ws).await;
        assert_eq!(next["message"], "msg_2");
    }

    #[tokio::test]
    async fn a_terminal_session_closes_with_4409() {
        let (state, base) = serve().await;
        let mut cmd = CommandBuilder::new("bash");
        cmd.args(["--noprofile", "--norc", "-c", "sleep 5"]);
        state.sessions.insert(
            "s-1",
            "PROJ-1",
            SessionKind::Claude,
            true,
            delilah_agents::pty::PtySession::spawn(cmd, 80, 24, None).unwrap(),
        );
        let (mut ws, _) =
            tokio_tungstenite::connect_async(format!("{base}/api/v1/sessions/s-1/live"))
                .await
                .unwrap();
        let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let WsMessage::Close(Some(frame)) = msg else {
            panic!("expected a close, got {msg:?}");
        };
        assert_eq!(u16::from(frame.code), 4409);
        assert_eq!(frame.reason.as_str(), "not in chat mode");
    }

    #[tokio::test]
    async fn an_unknown_session_is_a_404() {
        let (_, base) = serve().await;
        let err = tokio_tungstenite::connect_async(format!("{base}/api/v1/sessions/nope/live"))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("404"), "{err}");
    }
}
