//! A task's preview: the project's preview command, run by the daemon in the task's
//! workspace. It prints the URL it serves; the first one it prints is the preview's.
//! Memory only: a daemon restart does not adopt it again. The HTTP side is in
//! `preview_api.rs`.

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use delilah_core::TaskKey;
use serde::Serialize;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::oneshot;

use crate::api::ApiError;
use crate::events::{Events, Topic};
use crate::state::AppState;
use crate::store::StoreError;

/// Lines of output kept, the last ones.
const KEEP_LINES: usize = 200;
/// Bytes kept of a line; the rest of a longer one is dropped.
const MAX_LINE: usize = 4096;
/// Lines of output quoted in an error.
const TAIL_LINES: usize = 20;
/// How long the command has to print its URL.
pub const URL_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// Between asking a process group to end and forcing it.
const GRACE: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PreviewState {
    Starting,
    Running,
    Failed,
}

/// What the API shows of a preview.
#[derive(Debug, Clone, Serialize)]
pub struct PreviewView {
    pub state: PreviewState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Why it failed, with the last lines of its output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum StartError {
    /// One is already starting or running for this task.
    Running,
    Spawn(String),
}

struct Entry {
    id: u64,
    /// The process group (its leader's pid) while the leader has not been reaped: a
    /// pid that may be reused is never signalled.
    group: Option<u32>,
    view: PreviewView,
    output: VecDeque<String>,
    /// Tells the waiter task the preview was stopped (it also ends the URL timeout).
    cancel: Option<oneshot::Sender<()>>,
}

pub struct Previews {
    entries: Mutex<HashMap<String, Entry>>,
    events: Events,
    url_timeout: Duration,
    next_id: AtomicU64,
    /// Waiter tasks alive, to check none lingers.
    #[cfg(test)]
    waiters: Arc<std::sync::atomic::AtomicUsize>,
}

impl Previews {
    pub fn new(events: Events) -> Self {
        Self::with_timeout(events, URL_TIMEOUT)
    }

    pub fn with_timeout(events: Events, url_timeout: Duration) -> Self {
        Self {
            entries: Mutex::default(),
            events,
            url_timeout,
            next_id: AtomicU64::new(1),
            #[cfg(test)]
            waiters: Arc::default(),
        }
    }

    pub fn view(&self, task: &str) -> Option<PreviewView> {
        self.lock().get(task).map(|e| e.view.clone())
    }

    #[cfg(test)]
    fn output(&self, task: &str) -> Vec<String> {
        self.lock()
            .get(task)
            .map(|e| e.output.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Entry>> {
        self.entries.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Starts `command` with `zsh -lc` in `dir`. It is `starting` until it prints a URL.
    pub fn start(
        self: &Arc<Self>,
        task: &str,
        dir: &Path,
        command: &str,
    ) -> Result<PreviewView, StartError> {
        let mut entries = self.lock();
        if let Some(e) = entries.get(task)
            && matches!(e.view.state, PreviewState::Starting | PreviewState::Running)
        {
            return Err(StartError::Running);
        }
        let mut child = Command::new("/bin/zsh")
            .arg("-lc")
            .arg(command)
            .current_dir(dir)
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(false)
            .spawn()
            .map_err(|e| StartError::Spawn(e.to_string()))?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let view = PreviewView {
            state: PreviewState::Starting,
            url: None,
            error: None,
            started_at: Utc::now(),
        };
        let (cancel, cancelled) = oneshot::channel();
        // A replaced failed entry is dropped here; its group was already dealt with.
        entries.insert(
            task.to_string(),
            Entry {
                id,
                group: child.id().filter(|p| *p != 0),
                view: view.clone(),
                output: VecDeque::new(),
                cancel: Some(cancel),
            },
        );
        drop(entries);

        let readers = [
            child.stdout.take().map(|s| self.read(task, id, s)),
            child.stderr.take().map(|s| self.read(task, id, s)),
        ];
        let this = self.clone();
        let key = task.to_string();
        #[cfg(test)]
        let alive = {
            self.waiters.fetch_add(1, Ordering::SeqCst);
            WaiterGuard(self.waiters.clone())
        };
        tokio::spawn(async move {
            #[cfg(test)]
            let _alive = alive;
            this.watch(&key, id, child, cancelled, readers).await;
        });
        self.events.emit(Topic::Tasks);
        Ok(view)
    }

    /// Owns the child until it is reaped: waits for it, for the URL timeout and for a
    /// stop, whichever comes first.
    async fn watch(
        &self,
        task: &str,
        id: u64,
        mut child: Child,
        mut cancelled: oneshot::Receiver<()>,
        readers: [Option<tokio::task::JoinHandle<()>>; 2],
    ) {
        let timeout = tokio::time::sleep(self.url_timeout);
        tokio::pin!(timeout);
        let mut armed = true;
        let status = loop {
            tokio::select! {
                status = child.wait() => break Some(status),
                _ = &mut cancelled => break None,
                _ = &mut timeout, if armed => {
                    armed = false;
                    let mins = self.url_timeout.as_secs_f64() / 60.0;
                    let why = format!("the preview command printed no URL in {mins:.1} minutes");
                    if self.fail(task, id, &why, true) {
                        break None;
                    }
                }
            }
        };
        match status {
            Some(status) => {
                // The leader is gone: end what it left behind (`npm run dev & exit 1`)
                // once, and forget the group.
                if let Some(pid) = self.forget(task, id) {
                    signal_group(pid, "TERM");
                }
                // The last lines may still be in the pipe.
                for reader in readers.into_iter().flatten() {
                    let _ = tokio::time::timeout(Duration::from_secs(1), reader).await;
                }
                let status = status.map(|s| s.to_string()).unwrap_or_default();
                self.fail(
                    task,
                    id,
                    &format!("the preview command exited ({status})"),
                    false,
                );
            }
            None => {
                // Stopped or timed out: whoever decided sent the TERM; here the KILL
                // comes if the leader is still unreaped after the grace period.
                if tokio::time::timeout(GRACE, child.wait()).await.is_err() {
                    if let Some(pid) = child.id() {
                        signal_group(pid, "KILL");
                    }
                    let _ = child.wait().await;
                }
                self.forget(task, id);
            }
        }
    }

    /// The leader was reaped: its group is not ours to signal any more. Returns the
    /// group, if it was still known.
    fn forget(&self, task: &str, id: u64) -> Option<u32> {
        let mut entries = self.lock();
        let e = entries.get_mut(task).filter(|e| e.id == id)?;
        e.cancel = None;
        e.group.take()
    }

    fn read<R>(self: &Arc<Self>, task: &str, id: u64, stream: R) -> tokio::task::JoinHandle<()>
    where
        R: AsyncRead + Unpin + Send + 'static,
    {
        let this = self.clone();
        let key = task.to_string();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stream);
            let mut buf = Vec::new();
            while let Ok(true) = read_capped_line(&mut reader, &mut buf, MAX_LINE).await {
                this.line(&key, id, String::from_utf8_lossy(&buf).into_owned());
            }
        })
    }

    fn line(&self, task: &str, id: u64, line: String) {
        let mut entries = self.lock();
        let Some(e) = entries.get_mut(task).filter(|e| e.id == id) else {
            return;
        };
        let url = (e.view.state == PreviewState::Starting)
            .then(|| find_url(&line))
            .flatten();
        e.output.push_back(line);
        if e.output.len() > KEEP_LINES {
            e.output.pop_front();
        }
        if let Some(url) = url {
            e.view.state = PreviewState::Running;
            e.view.url = Some(url);
            drop(entries);
            self.events.emit(Topic::Tasks);
        }
    }

    /// Marks it failed (if it was starting or running), and with `term` asks its group
    /// to end. Whether it was.
    fn fail(&self, task: &str, id: u64, why: &str, term: bool) -> bool {
        let mut entries = self.lock();
        let Some(e) = entries.get_mut(task).filter(|e| e.id == id) else {
            return false;
        };
        if !matches!(e.view.state, PreviewState::Starting | PreviewState::Running) {
            return false;
        }
        let from = e.output.len().saturating_sub(TAIL_LINES);
        let tail = e.output.iter().skip(from).cloned().collect::<Vec<_>>();
        e.view.state = PreviewState::Failed;
        e.view.error = Some(if tail.is_empty() {
            why.to_string()
        } else {
            format!("{why}\n{}", tail.join("\n"))
        });
        let group = e.group.filter(|_| term);
        drop(entries);
        if let Some(pid) = group {
            signal_group(pid, "TERM");
        }
        self.events.emit(Topic::Tasks);
        true
    }

    /// Ends the task's preview: its whole process group gets SIGTERM, and its waiter
    /// SIGKILL if the leader is still there after a few seconds. The preview is gone
    /// afterwards.
    pub fn stop(&self, task: &str) {
        let entry = self.lock().remove(task);
        if let Some(mut e) = entry {
            // A failed one may still have a group (its timeout is ending it).
            if let Some(pid) = e.group {
                signal_group(pid, "TERM");
            }
            if let Some(cancel) = e.cancel.take() {
                let _ = cancel.send(());
            }
            self.events.emit(Topic::Tasks);
        }
    }

    /// For the daemon's shutdown: nothing is left running behind it.
    pub async fn stop_all(&self) {
        let pids: Vec<(String, u64, u32)> = self
            .lock()
            .iter()
            .filter_map(|(k, e)| e.group.map(|g| (k.clone(), e.id, g)))
            .collect();
        for (_, _, pid) in &pids {
            signal_group(*pid, "TERM");
        }
        if !pids.is_empty() {
            // Deliberately short: the shutdown must not be slow.
            tokio::time::sleep(Duration::from_millis(500)).await;
            let entries = self.lock();
            for (key, id, pid) in pids {
                // Only a leader not reaped yet: its pid is still its group's.
                if entries
                    .get(&key)
                    .is_some_and(|e| e.id == id && e.group == Some(pid))
                {
                    signal_group(pid, "KILL");
                }
            }
        }
        self.lock().clear();
    }
}

#[cfg(test)]
struct WaiterGuard(Arc<std::sync::atomic::AtomicUsize>);

#[cfg(test)]
impl Drop for WaiterGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Starts the preview of a task: its project's preview command, in its workspace.
/// A 409 says why not (`no-preview-command`, `no-workspace`, `running`); a command
/// that cannot be spawned is a 422.
pub(crate) async fn start_for(app: &AppState, key: &TaskKey) -> Result<PreviewView, ApiError> {
    let task = app
        .store
        .get_task(key)
        .await?
        .ok_or(StoreError::NotFound(key.clone()))?;
    let command = match &task.project {
        Some(name) => app.store.project(name).await?.preview_command,
        None => None,
    }
    .ok_or_else(|| {
        ApiError::conflict_reason("this project has no preview command", "no-preview-command")
    })?;
    let (_, workspace) = app.store.workspace(key).await?.ok_or_else(|| {
        ApiError::conflict_reason("the task has no workspace yet", "no-workspace")
    })?;
    app.previews
        .start(key.as_str(), &workspace.path, &command)
        .map_err(|e| match e {
            StartError::Running => {
                ApiError::conflict_reason("the preview is already running", "running")
            }
            StartError::Spawn(why) => ApiError::unprocessable(format!("could not start: {why}")),
        })
}

/// Reads up to the next newline into `buf` (without it, nor a `\r` before it), keeping
/// at most `cap` bytes: the rest of a longer line is dropped as it comes, so an endless
/// line costs no memory. `false` at the end of the stream with nothing read.
async fn read_capped_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    buf: &mut Vec<u8>,
    cap: usize,
) -> std::io::Result<bool> {
    buf.clear();
    let mut any = false;
    loop {
        let chunk = reader.fill_buf().await?;
        if chunk.is_empty() {
            return Ok(any);
        }
        any = true;
        let (used, done) = match chunk.iter().position(|b| *b == b'\n') {
            Some(i) => (i + 1, true),
            None => (chunk.len(), false),
        };
        let room = cap.saturating_sub(buf.len());
        let end = if done { used - 1 } else { used };
        buf.extend_from_slice(&chunk[..end.min(room)]);
        reader.consume(used);
        if done {
            if buf.last() == Some(&b'\r') {
                buf.pop();
            }
            return Ok(true);
        }
    }
}

/// Signals a whole process group (no `unsafe` here, so through `kill(1)`).
fn signal_group(pid: u32, signal: &str) {
    if pid == 0 {
        return;
    }
    let _ = std::process::Command::new("/bin/kill")
        .arg(format!("-{signal}"))
        .arg("--")
        .arg(format!("-{pid}"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// The first `http://` or `https://` URL in a line (`https?://[^\s)'"]+`).
fn find_url(line: &str) -> Option<String> {
    let start = ["http://", "https://"]
        .iter()
        .filter_map(|p| line.find(p))
        .min()?;
    let rest = &line[start..];
    let end = rest
        .find(|c: char| c.is_whitespace() || matches!(c, ')' | '\'' | '"'))
        .unwrap_or(rest.len());
    let url = &rest[..end];
    let scheme = if url.starts_with("https://") { 8 } else { 7 };
    (url.len() > scheme).then(|| url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn previews(timeout_ms: u64) -> Arc<Previews> {
        Arc::new(Previews::with_timeout(
            Events::default(),
            Duration::from_millis(timeout_ms),
        ))
    }

    fn dir() -> std::path::PathBuf {
        std::env::temp_dir()
    }

    async fn until(p: &Previews, task: &str, ok: impl Fn(&PreviewView) -> bool) -> PreviewView {
        for _ in 0..200 {
            if let Some(v) = p.view(task)
                && ok(&v)
            {
                return v;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("preview never got there: {:?}", p.view(task));
    }

    fn running(v: &PreviewView) -> bool {
        v.state == PreviewState::Running
    }

    #[tokio::test]
    async fn the_printed_url_makes_it_running() {
        let p = previews(60_000);
        let v = p
            .start("T-1", &dir(), "echo http://127.0.0.1:9/x; sleep 30")
            .unwrap();
        assert_eq!(v.state, PreviewState::Starting);
        let v = until(&p, "T-1", running).await;
        assert_eq!(v.url.as_deref(), Some("http://127.0.0.1:9/x"));
        p.stop("T-1");
    }

    #[tokio::test]
    async fn the_first_url_wins() {
        let p = previews(60_000);
        p.start(
            "T-1",
            &dir(),
            "echo a; echo http://one; echo http://two; sleep 30",
        )
        .unwrap();
        let v = until(&p, "T-1", running).await;
        assert_eq!(v.url.as_deref(), Some("http://one"));
        p.stop("T-1");
    }

    #[tokio::test]
    async fn exiting_fails_with_the_output() {
        let p = previews(60_000);
        p.start("T-1", &dir(), "echo oops; exit 3").unwrap();
        let v = until(&p, "T-1", |v| v.state == PreviewState::Failed).await;
        assert!(v.error.unwrap().contains("oops"));
    }

    #[tokio::test]
    async fn no_url_in_time_fails() {
        let p = previews(300);
        p.start("T-1", &dir(), "echo hello; sleep 30").unwrap();
        let v = until(&p, "T-1", |v| v.state == PreviewState::Failed).await;
        assert!(v.error.unwrap().contains("hello"));
    }

    #[tokio::test]
    async fn stop_kills_the_whole_group() {
        let p = previews(60_000);
        let pid_file = dir().join(format!("delilah-preview-{}", uuid::Uuid::new_v4()));
        let cmd = format!(
            "sleep 30 & echo $! > {}; echo http://x; wait",
            pid_file.display()
        );
        p.start("T-1", &dir(), &cmd).unwrap();
        until(&p, "T-1", running).await;
        let pid = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .to_string();
        let _ = std::fs::remove_file(&pid_file);
        let alive = || {
            std::process::Command::new("/bin/kill")
                .args(["-0", &pid])
                .stderr(Stdio::null())
                .status()
                .unwrap()
                .success()
        };
        assert!(alive());
        p.stop("T-1");
        assert!(p.view("T-1").is_none());
        for _ in 0..100 {
            if !alive() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("the grandchild is still alive");
    }

    #[tokio::test]
    async fn starting_twice_is_refused_until_it_is_over() {
        let p = previews(60_000);
        p.start("T-1", &dir(), "sleep 30").unwrap();
        assert_eq!(
            p.start("T-1", &dir(), "sleep 30").unwrap_err(),
            StartError::Running
        );
        p.stop("T-1");
        p.start("T-1", &dir(), "echo http://x; sleep 30").unwrap();
        until(&p, "T-1", running).await;
        assert_eq!(
            p.start("T-1", &dir(), "sleep 30").unwrap_err(),
            StartError::Running
        );
        p.stop("T-1");
    }

    #[tokio::test]
    async fn only_the_last_200_lines_stay() {
        let p = previews(60_000);
        p.start("T-1", &dir(), "seq 1 500; sleep 30").unwrap();
        for _ in 0..200 {
            if p.output("T-1").last().is_some_and(|l| l == "500") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let out = p.output("T-1");
        assert_eq!(out.len(), 200);
        assert_eq!(out[0], "301");
        p.stop("T-1");
    }

    #[test]
    fn urls_are_cut_at_quotes_and_parens() {
        assert_eq!(
            find_url("Local: (https://localhost:5173/a) ok").as_deref(),
            Some("https://localhost:5173/a")
        );
        assert_eq!(find_url("nothing here"), None);
    }

    #[tokio::test]
    async fn an_endless_line_is_capped_and_the_url_after_it_found() {
        let p = previews(60_000);
        p.start(
            "T-1",
            &dir(),
            "head -c 2000000 /dev/zero | tr '\\0' a; echo; echo http://x/y; sleep 30",
        )
        .unwrap();
        let v = until(&p, "T-1", running).await;
        assert_eq!(v.url.as_deref(), Some("http://x/y"));
        let out = p.output("T-1");
        assert!(out.iter().all(|l| l.len() <= MAX_LINE));
        assert!(out.iter().any(|l| l.len() == MAX_LINE));
        p.stop("T-1");
    }

    #[tokio::test]
    async fn a_capped_line_reader_drops_the_rest_until_the_newline() {
        let mut data = vec![b'a'; 10_000];
        data.extend_from_slice(b"\r\nnext\nlast");
        let mut reader = BufReader::with_capacity(64, &data[..]);
        let mut buf = Vec::new();
        assert!(read_capped_line(&mut reader, &mut buf, 100).await.unwrap());
        assert_eq!(buf.len(), 100);
        assert!(read_capped_line(&mut reader, &mut buf, 100).await.unwrap());
        assert_eq!(buf, b"next");
        assert!(read_capped_line(&mut reader, &mut buf, 100).await.unwrap());
        assert_eq!(buf, b"last");
        assert!(!read_capped_line(&mut reader, &mut buf, 100).await.unwrap());
    }

    fn alive(pid: &str) -> bool {
        std::process::Command::new("/bin/kill")
            .args(["-0", pid])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    }

    #[tokio::test]
    async fn a_failed_command_does_not_orphan_what_it_backgrounded() {
        let p = previews(60_000);
        let pid_file = dir().join(format!("delilah-preview-{}", uuid::Uuid::new_v4()));
        let cmd = format!(
            "sleep 30 & echo $! > {}; echo bye; exit 1",
            pid_file.display()
        );
        p.start("T-1", &dir(), &cmd).unwrap();
        until(&p, "T-1", |v| v.state == PreviewState::Failed).await;
        let pid = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .to_string();
        let _ = std::fs::remove_file(&pid_file);
        for _ in 0..100 {
            if !alive(&pid) {
                p.stop("T-1");
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("the backgrounded sleep is still alive");
    }

    #[tokio::test]
    async fn stopping_leaves_no_waiter_or_timeout_behind() {
        let p = previews(600_000);
        p.start("T-1", &dir(), "sleep 30").unwrap();
        assert_eq!(p.waiters.load(Ordering::SeqCst), 1);
        p.stop("T-1");
        for _ in 0..100 {
            if p.waiters.load(Ordering::SeqCst) == 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("the waiter (and its timeout) is still there");
    }
}
