//! `GET /api/v1/tasks/{key}/terminal`: the agent's live terminal over WebSocket.
//!
//! Binary frames carry terminal bytes both ways; text frames carry control messages
//! such as `{"type":"resize","cols":120,"rows":40}`. The server first sends the
//! scrollback, then the live output, and closes when the agent exits.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::Response;
use delilah_agents::pty::PtySession;
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;

use crate::api::ApiError;
use crate::state::AppState;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum Control {
    Resize { cols: u16, rows: u16 },
}

pub async fn terminal(
    ws: WebSocketUpgrade,
    State(app): State<AppState>,
    Path(key): Path<String>,
) -> Result<Response, ApiError> {
    let live = app
        .sessions
        .get(&key)
        .ok_or_else(|| ApiError::not_found(format!("task {key} has no running agent")))?;
    Ok(ws.on_upgrade(move |socket| pump(socket, live.pty)))
}

async fn pump(mut socket: WebSocket, pty: Arc<PtySession>) {
    let (scrollback, mut output) = pty.attach();
    if socket
        .send(Message::Binary(scrollback.into()))
        .await
        .is_err()
    {
        return;
    }
    loop {
        tokio::select! {
            chunk = output.recv() => match chunk {
                Ok(chunk) => {
                    if socket.send(Message::Binary(chunk.to_vec().into())).await.is_err() {
                        return;
                    }
                }
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => break,
            },
            msg = socket.recv() => match msg {
                Some(Ok(Message::Binary(bytes))) => {
                    let _ = pty.write(&bytes);
                }
                Some(Ok(Message::Text(text))) => match serde_json::from_str(text.as_str()) {
                    Ok(Control::Resize { cols, rows }) => {
                        let _ = pty.resize(cols, rows);
                    }
                    Err(e) => tracing::debug!("ignored control message: {e}"),
                },
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return,
                Some(Ok(_)) => {}
            },
            _ = pty.wait() => {
                // Flush what the agent printed right before exiting.
                while let Ok(chunk) = output.try_recv() {
                    let _ = socket.send(Message::Binary(chunk.to_vec().into())).await;
                }
                break;
            }
        }
    }
    let _ = socket.send(Message::Close(None)).await;
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use delilah_agents::pty::PtySession;
    use futures_util::{SinkExt, StreamExt};
    use portable_pty::CommandBuilder;
    use tokio_tungstenite::tungstenite::Message as WsMessage;

    use crate::config::Config;
    use crate::state::AppState;
    use crate::store::Store;

    async fn serve() -> (AppState, String) {
        let config = Config {
            home: std::env::temp_dir().join(format!("delilah-term-{}", uuid::Uuid::new_v4())),
            addr: "127.0.0.1:0".parse().unwrap(),
            allow_file_protocol: false,
        };
        let state = AppState::new(Store::in_memory().await.unwrap(), config);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = crate::api::router(state.clone());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (state, format!("ws://{addr}"))
    }

    async fn next_text(
        ws: &mut (
                 impl StreamExt<Item = Result<WsMessage, tokio_tungstenite::tungstenite::Error>> + Unpin
             ),
        seen: &mut String,
        needle: &str,
    ) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !seen.contains(needle) {
                match ws.next().await {
                    Some(Ok(WsMessage::Binary(b))) => seen.push_str(&String::from_utf8_lossy(&b)),
                    Some(Ok(_)) => {}
                    other => panic!("socket ended early: {other:?}"),
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("never saw {needle:?} in {seen:?}"));
    }

    #[tokio::test]
    async fn streams_the_terminal_and_forwards_keystrokes() {
        let (state, base) = serve().await;
        let mut cmd = CommandBuilder::new("bash");
        cmd.args([
            "--noprofile",
            "--norc",
            "-c",
            "echo ready; read line; echo got:$line",
        ]);
        state
            .sessions
            .insert("PROJ-1", PtySession::spawn(cmd, 80, 24, None).unwrap());

        let (mut ws, _) =
            tokio_tungstenite::connect_async(format!("{base}/api/v1/tasks/PROJ-1/terminal"))
                .await
                .unwrap();
        let mut seen = String::new();
        next_text(&mut ws, &mut seen, "ready").await;

        ws.send(WsMessage::Text(
            r#"{"type":"resize","cols":100,"rows":30}"#.into(),
        ))
        .await
        .unwrap();
        ws.send(WsMessage::Binary(b"hola\r".to_vec().into()))
            .await
            .unwrap();
        next_text(&mut ws, &mut seen, "got:hola").await;
    }

    #[tokio::test]
    async fn tasks_without_an_agent_have_no_terminal() {
        let (_, base) = serve().await;
        let err =
            tokio_tungstenite::connect_async(format!("{base}/api/v1/tasks/PROJ-404/terminal"))
                .await
                .unwrap_err();
        assert!(err.to_string().contains("404"), "{err}");
    }
}
