//! A task's preview: the project's preview command, run by the daemon in the task's
//! workspace. It prints the URL it serves; the first one it prints is the preview's.
//! Memory only: a daemon restart does not adopt it again.

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Json;
use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use delilah_core::TaskKey;
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;

use crate::api::ApiError;
use crate::events::{Events, Topic};
use crate::state::AppState;
use crate::store::StoreError;

/// Lines of output kept, the last ones.
const KEEP_LINES: usize = 200;
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
    Stopped,
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
    pid: u32,
    view: PreviewView,
    output: VecDeque<String>,
}

pub struct Previews {
    entries: Mutex<HashMap<String, Entry>>,
    events: Events,
    url_timeout: Duration,
    next_id: AtomicU64,
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
        entries.insert(
            task.to_string(),
            Entry {
                id,
                pid: child.id().unwrap_or(0),
                view: view.clone(),
                output: VecDeque::new(),
            },
        );
        drop(entries);

        let readers = [
            child.stdout.take().map(|s| self.read(task, id, s)),
            child.stderr.take().map(|s| self.read(task, id, s)),
        ];
        let this = self.clone();
        let key = task.to_string();
        tokio::spawn(async move {
            let status = child.wait().await;
            // The last lines may still be in the pipe.
            for reader in readers.into_iter().flatten() {
                let _ = tokio::time::timeout(Duration::from_secs(1), reader).await;
            }
            this.exited(&key, id, status.map(|s| s.to_string()).unwrap_or_default());
        });
        let this = self.clone();
        let key = task.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(this.url_timeout).await;
            this.timed_out(&key, id);
        });
        self.events.emit(Topic::Tasks);
        Ok(view)
    }

    fn read<R>(self: &Arc<Self>, task: &str, id: u64, stream: R) -> tokio::task::JoinHandle<()>
    where
        R: AsyncRead + Unpin + Send + 'static,
    {
        let this = self.clone();
        let key = task.to_string();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stream).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                this.line(&key, id, line);
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

    fn fail(&self, task: &str, id: u64, why: &str, kill: bool) {
        let mut entries = self.lock();
        let Some(e) = entries.get_mut(task).filter(|e| e.id == id) else {
            return;
        };
        if !matches!(e.view.state, PreviewState::Starting | PreviewState::Running) {
            return;
        }
        let from = e.output.len().saturating_sub(TAIL_LINES);
        let tail = e.output.iter().skip(from).cloned().collect::<Vec<_>>();
        e.view.state = PreviewState::Failed;
        e.view.error = Some(if tail.is_empty() {
            why.to_string()
        } else {
            format!("{why}\n{}", tail.join("\n"))
        });
        let pid = e.pid;
        drop(entries);
        if kill {
            terminate(pid);
        }
        self.events.emit(Topic::Tasks);
    }

    fn exited(&self, task: &str, id: u64, status: String) {
        self.fail(
            task,
            id,
            &format!("the preview command exited ({status})"),
            false,
        );
    }

    fn timed_out(&self, task: &str, id: u64) {
        let starting = self
            .lock()
            .get(task)
            .is_some_and(|e| e.id == id && e.view.state == PreviewState::Starting);
        if starting {
            let mins = self.url_timeout.as_secs_f64() / 60.0;
            self.fail(
                task,
                id,
                &format!("the preview command printed no URL in {mins:.1} minutes"),
                true,
            );
        }
    }

    /// Ends the task's preview: its whole process group gets SIGTERM, and SIGKILL if it
    /// is still there after a few seconds. The preview is gone afterwards.
    pub fn stop(&self, task: &str) {
        let entry = self.lock().remove(task);
        if let Some(e) = entry {
            if matches!(e.view.state, PreviewState::Starting | PreviewState::Running) {
                terminate(e.pid);
            }
            self.events.emit(Topic::Tasks);
        }
    }

    /// For the daemon's shutdown: nothing is left running behind it.
    pub async fn stop_all(&self) {
        let pids: Vec<u32> = self.lock().drain().map(|(_, e)| e.pid).collect();
        for pid in &pids {
            signal_group(*pid, "TERM");
        }
        if !pids.is_empty() {
            tokio::time::sleep(Duration::from_millis(500)).await;
            for pid in pids {
                signal_group(pid, "KILL");
            }
        }
    }
}

/// SIGTERM to the group now, SIGKILL after the grace period.
fn terminate(pid: u32) {
    signal_group(pid, "TERM");
    tokio::spawn(async move {
        tokio::time::sleep(GRACE).await;
        signal_group(pid, "KILL");
    });
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

/// `GET /api/v1/tasks/{key}/preview`
pub async fn get_preview(
    State(app): State<AppState>,
    UrlPath(key): UrlPath<String>,
) -> Json<Option<PreviewView>> {
    Json(app.previews.view(&key))
}

/// `POST /api/v1/tasks/{key}/preview`
pub async fn start_preview(
    State(app): State<AppState>,
    UrlPath(key): UrlPath<String>,
) -> Result<(StatusCode, Json<PreviewView>), ApiError> {
    let task_key = TaskKey::new(key.clone());
    let task = app
        .store
        .get_task(&task_key)
        .await?
        .ok_or(StoreError::NotFound(task_key.clone()))?;
    let command = match &task.project {
        Some(name) => app.store.project(name).await?.preview_command,
        None => None,
    }
    .ok_or_else(|| {
        ApiError::conflict_reason("this project has no preview command", "no-preview-command")
    })?;
    let (_, workspace) = app.store.workspace(&task_key).await?.ok_or_else(|| {
        ApiError::conflict_reason("the task has no workspace yet", "no-workspace")
    })?;
    let view = app
        .previews
        .start(&key, &workspace.path, &command)
        .map_err(|e| match e {
            StartError::Running => {
                ApiError::conflict_reason("the preview is already running", "running")
            }
            StartError::Spawn(why) => ApiError::unprocessable(format!("could not start: {why}")),
        })?;
    Ok((StatusCode::CREATED, Json(view)))
}

/// `DELETE /api/v1/tasks/{key}/preview`
pub async fn stop_preview(
    State(app): State<AppState>,
    UrlPath(key): UrlPath<String>,
) -> StatusCode {
    app.previews.stop(&key);
    StatusCode::NO_CONTENT
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    use super::*;
    use crate::gh::fake::FakeGh;
    use crate::pr_testkit::task_with_workspace;

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

    async fn call(app: &Router, method: &str, uri: &str) -> (StatusCode, Value) {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        (status, value)
    }

    #[tokio::test]
    async fn the_routes_start_show_and_stop_it() {
        let (app, key, _guard) = task_with_workspace(Arc::new(FakeGh::happy()), None).await;
        let router = crate::api::router(app.clone());
        let uri = format!("/api/v1/tasks/{key}/preview");

        let (status, body) = call(&router, "GET", &uri).await;
        assert_eq!((status, body), (StatusCode::OK, Value::Null));

        let (status, body) = call(&router, "POST", &uri).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["reason"], "no-preview-command");

        app.store
            .set_preview_command("proj", Some("echo http://127.0.0.1:9/p; sleep 30"))
            .await
            .unwrap();
        let (status, body) = call(&router, "POST", &uri).await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(body["state"], "starting");

        let (status, body) = call(&router, "POST", &uri).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["reason"], "running");

        until(&app.previews, key.as_str(), running).await;
        let (_, body) = call(&router, "GET", &uri).await;
        assert_eq!(body["state"], "running");
        assert_eq!(body["url"], "http://127.0.0.1:9/p");

        let (status, _) = call(&router, "DELETE", &uri).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (_, body) = call(&router, "GET", &uri).await;
        assert_eq!(body, Value::Null);
    }

    #[tokio::test]
    async fn no_workspace_is_refused() {
        let (app, key, _guard) = task_with_workspace(Arc::new(FakeGh::happy()), None).await;
        app.store
            .set_preview_command("proj", Some("sleep 30"))
            .await
            .unwrap();
        let mut other = delilah_core::Task::new(TaskKey::new("PROJ-2"), "t", "feature");
        other.project = Some("proj".into());
        app.store.create_task(&other).await.unwrap();
        let router = crate::api::router(app);
        let (status, body) = call(&router, "POST", "/api/v1/tasks/PROJ-2/preview").await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["reason"], "no-workspace");
        let _ = key;
    }
}
