//! What an agent session runs on: a pseudo-terminal or a stream-json pipe (chat mode).
//! An enum, not a trait object: the PTY-only operations (resize, attach, scrollback)
//! get a clear "not available" answer from [`Runtime::as_pty`].

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::claude::{self, Launch};
use crate::pty::PtySession;
use crate::stream::{StreamEvent, StreamSession};
use tokio::sync::mpsc;

/// Columns and rows an agent's terminal starts with.
const PTY_COLS: u16 = 120;
const PTY_ROWS: u16 = 40;
/// Pause between the text and Enter, so the TUI takes the Enter as a key press.
const ENTER_DELAY: Duration = Duration::from_millis(300);

/// On the wire (`SessionView.runtime`): `"pty"` or `"stream"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeKind {
    Pty,
    Stream,
}

#[derive(Clone)]
pub enum Runtime {
    Pty(Arc<PtySession>),
    Stream(Arc<StreamSession>),
}

/// `text` with line ends normalised and every control character dropped but newline
/// and tab: an ESC, Ctrl-C or Ctrl-D, or a paste marker, never reaches the terminal as
/// a key press.
pub fn sanitize_prompt(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|&c| c == '\n' || c == '\t' || !c.is_control())
        .collect()
}

/// The bytes that type `text` into the TUI. Several lines go as a bracketed paste,
/// or each newline would send the message early.
pub fn prompt_bytes(text: &str) -> Vec<u8> {
    let text = sanitize_prompt(text);
    if !text.contains('\n') {
        return text.into_bytes();
    }
    format!("\x1b[200~{text}\x1b[201~").into_bytes()
}

/// Starts a terminal running `cmd` (a shell, say) at the standard size.
pub fn spawn_pty(
    cmd: portable_pty::CommandBuilder,
    log: Option<PathBuf>,
) -> std::io::Result<Runtime> {
    PtySession::spawn(cmd, PTY_COLS, PTY_ROWS, log).map(Runtime::Pty)
}

/// Starts the agent for `launch` on the given runtime.
pub fn spawn(launch: &Launch, kind: RuntimeKind) -> std::io::Result<Runtime> {
    spawn_logged(launch, kind, None)
}

/// As [`spawn`], also appending the terminal output (a PTY) or the stderr (a stream) to
/// the `log` file.
pub fn spawn_logged(
    launch: &Launch,
    kind: RuntimeKind,
    log: Option<PathBuf>,
) -> std::io::Result<Runtime> {
    spawn_with_events(launch, kind, log).map(|(runtime, _)| runtime)
}

/// As [`spawn_logged`], also returning, for a stream runtime, the lossless events
/// (see [`StreamSession::take_events`]), complete from the first line. `None` for a PTY.
pub fn spawn_with_events(
    launch: &Launch,
    kind: RuntimeKind,
    log: Option<PathBuf>,
) -> std::io::Result<(Runtime, Option<mpsc::UnboundedReceiver<StreamEvent>>)> {
    match kind {
        RuntimeKind::Pty => PtySession::spawn(claude::command(launch), PTY_COLS, PTY_ROWS, log)
            .map(|pty| (Runtime::Pty(pty), None)),
        RuntimeKind::Stream => {
            let (session, _live) =
                StreamSession::spawn_logged(claude::stream_command(launch), log)?;
            let events = session.take_events();
            Ok((Runtime::Stream(Arc::new(session)), events))
        }
    }
}

impl From<Arc<PtySession>> for Runtime {
    fn from(pty: Arc<PtySession>) -> Self {
        Runtime::Pty(pty)
    }
}

impl From<Arc<StreamSession>> for Runtime {
    fn from(stream: Arc<StreamSession>) -> Self {
        Runtime::Stream(stream)
    }
}

impl Runtime {
    /// Whether both handles are the same running agent.
    pub fn same(&self, other: &Runtime) -> bool {
        match (self, other) {
            (Runtime::Pty(a), Runtime::Pty(b)) => Arc::ptr_eq(a, b),
            (Runtime::Stream(a), Runtime::Stream(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }

    /// Types `text` as the user would and sends it: the text (a bracketed paste when
    /// it has several lines), a pause, then Enter apart, after any paste is closed.
    pub async fn send_prompt(&self, text: &str) -> std::io::Result<()> {
        match self {
            Runtime::Pty(pty) => {
                pty.write(&prompt_bytes(text))?;
                tokio::time::sleep(ENTER_DELAY).await;
                pty.write(b"\r")
            }
            Runtime::Stream(stream) => stream.send_user(&sanitize_prompt(text)).await,
        }
    }

    /// Interrupts what the agent is doing) (ESC, or an interrupt request on a stream).
    pub async fn stop(&self) -> std::io::Result<()> {
        match self {
            Runtime::Pty(pty) => pty.write(b"\x1b"),
            Runtime::Stream(stream) => stream.interrupt().await,
        }
    }

    pub fn kill(&self) -> std::io::Result<()> {
        match self {
            Runtime::Pty(pty) => pty.kill(),
            Runtime::Stream(stream) => {
                stream.kill();
                Ok(())
            }
        }
    }

    pub fn has_exited(&self) -> bool {
        match self {
            Runtime::Pty(pty) => pty.has_exited(),
            Runtime::Stream(stream) => stream.has_exited(),
        }
    }

    /// Resolves once the process has exited and all its output has been read.
    pub async fn wait(&self) {
        match self {
            Runtime::Pty(pty) => pty.wait().await,
            Runtime::Stream(stream) => {
                stream.wait().await;
            }
        }
    }

    /// The terminal, for attach, resize and the trust dialog; `None` when the agent
    /// has none.
    pub fn as_pty(&self) -> Option<&Arc<PtySession>> {
        match self {
            Runtime::Pty(pty) => Some(pty),
            Runtime::Stream(_) => None,
        }
    }

    /// The stream session, to subscribe to its events and answer permissions; `None`
    /// for a terminal.
    pub fn as_stream(&self) -> Option<&Arc<StreamSession>> {
        match self {
            Runtime::Stream(stream) => Some(stream),
            Runtime::Pty(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use portable_pty::CommandBuilder;

    use super::*;

    #[test]
    fn prompt_bytes_drop_control_characters_in_single_and_multi_line_text() {
        assert_eq!(prompt_bytes("a\x1bb\x03\x04c"), b"abc");
        assert_eq!(prompt_bytes("x\x1b[200~y"), b"x[200~y");
        assert_eq!(prompt_bytes("a\x1b\nb"), b"\x1b[200~a\nb\x1b[201~".to_vec());
    }

    #[test]
    fn control_characters_in_one_line_are_dropped_too() {
        assert_eq!(prompt_bytes("a\x1b[201~b\x07"), b"a[201~b");
    }

    #[test]
    fn several_lines_go_as_one_paste() {
        assert_eq!(
            prompt_bytes("uno\r\ndos\nthree"),
            b"\x1b[200~uno\ndos\nthree\x1b[201~"
        );
    }

    #[test]
    fn a_lone_carriage_return_is_a_new_line_too() {
        assert_eq!(prompt_bytes("a\rb"), b"\x1b[200~a\nb\x1b[201~");
    }

    #[test]
    fn a_trailing_new_line_stays_inside_the_paste() {
        assert_eq!(prompt_bytes("a\n"), b"\x1b[200~a\n\x1b[201~");
    }

    #[test]
    fn a_paste_marker_in_the_text_cannot_end_the_paste() {
        assert_eq!(
            prompt_bytes("a\x1b[201~\rb"),
            b"\x1b[200~a[201~\nb\x1b[201~"
        );
        // Removing the marker must not leave another one behind.
        assert_eq!(
            prompt_bytes("a\x1b[20\x1b[201~1~\nb"),
            b"\x1b[200~a[20[201~1~\nb\x1b[201~"
        );
        assert_eq!(prompt_bytes("a\tb\x07\nc"), b"\x1b[200~a\tb\nc\x1b[201~");
    }

    /// A process that puts the terminal in raw mode, saves `n` bytes of input to a
    /// file and exits.
    fn capture(n: usize) -> (Runtime, PathBuf) {
        let dir = std::env::temp_dir().join(format!("delilah-rt-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("input");
        let mut cmd = CommandBuilder::new("bash");
        cmd.args([
            "--noprofile",
            "--norc",
            "-c",
            &format!("stty raw -echo; head -c {n} > {}", out.display()),
        ]);
        let pty = PtySession::spawn(cmd, 80, 24, None).unwrap();
        (Runtime::Pty(pty), out)
    }

    async fn finished(rt: &Runtime, out: &PathBuf) -> Vec<u8> {
        tokio::time::timeout(Duration::from_secs(5), rt.wait())
            .await
            .unwrap();
        let bytes = std::fs::read(out).unwrap();
        std::fs::remove_dir_all(out.parent().unwrap()).unwrap();
        bytes
    }

    /// Lets `stty raw` take effect before input arrives.
    async fn settle() {
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    #[tokio::test]
    async fn send_prompt_writes_the_text_then_enter() {
        let (rt, out) = capture(5);
        settle().await;
        rt.send_prompt("hola").await.unwrap();
        assert_eq!(finished(&rt, &out).await, b"hola\r");
    }

    #[tokio::test]
    async fn send_prompt_pastes_several_lines_then_enter() {
        let text = "a\nb";
        let want = [prompt_bytes(text), b"\r".to_vec()].concat();
        let (rt, out) = capture(want.len());
        settle().await;
        rt.send_prompt(text).await.unwrap();
        assert_eq!(finished(&rt, &out).await, want);
    }

    #[tokio::test]
    async fn stop_writes_escape() {
        let (rt, out) = capture(1);
        settle().await;
        rt.stop().await.unwrap();
        assert_eq!(finished(&rt, &out).await, b"\x1b");
    }

    #[tokio::test]
    async fn kill_wait_and_has_exited_delegate() {
        let pty = PtySession::spawn(
            {
                let mut cmd = CommandBuilder::new("sleep");
                cmd.arg("30");
                cmd
            },
            80,
            24,
            None,
        )
        .unwrap();
        let rt = Runtime::Pty(pty);
        assert!(!rt.has_exited());
        rt.kill().unwrap();
        tokio::time::timeout(Duration::from_secs(5), rt.wait())
            .await
            .unwrap();
        assert!(rt.has_exited());
    }

    #[tokio::test]
    async fn as_pty_is_some() {
        let pty = PtySession::spawn(CommandBuilder::new("true"), 80, 24, None).unwrap();
        let rt = Runtime::Pty(Arc::clone(&pty));
        assert!(Arc::ptr_eq(rt.as_pty().unwrap(), &pty));
        rt.wait().await;
    }

    fn stream_launch<'a>(settings: &'a std::path::Path, ws: &'a std::path::Path) -> Launch<'a> {
        static FAKE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        let program = FAKE.get_or_init(|| {
            PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/fake-claude-stream.sh"
            ))
        });
        Launch {
            workspace: ws,
            session_id: "abc",
            settings,
            prompt: None,
            resume: false,
            extra_dirs: &[],
            permission: claude::Permission::Default,
            plugin_dir: None,
            append_system_prompt: None,
            remote_control: None,
            program: Some(program),
        }
    }

    async fn next(rx: &mut mpsc::UnboundedReceiver<StreamEvent>) -> StreamEvent {
        tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .unwrap()
            .unwrap()
    }

    #[tokio::test]
    async fn stream_runtime_spawns_prompts_stops_and_kills() {
        let tmp = std::env::temp_dir();
        let launch = stream_launch(&tmp, &tmp);
        let (rt, rx) = spawn_with_events(&launch, RuntimeKind::Stream, None).unwrap();
        let mut rx = rx.unwrap();
        assert!(rt.as_pty().is_none());
        assert!(rt.as_stream().is_some());
        assert!(rt.same(&rt.clone()));
        let (other, _) = spawn_with_events(&launch, RuntimeKind::Stream, None).unwrap();
        assert!(!rt.same(&other));
        other.kill().unwrap();

        rt.send_prompt("hola\x1b").await.unwrap();
        // The agent says hello (init) once it has read the first message.
        assert!(matches!(next(&mut rx).await, StreamEvent::Init { .. }));
        loop {
            if matches!(next(&mut rx).await, StreamEvent::TurnResult { .. }) {
                break;
            }
        }

        rt.send_prompt("WAIT").await.unwrap();
        // Text is not on the lossless channel: the turn's init says it started.
        assert!(matches!(next(&mut rx).await, StreamEvent::Init { .. }));
        rt.stop().await.unwrap();
        loop {
            if matches!(next(&mut rx).await, StreamEvent::TurnResult { .. }) {
                break;
            }
        }
        assert!(!rt.has_exited());

        rt.kill().unwrap();
        tokio::time::timeout(Duration::from_secs(5), rt.wait())
            .await
            .unwrap();
        assert!(rt.has_exited());
    }

    #[tokio::test]
    async fn pty_spawn_has_no_initial_receiver() {
        let mut cmd = CommandBuilder::new("true");
        cmd.arg("x");
        let pty = PtySession::spawn(cmd, 80, 24, None).unwrap();
        let rt = Runtime::from(pty);
        assert!(rt.as_stream().is_none());
        rt.wait().await;
    }
}
