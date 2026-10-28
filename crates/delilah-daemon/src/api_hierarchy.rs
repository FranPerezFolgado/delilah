//! HTTP handlers for parent tasks and subtasks.

use axum::Json;
use axum::extract::{Path, State};
use delilah_core::TaskKey;
use delilah_core::hierarchy::TaskKind;
use serde::Serialize;

use crate::api::ApiError;
use crate::integration::{self, SubtaskPr};
use crate::pr_types::PrState;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct SubtaskPrSummary {
    pub url: String,
    /// `open`, `merged` or `closed`; `open` also while the PR was not read yet.
    pub state: &'static str,
}

#[derive(Debug, Serialize)]
pub struct SubtaskSummary {
    pub key: String,
    pub title: String,
    pub status: delilah_core::TaskStatus,
    pub archived: bool,
    pub dropped: bool,
    pub pr: Option<SubtaskPrSummary>,
}

impl From<SubtaskPr> for SubtaskPrSummary {
    fn from(p: SubtaskPr) -> Self {
        let state = match p.state {
            Some(PrState::Merged) => "merged",
            Some(PrState::Closed) => "closed",
            _ => "open",
        };
        Self { url: p.url, state }
    }
}

/// The task `key`, which must be a parent task (404 otherwise).
async fn parent_task(app: &AppState, key: &str) -> Result<delilah_core::Task, ApiError> {
    let key = TaskKey::new(key);
    match app.store.get_task(&key).await? {
        Some(t) if t.kind == TaskKind::ParentTask => Ok(t),
        Some(_) => Err(ApiError::not_found(format!("{key} is not a parent task"))),
        None => Err(ApiError::not_found(format!("no task {key}"))),
    }
}

/// `GET /api/v1/tasks/{key}/subtasks`.
pub async fn subtasks(
    State(app): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Vec<SubtaskSummary>>, ApiError> {
    let parent = parent_task(&app, &key).await?;
    let facts = integration::subtask_facts(&app.store, &parent.key)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(
        facts
            .into_iter()
            .map(|f| SubtaskSummary {
                key: f.task.key.to_string(),
                title: f.task.title,
                status: f.task.status,
                archived: f.facts.archived,
                dropped: f.facts.archived && !f.facts.merged,
                pr: f.pr.map(Into::into),
            })
            .collect(),
    ))
}

/// `POST /api/v1/tasks/{key}/push-branch`: pushes the parent task's branch again.
pub async fn push_branch(
    State(app): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let parent = parent_task(&app, &key).await?;
    let (_, ws) = app
        .store
        .workspace(&parent.key)
        .await?
        .ok_or_else(|| ApiError::conflict(format!("{} has no workspace yet", parent.key)))?;
    let opts = delilah_git::GitOptions {
        allow_file_protocol: app.config.allow_file_protocol,
    };
    let pushing = ws.clone();
    tokio::task::spawn_blocking(move || integration::push_workspace_branch(&pushing, &opts))
        .await
        .map_err(|e| ApiError::internal(format!("git task failed: {e}")))?
        .map_err(|e| ApiError::conflict(e.to_string()))?;
    Ok(Json(serde_json::json!({ "branch": ws.branch })))
}

/// A known task, 404 otherwise.
async fn known(app: &AppState, key: &str) -> Result<delilah_core::Task, ApiError> {
    let key = TaskKey::new(key);
    app.store
        .get_task(&key)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("no task {key}")))
}

/// `POST /api/v1/tasks/{key}/make-parent-task`: turns a normal task into a parent task.
/// A running task keeps its workspace; its agent settings are turned off and no agent starts.
pub async fn make_parent_task(
    State(app): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<delilah_core::Task>, ApiError> {
    let task = known(&app, &key).await?;
    let has_open_pr = app
        .store
        .prs_of(&task.key)
        .await?
        .iter()
        .any(|p| p.snapshot.as_ref().is_none_or(|s| s.state == PrState::Open));
    let has_session = !app
        .store
        .open_sessions(Some(task.key.as_str()))
        .await?
        .is_empty();
    delilah_core::hierarchy::check_make_parent_task(&task, has_open_pr, has_session)
        .map_err(|r| ApiError::refused(app.lang(), r, serde_json::Value::Null))?;
    if task.kind == TaskKind::ParentTask {
        return Ok(Json(task));
    }
    crate::store::become_parent_task(&app.store.pool, &task.key).await?;
    let updated = known(&app, &key).await?;
    app.emit(crate::events::Topic::Tasks);
    Ok(Json(updated))
}

#[derive(Debug, serde::Deserialize)]
pub struct AttachBody {
    pub parent_task: String,
}

/// `POST /api/v1/tasks/{key}/attach`: makes a never-started normal task a subtask.
pub async fn attach(
    State(app): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<AttachBody>,
) -> Result<Json<delilah_core::Task>, ApiError> {
    use delilah_core::hierarchy::Refusal;
    let task = known(&app, &key).await?;
    let parent_key = TaskKey::new(&body.parent_task);
    let parent = app.store.get_task(&parent_key).await?;
    let refused = |r: Refusal| {
        let detail = serde_json::json!({ "key": parent_key.as_str() });
        ApiError::refused(app.lang(), r, detail)
    };
    let Some(parent) = parent else {
        return Err(refused(Refusal::NotAParentTask));
    };
    let archived = parent.status.is_terminal() && app.store.workspace(&parent.key).await?.is_none();
    let started = task.status != delilah_core::TaskStatus::Queued
        || app.store.workspace(&task.key).await?.is_some();
    delilah_core::hierarchy::check_attach(&task, &parent, archived, started).map_err(refused)?;
    // A subtask takes its parent task's project.
    sqlx::query("UPDATE tasks SET project = ? WHERE key = ?")
        .bind(parent.project.as_deref())
        .bind(task.key.as_str())
        .execute(&app.store.pool)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let updated = app
        .store
        .set_kind(&task.key, TaskKind::Subtask, Some(&parent.key))
        .await?;
    app.emit(crate::events::Topic::Tasks);
    Ok(Json(updated))
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use crate::api::router;
    use crate::state::AppState;
    use crate::store::Store;

    async fn call(
        app: &Router,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(uri);
        let body = match body {
            Some(json) => {
                req = req.header("content-type", "application/json");
                Body::from(json.to_string())
            }
            None => Body::empty(),
        };
        let res = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        (status, value)
    }

    async fn app() -> (Router, AppState) {
        let config = crate::config::Config {
            home: std::env::temp_dir().join(format!("delilah-api-{}", uuid::Uuid::new_v4())),
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            claude_bin: "claude".into(),
            agent_program: None,
            ..crate::config::Config::test_defaults()
        };
        let state = AppState::new(Store::in_memory().await.unwrap(), config);
        for name in ["web", "api"] {
            state
                .store
                .create_project(&delilah_core::Project {
                    name: name.into(),
                    path: format!("/w/{name}").into(),
                    kind: delilah_core::ProjectKind::Repo,
                    workflow: "prompt".into(),
                    settings: crate::autopilot::feature_settings(),
                    preview_command: None,
                })
                .await
                .unwrap();
        }
        (router(state.clone()), state)
    }

    async fn create(app: &Router, body: Value) -> (StatusCode, Value) {
        call(app, "POST", "/api/v1/tasks", Some(body)).await
    }

    async fn parent(app: &Router) -> Value {
        let (s, p) = create(
            app,
            json!({"free": true, "title": "P", "project": "web", "kind": "parent_task",
                   "saving_mode": true, "autopilot": false, "ci_autofix": true}),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED, "{p}");
        p
    }

    #[tokio::test]
    async fn a_parent_task_has_no_agent_settings() {
        let (app, _) = app().await;
        let p = parent(&app).await;
        assert_eq!(p["kind"], "parent_task");
        assert_eq!(p["workflow"], "none");
        assert_eq!(p["saving_mode"], false);
        assert!(p["autopilot"].is_null());
        assert_eq!(p["ci_autofix"], false);
        assert_ne!(p["in_checkout"], true);
    }

    #[tokio::test]
    async fn a_subtask_lands_in_the_parent_tasks_project_with_free_keys() {
        let (app, _) = app().await;
        let p = parent(&app).await;
        let pk = p["key"].as_str().unwrap();
        let (s, a) = create(&app, json!({"free": true, "title": "a", "parent_task": pk})).await;
        assert_eq!(s, StatusCode::CREATED, "{a}");
        assert_eq!(a["kind"], "subtask");
        assert_eq!(a["parent_task"], pk);
        assert_eq!(a["project"], "web");
        let (s, b) = create(
            &app,
            json!({"free": true, "title": "b", "parent_task": pk, "project": "web"}),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
        assert_ne!(a["key"], b["key"]);
        assert_ne!(a["key"], p["key"]);
    }

    #[tokio::test]
    async fn creating_a_subtask_is_refused_with_the_contract_reason() {
        let (app, state) = app().await;
        let p = parent(&app).await;
        let pk = p["key"].as_str().unwrap().to_string();
        let (_, normal) = create(&app, json!({"free": true, "title": "n", "project": "web"})).await;
        let (_, sub) = create(&app, json!({"free": true, "title": "s", "parent_task": pk})).await;

        let (s, body) = create(
            &app,
            json!({"free": true, "title": "x", "parent_task": normal["key"]}),
        )
        .await;
        assert_eq!(
            (s, body["error"].as_str()),
            (StatusCode::CONFLICT, Some("not-a-parent-task")),
            "{body}"
        );
        assert!(body["message"].is_string());
        let (s, body) = create(
            &app,
            json!({"free": true, "title": "x", "parent_task": sub["key"]}),
        )
        .await;
        assert_eq!(
            (s, body["error"].as_str()),
            (StatusCode::CONFLICT, Some("not-a-parent-task"))
        );
        let (s, body) = create(
            &app,
            json!({"free": true, "title": "x", "parent_task": "NOPE-1"}),
        )
        .await;
        assert_eq!(
            (s, body["error"].as_str()),
            (StatusCode::CONFLICT, Some("not-a-parent-task"))
        );
        let (s, body) = create(
            &app,
            json!({"free": true, "title": "x", "kind": "parent_task", "parent_task": pk}),
        )
        .await;
        assert_eq!(
            (s, body["error"].as_str()),
            (StatusCode::CONFLICT, Some("nesting")),
            "{body}"
        );
        let (s, body) = create(
            &app,
            json!({"free": true, "title": "x", "parent_task": pk, "project": "api"}),
        )
        .await;
        assert_eq!(
            (s, body["error"].as_str()),
            (StatusCode::CONFLICT, Some("project-mismatch")),
            "{body}"
        );

        sqlx::query("UPDATE tasks SET status = '{\"type\":\"done\"}' WHERE key = ?")
            .bind(&pk)
            .execute(&state.store.pool)
            .await
            .unwrap();
        let (s, body) = create(&app, json!({"free": true, "title": "x", "parent_task": pk})).await;
        assert_eq!(
            (s, body["error"].as_str()),
            (StatusCode::CONFLICT, Some("parent-task-archived")),
            "{body}"
        );
    }

    #[tokio::test]
    async fn tasks_carry_kind_parent_and_progress() {
        let (app, _) = app().await;
        let p = parent(&app).await;
        let pk = p["key"].as_str().unwrap();
        create(&app, json!({"free": true, "title": "a", "parent_task": pk})).await;
        create(&app, json!({"free": true, "title": "b", "parent_task": pk})).await;
        let (_, list) = call(&app, "GET", "/api/v1/tasks", None).await;
        let list = list.as_array().unwrap();
        let by_key = |k: &str| list.iter().find(|t| t["key"] == k).unwrap().clone();
        let got = by_key(pk);
        assert_eq!(
            got["subtask_progress"],
            json!({"merged": 0, "counted": 2, "dropped": 0, "ready": false})
        );
        let sub = list.iter().find(|t| t["kind"] == "subtask").unwrap();
        assert_eq!(sub["parent_task"], pk);
        assert!(sub.get("subtask_progress").is_none());
        let (_, one) = call(&app, "GET", &format!("/api/v1/tasks/{pk}"), None).await;
        assert_eq!(one["subtask_progress"]["counted"], 2);
        assert_eq!(one["kind"], "parent_task");
    }

    #[tokio::test]
    async fn subtasks_lists_the_children_and_404s_for_other_kinds() {
        let (app, _) = app().await;
        let p = parent(&app).await;
        let pk = p["key"].as_str().unwrap();
        let (_, a) = create(&app, json!({"free": true, "title": "a", "parent_task": pk})).await;
        let (s, list) = call(&app, "GET", &format!("/api/v1/tasks/{pk}/subtasks"), None).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(list.as_array().unwrap().len(), 1);
        assert_eq!(list[0]["key"], a["key"]);
        assert_eq!(list[0]["archived"], false);
        assert_eq!(list[0]["dropped"], false);
        assert!(list[0]["pr"].is_null());
        let ak = a["key"].as_str().unwrap();
        let (s, _) = call(&app, "GET", &format!("/api/v1/tasks/{ak}/subtasks"), None).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        let (s, _) = call(&app, "GET", "/api/v1/tasks/NOPE-1/subtasks", None).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn push_branch_is_for_started_parent_tasks_only() {
        let (app, _) = app().await;
        let p = parent(&app).await;
        let pk = p["key"].as_str().unwrap();
        let (_, a) = create(&app, json!({"free": true, "title": "a", "parent_task": pk})).await;
        let ak = a["key"].as_str().unwrap();
        let (s, _) = call(
            &app,
            "POST",
            &format!("/api/v1/tasks/{ak}/push-branch"),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        let (s, _) = call(
            &app,
            "POST",
            &format!("/api/v1/tasks/{pk}/push-branch"),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::CONFLICT);
    }

    async fn status_of(app: &Router, key: &str) -> Value {
        call(app, "GET", &format!("/api/v1/tasks/{key}"), None)
            .await
            .1
    }

    async fn normal(app: &Router, title: &str) -> String {
        let (s, t) = create(app, json!({"free": true, "title": title, "project": "web"})).await;
        assert_eq!(s, StatusCode::CREATED, "{t}");
        t["key"].as_str().unwrap().to_string()
    }

    async fn post(app: &Router, key: &str, verb: &str, body: Option<Value>) -> (StatusCode, Value) {
        call(app, "POST", &format!("/api/v1/tasks/{key}/{verb}"), body).await
    }

    #[tokio::test]
    async fn make_parent_task_converts_a_normal_task_and_refuses_the_rest() {
        let (app, state) = app().await;
        let k = normal(&app, "n").await;
        let (s, t) = post(&app, &k, "make-parent-task", None).await;
        assert_eq!(s, StatusCode::OK, "{t}");
        assert_eq!(t["kind"], "parent_task");
        assert_eq!(t["workflow"], "none");
        assert_eq!(t["saving_mode"], false);
        assert!(t["autopilot"].is_null());
        // Again: already a parent task, nothing changes.
        assert_eq!(
            post(&app, &k, "make-parent-task", None).await.0,
            StatusCode::OK
        );

        let p = parent(&app).await;
        let (_, sub) = create(
            &app,
            json!({"free": true, "title": "s", "parent_task": p["key"]}),
        )
        .await;
        let (s, b) = post(&app, sub["key"].as_str().unwrap(), "make-parent-task", None).await;
        assert_eq!(
            (s, b["error"].as_str()),
            (StatusCode::CONFLICT, Some("nesting")),
            "{b}"
        );

        let c = normal(&app, "c").await;
        sqlx::query("UPDATE tasks SET in_checkout = 1 WHERE key = ?")
            .bind(&c)
            .execute(&state.store.pool)
            .await
            .unwrap();
        let (s, b) = post(&app, &c, "make-parent-task", None).await;
        assert_eq!(
            (s, b["error"].as_str()),
            (StatusCode::CONFLICT, Some("in-checkout")),
            "{b}"
        );

        let w = normal(&app, "w").await;
        sqlx::query(
            "INSERT INTO sessions (id, task_key, kind, label, is_primary, created_at)
             VALUES ('s1', ?, 'claude', 'Main', 1, 'x')",
        )
        .bind(&w)
        .execute(&state.store.pool)
        .await
        .unwrap();
        let (s, b) = post(&app, &w, "make-parent-task", None).await;
        assert_eq!(
            (s, b["error"].as_str()),
            (StatusCode::CONFLICT, Some("has-session")),
            "{b}"
        );

        let o = normal(&app, "o").await;
        state
            .store
            .link_pr(
                &delilah_core::TaskKey::new(&o),
                &crate::store_prs::NewPr {
                    repo: "r".into(),
                    host: "github.com".into(),
                    owner_name: "me/r".into(),
                    number: 1,
                    url: "https://github.com/me/r/pull/1".into(),
                    linked_by: "detected".into(),
                },
            )
            .await
            .unwrap();
        let (s, b) = post(&app, &o, "make-parent-task", None).await;
        assert_eq!(
            (s, b["error"].as_str()),
            (StatusCode::CONFLICT, Some("has-open-pr")),
            "{b}"
        );
    }

    #[tokio::test]
    async fn a_running_task_converted_keeps_its_workspace_and_starts_no_agent() {
        let (app, state) = app().await;
        let k = normal(&app, "r").await;
        let key = delilah_core::TaskKey::new(&k);
        state
            .store
            .save_workspace(
                &key,
                "/tmp/parent".as_ref(),
                &delilah_git::Workspace {
                    path: "/tmp/ws".into(),
                    branch: format!("feature/{k}"),
                    kind: delilah_core::ProjectKind::Repo,
                    submodules: vec![],
                },
            )
            .await
            .unwrap();
        sqlx::query("UPDATE tasks SET status = '{\"type\":\"running\"}', autopilot = NULL, saving_mode = 1, ci_autofix = 1 WHERE key = ?")
            .bind(&k)
            .execute(&state.store.pool)
            .await
            .unwrap();
        let (s, t) = post(&app, &k, "make-parent-task", None).await;
        assert_eq!(s, StatusCode::OK, "{t}");
        assert_eq!(t["kind"], "parent_task");
        assert_eq!(t["status"]["type"], "running");
        assert_eq!(t["saving_mode"], false);
        assert_eq!(t["ci_autofix"], false);
        assert!(state.store.workspace(&key).await.unwrap().is_some());
        assert!(
            state
                .store
                .open_sessions(Some(&k))
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn attach_makes_a_never_started_task_a_subtask_in_the_parents_project() {
        let (app, state) = app().await;
        let p = parent(&app).await;
        let pk = p["key"].as_str().unwrap().to_string();
        let (_, n) = create(&app, json!({"free": true, "title": "n"})).await;
        assert!(n["project"].is_null(), "{n}");
        let nk = n["key"].as_str().unwrap();
        let (s, t) = post(&app, nk, "attach", Some(json!({"parent_task": pk}))).await;
        assert_eq!(s, StatusCode::OK, "{t}");
        assert_eq!(t["kind"], "subtask");
        assert_eq!(t["parent_task"], pk.as_str());
        assert_eq!(t["project"], "web");
        assert_eq!(status_of(&app, nk).await["project"], "web");

        let o = create(&app, json!({"free": true, "title": "o", "project": "api"}))
            .await
            .1;
        let (s, b) = post(
            &app,
            o["key"].as_str().unwrap(),
            "attach",
            Some(json!({"parent_task": pk})),
        )
        .await;
        assert_eq!(
            (s, b["error"].as_str()),
            (StatusCode::CONFLICT, Some("project-mismatch")),
            "{b}"
        );

        // Already a subtask: nesting.
        let (s, b) = post(&app, nk, "attach", Some(json!({"parent_task": pk}))).await;
        assert_eq!(
            (s, b["error"].as_str()),
            (StatusCode::CONFLICT, Some("nesting")),
            "{b}"
        );

        let m = normal(&app, "m").await;
        let (s, b) = post(&app, &m, "attach", Some(json!({"parent_task": m}))).await;
        assert_eq!(
            (s, b["error"].as_str()),
            (StatusCode::CONFLICT, Some("not-a-parent-task")),
            "{b}"
        );
        let (s, b) = post(&app, &m, "attach", Some(json!({"parent_task": "NOPE-1"}))).await;
        assert_eq!(
            (s, b["error"].as_str()),
            (StatusCode::CONFLICT, Some("not-a-parent-task")),
            "{b}"
        );

        sqlx::query("UPDATE tasks SET status = '{\"type\":\"running\"}' WHERE key = ?")
            .bind(&m)
            .execute(&state.store.pool)
            .await
            .unwrap();
        let (s, b) = post(&app, &m, "attach", Some(json!({"parent_task": pk}))).await;
        assert_eq!(
            (s, b["error"].as_str()),
            (StatusCode::CONFLICT, Some("already-started")),
            "{b}"
        );

        let q = normal(&app, "q").await;
        sqlx::query("UPDATE tasks SET status = '{\"type\":\"done\"}' WHERE key = ?")
            .bind(&pk)
            .execute(&state.store.pool)
            .await
            .unwrap();
        let (s, b) = post(&app, &q, "attach", Some(json!({"parent_task": pk}))).await;
        assert_eq!(
            (s, b["error"].as_str()),
            (StatusCode::CONFLICT, Some("parent-task-archived")),
            "{b}"
        );
    }
}
