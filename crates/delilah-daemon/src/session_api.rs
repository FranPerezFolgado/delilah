//! `/api/v1/sessions` and `/api/v1/tasks/{key}/sessions`: extra Claude sessions and
//! shells in a task's workspace, next to its primary session.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use delilah_core::TaskKey;
use serde::{Deserialize, Serialize};

use crate::api::ApiError;
use crate::events::Topic;
use crate::run::launch_session;
use crate::sessions::LiveState;
use crate::state::AppState;
use crate::store::StoreError;
use crate::store_sessions::{SessionKind, SessionRecord, project_of, project_owner};

#[derive(Debug, Serialize)]
pub struct SessionView {
    #[serde(flatten)]
    pub record: SessionRecord,
    /// Whether its process is running in this daemon now.
    pub live: bool,
    /// Live sessions only.
    pub state: Option<LiveState>,
}

#[derive(Debug, Deserialize)]
pub struct OpenSession {
    pub kind: SessionKind,
    pub label: Option<String>,
    /// Claude only: a first message, e.g. "Revisa los cambios de esta rama".
    pub prompt: Option<String>,
    /// Claude in plan mode. Defaults to true in a project (its checkout is yours),
    /// false in a task.
    pub read_only: Option<bool>,
}

/// Every open session, of every task.
pub async fn list_all(State(app): State<AppState>) -> Result<Json<Vec<SessionView>>, ApiError> {
    let records = app.store.open_sessions(None).await?;
    Ok(Json(records.into_iter().map(|r| view(&app, r)).collect()))
}

pub async fn list_for_task(
    State(app): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Vec<SessionView>>, ApiError> {
    let records = app.store.open_sessions(Some(&key)).await?;
    Ok(Json(records.into_iter().map(|r| view(&app, r)).collect()))
}

/// Opens another session in the task's workspace.
pub async fn open(
    State(app): State<AppState>,
    Path(key): Path<String>,
    Json(req): Json<OpenSession>,
) -> Result<(StatusCode, Json<SessionView>), ApiError> {
    let task_key = TaskKey::new(&key);
    app.store
        .get_task(&task_key)
        .await?
        .ok_or(StoreError::NotFound(task_key.clone()))?;
    open_for(&app, key, req).await
}

/// Opens a session in a project's own checkout, to ask about the code without a task:
/// Claude starts read-only unless asked otherwise.
pub async fn open_in_project(
    State(app): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<OpenSession>,
) -> Result<(StatusCode, Json<SessionView>), ApiError> {
    app.store.project(&name).await?;
    open_for(&app, project_owner(&name), req).await
}

async fn open_for(
    app: &AppState,
    key: String,
    req: OpenSession,
) -> Result<(StatusCode, Json<SessionView>), ApiError> {
    let ws = crate::owners::workspace(app, &key)
        .await
        .map_err(|_| ApiError::conflict(format!("{key} has no workspace (archived?)")))?;
    let read_only =
        req.kind == SessionKind::Claude && req.read_only.unwrap_or(project_of(&key).is_some());

    let existing = app.store.open_sessions(Some(&key)).await?;
    let label = req
        .label
        .filter(|l| !l.trim().is_empty())
        .unwrap_or_else(|| {
            let same = existing.iter().filter(|s| s.kind == req.kind).count() + 1;
            match req.kind {
                SessionKind::Claude => format!("Claude {same}"),
                SessionKind::Shell => {
                    if same == 1 {
                        "Terminal".into()
                    } else {
                        format!("Terminal {same}")
                    }
                }
            }
        });
    let record = SessionRecord {
        id: uuid::Uuid::new_v4().to_string(),
        task: key.clone(),
        kind: req.kind,
        label,
        primary: false,
        closed: false,
        read_only,
        conversation: false,
    };
    app.store.create_session(&record).await?;
    app.emit(Topic::Sessions);
    let prompt = req.prompt.as_deref().filter(|p| !p.trim().is_empty());
    launch_session(app, &record, &ws.path, prompt, false)?;
    Ok((StatusCode::CREATED, Json(view(app, record))))
}

/// Lets a read-only Claude edit: it starts again in normal mode on the same
/// conversation (`--resume`), so nothing said so far is lost.
pub async fn allow_edits(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SessionView>, ApiError> {
    let mut record = app
        .store
        .session(&id)
        .await?
        .filter(|r| !r.closed && r.kind == SessionKind::Claude)
        .ok_or_else(|| ApiError::not_found(format!("no open Claude session {id}")))?;
    app.store.set_read_only(&id, false).await?;
    record.read_only = false;
    if let Some(live) = app.sessions.get(&id) {
        let _ = live.pty.kill();
        live.pty.wait().await;
    }
    let ws = crate::owners::workspace(&app, &record.task).await?;
    launch_session(&app, &record, &ws.path, None, true)?;
    app.emit(Topic::Sessions);
    Ok(Json(view(&app, record)))
}

/// Stops an extra session and removes it from the task. The primary one stays: it
/// ends with the task (`done`).
pub async fn close(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let record = app
        .store
        .session(&id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("session {id} does not exist")))?;
    if record.primary {
        return Err(ApiError::conflict(
            "the primary session ends with the task: mark the task done",
        ));
    }
    if let Some(live) = app.sessions.get(&id) {
        let _ = live.pty.kill();
    }
    app.store.close_session(&id).await?;
    app.emit(Topic::Sessions);
    app.emit(Topic::Inbox);
    Ok(StatusCode::NO_CONTENT)
}

/// Starts a session that is no longer running again: Claude picks its conversation
/// back up with `--resume`; a shell starts fresh.
pub async fn reopen(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SessionView>, ApiError> {
    let record = app
        .store
        .session(&id)
        .await?
        .filter(|r| !r.closed)
        .ok_or_else(|| ApiError::not_found(format!("session {id} does not exist or was closed")))?;
    if app.sessions.get(&id).is_some_and(|l| !l.pty.has_exited()) {
        return Err(ApiError::conflict("the session is already running"));
    }
    let ws = crate::owners::workspace(&app, &record.task)
        .await
        .map_err(|_| ApiError::conflict(format!("{} has no workspace (archived?)", record.task)))?;
    launch_session(
        &app,
        &record,
        &ws.path,
        None,
        record.kind == SessionKind::Claude,
    )?;
    Ok(Json(view(&app, record)))
}

fn view(app: &AppState, record: SessionRecord) -> SessionView {
    let live = app.sessions.get(&record.id).filter(|l| !l.pty.has_exited());
    SessionView {
        live: live.is_some(),
        state: live.map(|l| l.state()),
        record,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use axum::body::Body;
    use axum::http::Request;
    use chrono::Utc;
    use delilah_agents::pty::PtySession;
    use delilah_core::{Event, Task, TaskStatus};
    use delilah_git::Workspace;
    use portable_pty::CommandBuilder;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use super::*;
    use crate::config::Config;
    use crate::store::Store;

    /// A running task with a workspace folder, its primary session and one extra
    /// Claude session, both played by `bash` echoing what they are sent.
    async fn task_with_two_sessions() -> (AppState, Arc<PtySession>) {
        let home = std::env::temp_dir().join(format!("delilah-sessions-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(home.join("ws")).unwrap();
        let config = Config {
            home: home.clone(),
            addr: "127.0.0.1:7420".parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
        };
        let app = AppState::new(Store::in_memory().await.unwrap(), config);
        let key = TaskKey::new("PROJ-1");
        app.store
            .create_task(&Task::new(key.clone(), "Card retries", "feature"))
            .await
            .unwrap();
        app.transition(&key, Event::Start).await.unwrap();
        app.transition(&key, Event::WorkspaceReady).await.unwrap();
        let ws = Workspace {
            path: home.join("ws"),
            branch: "feature/PROJ-1".into(),
            kind: delilah_core::ProjectKind::Repo,
            submodules: vec![],
        };
        app.store.save_workspace(&key, &home, &ws).await.unwrap();
        let mut extra = None;
        for (id, primary) in [("main", true), ("review", false)] {
            let record = SessionRecord {
                id: id.into(),
                task: "PROJ-1".into(),
                kind: SessionKind::Claude,
                label: id.into(),
                primary,
                closed: false,
                read_only: false,
                conversation: false,
            };
            app.store.create_session(&record).await.unwrap();
            let mut cmd = CommandBuilder::new("bash");
            cmd.args([
                "--noprofile",
                "--norc",
                "-c",
                "read line; echo \"got:$line\"; sleep 5",
            ]);
            let pty = PtySession::spawn(cmd, 200, 24, None).unwrap();
            app.sessions
                .insert(id, "PROJ-1", SessionKind::Claude, primary, pty.clone());
            if !primary {
                extra = Some(pty);
            }
        }
        (app, extra.unwrap())
    }

    async fn call(
        app: &AppState,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(uri);
        let body = match body {
            Some(b) => {
                req = req.header("content-type", "application/json");
                Body::from(b.to_string())
            }
            None => Body::empty(),
        };
        let res = crate::api::router(app.clone())
            .oneshot(req.body(body).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = http_body_util::BodyExt::collect(res.into_body())
            .await
            .unwrap()
            .to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn status(app: &AppState) -> TaskStatus {
        app.store
            .get_task(&TaskKey::new("PROJ-1"))
            .await
            .unwrap()
            .unwrap()
            .status
    }

    #[tokio::test]
    async fn an_extra_sessions_turns_do_not_move_the_task() {
        let (app, _) = task_with_two_sessions().await;
        call(
            &app,
            "POST",
            "/hooks/claude?task=PROJ-1&session=review",
            Some(json!({ "hook_event_name": "Stop" })),
        )
        .await;
        assert_eq!(status(&app).await, TaskStatus::Running);
        assert_eq!(
            app.sessions.get("review").unwrap().state(),
            LiveState::Waiting
        );

        call(
            &app,
            "POST",
            "/hooks/claude?task=PROJ-1&session=main",
            Some(json!({ "hook_event_name": "Stop" })),
        )
        .await;
        assert_eq!(status(&app).await, TaskStatus::WaitingForUser);
    }

    #[tokio::test]
    async fn a_limit_in_an_extra_session_pauses_the_task_and_both_resume() {
        let (app, review) = task_with_two_sessions().await;
        // A reset in the past would mean "retry in 30 min": make it due in 2 s.
        let reset = (Utc::now() + chrono::Duration::seconds(2)).timestamp();
        call(&app, "POST", "/hooks/claude/statusline?task=PROJ-1&session=review",
            Some(json!({ "rate_limits": { "five_hour": { "used_percentage": 100, "resets_at": reset } } }))).await;
        call(
            &app,
            "POST",
            "/hooks/claude?task=PROJ-1&session=review",
            Some(json!({ "hook_event_name": "StopFailure" })),
        )
        .await;
        assert_eq!(status(&app).await.name(), "paused-by-limit");
        assert_eq!(
            app.sessions.get("review").unwrap().state(),
            LiveState::Paused
        );

        let (_, mut output) = review.attach();
        tokio::time::sleep(Duration::from_millis(2600)).await;
        crate::scheduler::tick(&app).await;
        assert_eq!(status(&app).await, TaskStatus::Running);
        let mut seen = String::new();
        tokio::time::timeout(Duration::from_secs(5), async {
            while !seen.contains("got:Se ha restablecido") {
                seen.push_str(&String::from_utf8_lossy(&output.recv().await.unwrap()));
            }
        })
        .await
        .unwrap_or_else(|_| panic!("the paused extra session was not resumed: {seen:?}"));
    }

    #[tokio::test]
    async fn lists_sessions_and_refuses_to_close_the_primary() {
        let (app, _) = task_with_two_sessions().await;
        let (_, list) = call(&app, "GET", "/api/v1/tasks/PROJ-1/sessions", None).await;
        assert_eq!(list.as_array().unwrap().len(), 2);
        assert_eq!(list[0]["primary"], true);
        assert_eq!(list[1]["live"], true);

        let (code, body) = call(&app, "POST", "/api/v1/sessions/main/close", None).await;
        assert_eq!(code, StatusCode::CONFLICT, "{body}");
        let (code, _) = call(&app, "POST", "/api/v1/sessions/review/close", None).await;
        assert_eq!(code, StatusCode::NO_CONTENT);
        let (_, list) = call(&app, "GET", "/api/v1/sessions", None).await;
        assert_eq!(list.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_task_without_workspace_cannot_open_sessions() {
        let (app, _) = task_with_two_sessions().await;
        app.store
            .delete_workspace(&TaskKey::new("PROJ-1"))
            .await
            .unwrap();
        let (code, body) = call(
            &app,
            "POST",
            "/api/v1/tasks/PROJ-1/sessions",
            Some(json!({ "kind": "shell" })),
        )
        .await;
        assert_eq!(code, StatusCode::CONFLICT, "{body}");
    }
}

#[cfg(test)]
mod project_tests {
    use axum::body::Body;
    use axum::http::Request;
    use delilah_core::{Project, ProjectKind};
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use super::*;
    use crate::config::Config;
    use crate::store::Store;

    async fn call(app: &AppState, req: Request<Body>) -> (StatusCode, Value) {
        let res = crate::api::router(app.clone()).oneshot(req).await.unwrap();
        let status = res.status();
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
    }

    #[tokio::test]
    async fn a_project_session_runs_in_the_project_checkout() {
        let home = std::env::temp_dir().join(format!("delilah-proj-{}", uuid::Uuid::new_v4()));
        let repo = home.join("plataforma");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(repo.join("src/main.rs"), "fn main() {}\n").unwrap();
        let config = Config {
            home: home.clone(),
            addr: "127.0.0.1:7420".parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
        };
        let app = AppState::new(Store::in_memory().await.unwrap(), config);
        app.store
            .create_project(&Project {
                name: "plataforma".into(),
                path: repo.clone(),
                kind: ProjectKind::Repo,
                workflow: "feature".into(),
            })
            .await
            .unwrap();

        let req = Request::post("/api/v1/projects/plataforma/sessions")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "kind": "shell" }).to_string()))
            .unwrap();
        let (status, session) = call(&app, req).await;
        assert_eq!(status, StatusCode::CREATED, "{session}");
        assert_eq!(session["task"], "@plataforma");
        assert_eq!(session["read_only"], false, "a terminal is never read-only");

        let (_, listed) = call(
            &app,
            Request::get("/api/v1/sessions")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert!(
            listed
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s["task"] == "@plataforma")
        );
        let (status, tree) = call(
            &app,
            Request::get("/api/v1/tasks/@plataforma/tree?path=")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(tree[0]["name"], "src");

        let id = session["id"].as_str().unwrap();
        if let Some(live) = app.sessions.get(id) {
            let _ = live.pty.kill();
        }
        std::fs::remove_dir_all(home).ok();
    }
}
