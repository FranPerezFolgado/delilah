//! `delilah attach`: puts this terminal into the agent's live terminal through the
//! daemon's WebSocket. Detaching leaves the agent running.

use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::Duration;

use crossterm::terminal;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

/// Ctrl-], as in telnet: unlikely to clash with what agents use.
const DETACH: u8 = 0x1d;

type Socket = WebSocket<MaybeTlsStream<std::net::TcpStream>>;

pub fn attach(base: &str, key: &str) -> Result<(), String> {
    let url = format!(
        "{}/api/v1/tasks/{key}/terminal",
        base.replacen("http", "ws", 1)
    );
    let (mut ws, _) = tungstenite::connect(&url).map_err(|e| handshake_message(key, &e))?;
    // Short reads let one thread both print output and forward keystrokes.
    if let MaybeTlsStream::Plain(stream) = ws.get_mut() {
        stream
            .set_read_timeout(Some(Duration::from_millis(20)))
            .map_err(|e| e.to_string())?;
    }

    eprintln!("Attached to {key}. Ctrl-] detaches; the agent keeps running.\r");
    let ended = {
        let _raw = RawMode::enable()?;
        pump(&mut ws)?
    };
    if ended {
        eprintln!("\r\nThe agent of {key} exited.");
    } else {
        let _ = ws.close(None);
        eprintln!("\r\nDetached from {key}. Run `delilah attach {key}` to come back.");
    }
    Ok(())
}

/// What to tell the user when the WebSocket handshake fails: a 409 (chat-mode task)
/// carries the API's own message, which is printed as it is.
fn handshake_message(key: &str, e: &tungstenite::Error) -> String {
    if let tungstenite::Error::Http(res) = e
        && res.status().as_u16() == 409
    {
        let body = res.body().as_deref().unwrap_or_default();
        let msg = serde_json::from_slice::<serde_json::Value>(body)
            .ok()
            .and_then(|v| v.get("error").and_then(|m| m.as_str()).map(String::from));
        return msg.unwrap_or_else(|| format!("cannot attach to {key}: the daemon answered 409"));
    }
    if e.to_string().contains("404") {
        format!("{key} has no running agent")
    } else {
        format!("cannot attach to {key}: {e}")
    }
}

/// Returns true when the agent exited, false when the user detached.
fn pump(ws: &mut Socket) -> Result<bool, String> {
    let (keys, keystrokes) = mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin();
        let mut buf = [0u8; 1024];
        while let Ok(n) = stdin.read(&mut buf) {
            if n == 0 || keys.send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });

    let mut size = terminal::size().ok();
    send_resize(ws, size)?;
    let mut stdout = std::io::stdout();
    loop {
        match ws.read() {
            Ok(Message::Binary(bytes)) => {
                stdout.write_all(&bytes).map_err(|e| e.to_string())?;
                stdout.flush().map_err(|e| e.to_string())?;
            }
            Ok(Message::Close(_)) => return Ok(true),
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                return Ok(true);
            }
            Err(e) => return Err(e.to_string()),
        }

        while let Ok(input) = keystrokes.try_recv() {
            if let Some(pos) = input.iter().position(|&b| b == DETACH) {
                if pos > 0 {
                    send(ws, Message::Binary(input[..pos].to_vec().into()))?;
                }
                return Ok(false);
            }
            send(ws, Message::Binary(input.into()))?;
        }

        let now = terminal::size().ok();
        if now != size {
            size = now;
            send_resize(ws, size)?;
        }
    }
}

fn send_resize(ws: &mut Socket, size: Option<(u16, u16)>) -> Result<(), String> {
    let Some((cols, rows)) = size else {
        return Ok(());
    };
    let control = format!(r#"{{"type":"resize","cols":{cols},"rows":{rows}}}"#);
    send(ws, Message::Text(control.into()))
}

fn send(ws: &mut Socket, msg: Message) -> Result<(), String> {
    ws.send(msg)
        .map_err(|e| format!("lost the connection: {e}"))
}

/// Raw mode for as long as it lives, restored even on early returns.
struct RawMode;

impl RawMode {
    fn enable() -> Result<Self, String> {
        terminal::enable_raw_mode().map_err(|e| format!("cannot use this terminal: {e}"))?;
        Ok(Self)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn http(status: u16, body: &str) -> tungstenite::Error {
        let res = tungstenite::http::Response::builder()
            .status(status)
            .body(Some(body.as_bytes().to_vec()))
            .unwrap();
        tungstenite::Error::Http(res)
    }

    #[test]
    fn a_409_prints_the_api_message() {
        let e = http(
            409,
            r#"{"error":"K-1 is a chat session; open it in the chat view"}"#,
        );
        assert_eq!(
            handshake_message("K-1", &e),
            "K-1 is a chat session; open it in the chat view"
        );
    }

    #[test]
    fn a_409_without_json_still_reads_well() {
        assert!(handshake_message("K-1", &http(409, "nope")).contains("409"));
    }

    #[test]
    fn a_404_says_no_running_agent() {
        assert_eq!(
            handshake_message("K-1", &http(404, "")),
            "K-1 has no running agent"
        );
    }
}
