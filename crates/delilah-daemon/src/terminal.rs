//! `GET /api/v1/tasks/{key}/terminal`: the agent's live terminal over WebSocket.
//!
//! Binary frames carry terminal bytes both ways; text frames carry control messages
//! such as `{"type":"resize","cols":120,"rows":40}`. The server first sends the
//! scrollback, then the live output, and closes when the agent exits.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::Response;
use delilah_agents::pty::PtySession;
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;

use crate::api::ApiError;
use crate::sessions::Live;
use crate::state::AppState;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum Control {
    Resize { cols: u16, rows: u16 },
}

/// The primary session of a task (what `delilah attach` opens).
pub async fn task_terminal(
    ws: WebSocketUpgrade,
    State(app): State<AppState>,
    Path(key): Path<String>,
) -> Result<Response, ApiError> {
    let live = app
        .sessions
        .primary(&key)
        .ok_or_else(|| ApiError::not_found(format!("task {key} has no running agent")))?;
    Ok(upgrade(ws, live))
}

pub async fn session_terminal(
    ws: WebSocketUpgrade,
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let live = app
        .sessions
        .get(&id)
        .ok_or_else(|| ApiError::not_found(format!("session {id} is not running")))?;
    Ok(upgrade(ws, live))
}

fn upgrade(ws: WebSocketUpgrade, live: Live) -> Response {
    ws.on_upgrade(move |socket| async move {
        live.viewers.fetch_add(1, Ordering::Relaxed);
        pump(socket, live.pty).await;
        live.viewers.fetch_sub(1, Ordering::Relaxed);
    })
}

async fn pump(mut socket: WebSocket, pty: Arc<PtySession>) {
    let (scrollback, mut output) = pty.attach();
    let mut sizes = pty.watch_size();
    // The size first, so the client draws the scrollback at the size it was made for.
    if socket.send(size_message(pty.size())).await.is_err()
        || socket
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
            changed = sizes.changed() => {
                if changed.is_err() {
                    break;
                }
                let size = *sizes.borrow_and_update();
                if socket.send(size_message(size)).await.is_err() {
                    return;
                }
            }
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

/// `{"type":"size","cols":120,"rows":40}`: the size every viewer should draw at.
fn size_message((cols, rows): (u16, u16)) -> Message {
    Message::Text(format!(r#"{{"type":"size","cols":{cols},"rows":{rows}}}"#).into())
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
    use crate::store_sessions::SessionKind;

    async fn serve() -> (AppState, String) {
        let config = Config {
            home: std::env::temp_dir().join(format!("delilah-term-{}", uuid::Uuid::new_v4())),
            addr: "127.0.0.1:0".parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            claude_bin: "claude".into(),
            agent_program: None,
            ..crate::config::Config::test_defaults()
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
        state.sessions.insert(
            "s-1",
            "PROJ-1",
            SessionKind::Claude,
            true,
            PtySession::spawn(cmd, 80, 24, None).unwrap(),
        );

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
    async fn every_viewer_hears_about_a_resize() {
        let (state, base) = serve().await;
        let mut cmd = CommandBuilder::new("bash");
        cmd.args(["--noprofile", "--norc", "-c", "sleep 5"]);
        state.sessions.insert(
            "s-2",
            "PROJ-2",
            SessionKind::Claude,
            true,
            PtySession::spawn(cmd, 80, 24, None).unwrap(),
        );
        let url = format!("{base}/api/v1/sessions/s-2/terminal");
        let (mut driver, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        let (mut watcher, _) = tokio_tungstenite::connect_async(&url).await.unwrap();

        let first_text = |msg: Option<Result<WsMessage, _>>| match msg {
            Some(Ok(WsMessage::Text(t))) => Some(t.to_string()),
            _ => None,
        };
        assert_eq!(
            first_text(watcher.next().await).as_deref(),
            Some(r#"{"type":"size","cols":80,"rows":24}"#)
        );

        driver
            .send(WsMessage::Text(
                r#"{"type":"resize","cols":132,"rows":40}"#.into(),
            ))
            .await
            .unwrap();
        let heard = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(text) = first_text(watcher.next().await)
                    && text.contains("132")
                {
                    return text;
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(heard, r#"{"type":"size","cols":132,"rows":40}"#);
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
