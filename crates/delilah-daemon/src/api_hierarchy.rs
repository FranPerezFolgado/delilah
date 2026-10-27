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
    let (dir, branch) = (ws.path.clone(), ws.branch.clone());
    tokio::task::spawn_blocking(move || delilah_git::push_branch(&dir, &branch, &opts))
        .await
        .map_err(|e| ApiError::internal(format!("git task failed: {e}")))?
        .map_err(|e| ApiError::conflict(e.to_string()))?;
    Ok(Json(serde_json::json!({ "branch": ws.branch })))
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
}
