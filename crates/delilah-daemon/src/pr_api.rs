//! HTTP routes of a task's pull requests: list, badges, refresh, link and unlink.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use delilah_core::{SpecPhase, TaskKey};
use serde::{Deserialize, Serialize};

use crate::api::ApiError;
use crate::events::Topic;
use crate::gh::{GhError, classify, parse_pr_url};
use crate::phases;
use crate::pr_loop;
use crate::pr_types::{Check, PrState, PrSummary, ReviewDecision, Rollup, Snapshot};
use crate::prs;
use crate::state::AppState;
use crate::store::StoreError;
use crate::store_prs::{NewPr, PrRow};

#[derive(Serialize)]
pub struct SnapshotView {
    state: PrState,
    draft: bool,
    title: String,
    head: String,
    base: String,
    checks: Vec<Check>,
    ci: Rollup,
    review_decision: Option<ReviewDecision>,
    comments: usize,
}

#[derive(Serialize)]
pub struct PrView {
    id: i64,
    repo: String,
    host: String,
    owner_name: String,
    number: i64,
    url: String,
    linked_by: String,
    checked_at: Option<DateTime<Utc>>,
    error: Option<String>,
    snapshot: Option<SnapshotView>,
}

#[derive(Serialize)]
pub struct Problem {
    kind: &'static str,
    message: String,
}

#[derive(Serialize)]
pub struct TaskPrs {
    account: Option<String>,
    problem: Option<Problem>,
    prs: Vec<PrView>,
}

#[derive(Deserialize)]
pub struct LinkRequest {
    url: String,
}

fn view_of_snapshot(s: &Snapshot) -> SnapshotView {
    SnapshotView {
        state: s.state,
        draft: s.draft,
        title: s.title.clone(),
        head: s.head.clone(),
        base: s.base.clone(),
        checks: s.checks.clone(),
        ci: prs::ci_outcome(&s.checks),
        review_decision: s.review_decision,
        comments: s.comments.len(),
    }
}

fn view(row: &PrRow) -> PrView {
    PrView {
        id: row.id,
        repo: row.repo.clone(),
        host: row.host.clone(),
        owner_name: row.owner_name.clone(),
        number: row.number,
        url: row.url.clone(),
        linked_by: row.linked_by.clone(),
        checked_at: row.checked_at,
        error: row.error.clone(),
        snapshot: row.snapshot.as_ref().map(view_of_snapshot),
    }
}

async fn known_task(app: &AppState, key: &str) -> Result<TaskKey, ApiError> {
    let key = TaskKey::new(key);
    app.store
        .get_task(&key)
        .await?
        .ok_or_else(|| StoreError::NotFound(key.clone()))?;
    Ok(key)
}

pub async fn task_prs(
    State(app): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<TaskPrs>, ApiError> {
    let key = known_task(&app, &key).await?;
    let rows = app.store.prs_of(&key).await?;
    let account = match rows.first() {
        Some(row) => app.gh.user(&row.host).await.ok(),
        None => None,
    };
    // `error` is the text of the `gh` failure: classify it again for the kind.
    let problem = rows.iter().find_map(|r| r.error.as_ref()).map(|message| {
        let err = GhError::Failed {
            code: None,
            stderr: message.clone(),
        };
        Problem {
            kind: classify(&err).kind(),
            message: message.clone(),
        }
    });
    Ok(Json(TaskPrs {
        account,
        problem,
        prs: rows.iter().map(view).collect(),
    }))
}

pub async fn summaries(State(app): State<AppState>) -> Result<Json<Vec<PrSummary>>, ApiError> {
    let mut out = Vec::new();
    for task in app.store.list_tasks().await? {
        let rows = app.store.prs_of(&task.key).await?;
        out.extend(prs::summary(task.key.as_str(), &rows));
    }
    Ok(Json(out))
}

pub async fn refresh(
    State(app): State<AppState>,
    Path(key): Path<String>,
) -> Result<StatusCode, ApiError> {
    let key = known_task(&app, &key).await?;
    tokio::spawn(async move { pr_loop::check_task(&app, &key, true).await });
    Ok(StatusCode::ACCEPTED)
}

pub async fn link(
    State(app): State<AppState>,
    Path(key): Path<String>,
    Json(req): Json<LinkRequest>,
) -> Result<Json<PrView>, ApiError> {
    let key = known_task(&app, &key).await?;
    let url = req.url.trim();
    if parse_pr_url(url).is_none() {
        return Err(ApiError::bad_request(
            "that is not the URL of a pull request",
        ));
    }
    let (pr, snapshot) = app
        .gh
        .pr_view_url(url)
        .await
        .map_err(|e| ApiError::conflict(e.to_string()))?;

    let mut repo = ".".to_string();
    if let Some((_, ws)) = app.store.workspace(&key).await? {
        for (name, dir) in ws.repos() {
            if let Ok(info) = app.gh.repo_view(&dir).await
                && info.host == pr.host
                && info.owner_name == pr.owner_name
            {
                repo = name;
                break;
            }
        }
    }
    let new = NewPr {
        repo,
        host: pr.host,
        owner_name: pr.owner_name,
        number: pr.number,
        url: url.to_string(),
        linked_by: "pasted".into(),
    };
    let row = app.store.link_pr(&key, &new).await?;
    phases::advance(&app, &key, SpecPhase::Review).await;
    app.store
        .save_check(row.id, &snapshot, &row.marks, Utc::now(), &[], &[])
        .await?;
    app.emit(Topic::Prs);
    let row = app.store.pr(row.id).await?.unwrap_or(row);
    Ok(Json(view(&row)))
}

pub async fn unlink(
    State(app): State<AppState>,
    Path((key, id)): Path<(String, i64)>,
) -> Result<StatusCode, ApiError> {
    let key = known_task(&app, &key).await?;
    let mine = app
        .store
        .pr(id)
        .await?
        .is_some_and(|r| r.task_key == key.as_str());
    if !mine {
        return Err(ApiError::not_found(format!("{key} has no PR {id}")));
    }
    app.store.unlink_pr(id).await?;
    app.emit(Topic::Prs);
    app.emit(Topic::Inbox);
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Duration;

    use axum::Router;
    use axum::body::Body;
    use axum::http::Request;
    use delilah_core::{Event, Project, ProjectKind, Task};
    use delilah_git::Workspace;
    use http_body_util::BodyExt;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use super::*;
    use crate::config::Config;
    use crate::gh::fake::FakeGh;
    use crate::store::Store;

    const REPO_VIEW: &str = r#"{"nameWithOwner":"o/r","url":"https://github.com/o/r","defaultBranchRef":{"name":"main"}}"#;
    const PR_LIST: &str = r#"[{"number":7,"url":"https://github.com/o/r/pull/7","state":"OPEN","updatedAt":"2026-10-20T10:00:00Z"}]"#;
    const GRAPHQL: &str = r#"{"data":{"repository":{"pullRequest":{"state":"OPEN","isDraft":false,"merged":false,
      "headRefOid":"abc","baseRefName":"main","title":"t","url":"u","reviewDecision":null,
      "commits":{"nodes":[{"commit":{"statusCheckRollup":null}}]},
      "reviews":{"nodes":[]},"comments":{"nodes":[]},"reviewThreads":{"nodes":[]}}}}}"#;
    const PR_URL: &str = "https://github.com/o/r/pull/7";

    fn fake() -> FakeGh {
        FakeGh::new()
            .on(&["api", "user"], Ok("me\n"))
            .on(&["repo", "view"], Ok(REPO_VIEW))
            .on(&["pr", "list"], Ok(PR_LIST))
            .on(&["api", "graphql"], Ok(GRAPHQL))
    }

    async fn setup(gh: FakeGh) -> (Router, AppState) {
        let config = Config {
            home: std::env::temp_dir().join(format!("delilah-prapi-{}", uuid::Uuid::new_v4())),
            addr: "127.0.0.1:7420".parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
        };
        let app = AppState::new(Store::in_memory().await.unwrap(), config).with_gh(Arc::new(gh));
        let key = TaskKey::new("PROJ-1");
        app.store
            .create_project(&Project {
                name: "proj".into(),
                path: "/parent".into(),
                kind: ProjectKind::Repo,
                workflow: "feature".into(),
            })
            .await
            .unwrap();
        let mut task = Task::new(key.clone(), "t", "feature");
        task.project = Some("proj".into());
        app.store.create_task(&task).await.unwrap();
        app.transition(&key, Event::Start).await.unwrap();
        app.transition(&key, Event::WorkspaceReady).await.unwrap();
        let ws = Workspace {
            path: std::env::temp_dir().join("delilah-prapi-ws"),
            branch: "feature/PROJ-1".into(),
            kind: ProjectKind::Repo,
            submodules: vec![],
        };
        app.store
            .save_workspace(&key, Path::new("/parent"), &ws)
            .await
            .unwrap();
        (crate::api::router(app.clone()), app)
    }

    async fn call(
        router: &Router,
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
        let res = router
            .clone()
            .oneshot(req.body(body).unwrap())
            .await
            .unwrap();
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
    async fn lists_the_prs_of_a_task_with_account_and_snapshot_view() {
        let (router, app) = setup(fake()).await;
        let key = TaskKey::new("PROJ-1");
        let (s, v) = call(&router, "GET", "/api/v1/tasks/PROJ-1/prs", None).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v["prs"], json!([]));
        assert_eq!(v["account"], Value::Null);
        assert_eq!(v["problem"], Value::Null);

        pr_loop::check_task(&app, &key, true).await;
        let (_, v) = call(&router, "GET", "/api/v1/tasks/PROJ-1/prs", None).await;
        assert_eq!(v["account"], "me");
        let pr = &v["prs"][0];
        assert_eq!(pr["url"], PR_URL);
        assert_eq!(pr["linked_by"], "detected");
        assert_eq!(pr["snapshot"]["state"], "open");
        assert_eq!(pr["snapshot"]["ci"], "none");
        assert_eq!(pr["snapshot"]["comments"], 0);

        let (s, _) = call(&router, "GET", "/api/v1/tasks/NOPE-1/prs", None).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn shows_the_problem_of_a_failed_check() {
        let (router, app) = setup(fake()).await;
        let key = TaskKey::new("PROJ-1");
        pr_loop::check_task(&app, &key, true).await;
        let id = app.store.prs_of(&key).await.unwrap()[0].id;
        app.store
            .set_pr_error(
                id,
                Some("To get started with GitHub CLI, please run: gh auth login"),
            )
            .await
            .unwrap();
        let (_, v) = call(&router, "GET", "/api/v1/tasks/PROJ-1/prs", None).await;
        assert_eq!(v["problem"]["kind"], "signed-out");
        assert!(
            v["problem"]["message"]
                .as_str()
                .unwrap()
                .contains("gh auth login")
        );
    }

    #[tokio::test]
    async fn summaries_one_per_task_with_prs() {
        let (router, app) = setup(fake()).await;
        let (_, v) = call(&router, "GET", "/api/v1/prs", None).await;
        assert_eq!(v, json!([]));
        pr_loop::check_task(&app, &TaskKey::new("PROJ-1"), true).await;
        let (s, v) = call(&router, "GET", "/api/v1/prs", None).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v[0]["task"], "PROJ-1");
        assert_eq!(v[0]["state"], "open");
        assert_eq!(v[0]["count"], 1);
    }

    #[tokio::test]
    async fn refresh_answers_202_and_checks_in_the_background() {
        let (router, app) = setup(fake()).await;
        let (s, _) = call(&router, "POST", "/api/v1/tasks/NOPE-1/prs/refresh", None).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        let (s, _) = call(&router, "POST", "/api/v1/tasks/PROJ-1/prs/refresh", None).await;
        assert_eq!(s, StatusCode::ACCEPTED);
        let key = TaskKey::new("PROJ-1");
        for _ in 0..100 {
            if !app.store.prs_of(&key).await.unwrap().is_empty() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("the refresh never linked the PR");
    }

    #[tokio::test]
    async fn linking_a_bad_url_is_400_and_a_good_one_moves_to_review() {
        let (router, app) = setup(fake()).await;
        let body = json!({ "url": "https://example.com/nope" });
        let (s, _) = call(&router, "POST", "/api/v1/tasks/PROJ-1/prs/link", Some(body)).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);

        let body = json!({ "url": PR_URL });
        let (s, v) = call(&router, "POST", "/api/v1/tasks/PROJ-1/prs/link", Some(body)).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v["linked_by"], "pasted");
        assert_eq!(v["repo"], ".");
        assert_eq!(v["snapshot"]["state"], "open");
        let task = app
            .store
            .get_task(&TaskKey::new("PROJ-1"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(task.phase.as_deref(), Some("review"));
    }

    #[tokio::test]
    async fn linking_a_pr_gh_cannot_read_is_409() {
        let failing = FakeGh::new().on(
            &["api", "graphql"],
            Err(GhError::Failed {
                code: Some(1),
                stderr: "boom".into(),
            }),
        );
        let (router, _) = setup(failing).await;
        let body = json!({ "url": PR_URL });
        let (s, v) = call(&router, "POST", "/api/v1/tasks/PROJ-1/prs/link", Some(body)).await;
        assert_eq!(s, StatusCode::CONFLICT);
        assert!(v["error"].as_str().unwrap().contains("boom"));
    }

    #[tokio::test]
    async fn unlinking_deletes_and_the_pr_is_not_detected_again() {
        let (router, app) = setup(fake()).await;
        let key = TaskKey::new("PROJ-1");
        pr_loop::check_task(&app, &key, true).await;
        let id = app.store.prs_of(&key).await.unwrap()[0].id;

        let (s, _) = call(&router, "DELETE", "/api/v1/tasks/PROJ-1/prs/9999", None).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        let uri = format!("/api/v1/tasks/PROJ-1/prs/{id}");
        let (s, _) = call(&router, "DELETE", &uri, None).await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        assert!(app.store.prs_of(&key).await.unwrap().is_empty());

        pr_loop::check_task(&app, &key, true).await;
        assert!(app.store.prs_of(&key).await.unwrap().is_empty());
    }
}
