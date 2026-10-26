//! HTTP routes of a task's pull requests: list, badges, refresh, link and unlink, and
//! (in the submodules) opening them and what to do with their notices.

mod dto;
pub(crate) mod forward;
pub mod merge;
mod open;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use delilah_core::TaskKey;

use self::dto::{LinkRequest, PrView, Problem, TaskPrs};
pub use self::forward::forward;
pub use self::open::{open, proposal};
pub(crate) use self::open::{open_for, proposal_for};
use crate::api::ApiError;
use crate::events::Topic;
use crate::gh::{AccessProblem, classify};
use crate::pr_loop::{self, LinkError};
use crate::pr_types::PrSummary;
use crate::prs;
use crate::state::AppState;
use crate::store::StoreError;
use crate::store_prs::PrRow;

impl From<LinkError> for ApiError {
    fn from(e: LinkError) -> Self {
        match e {
            LinkError::NotAPr => ApiError::bad_request("that is not the URL of a pull request"),
            LinkError::Gh(e) => ApiError::conflict(e.to_string()),
            LinkError::NotFromTask => {
                ApiError::conflict("the PR is not from any repo of this task")
            }
            LinkError::Store(e) => e.into(),
        }
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

/// The login `gh` has on the host of the task's PRs: the first one's, or the first repo
/// of its workspace when it has none yet. `None` if anything fails; when the lookup of
/// the login fails for lack of access, that is the problem to show.
async fn account_of(
    app: &AppState,
    key: &TaskKey,
    rows: &[PrRow],
) -> (Option<String>, Option<Problem>) {
    let host = match rows.first() {
        Some(row) => row.host.clone(),
        None => {
            let Some((_, ws)) = app.store.workspace(key).await.ok().flatten() else {
                return (None, None);
            };
            let Some((_, dir)) = ws.repos().into_iter().next() else {
                return (None, None);
            };
            match app.gh.host_of_dir(&dir).await {
                Ok(host) => host,
                Err(_) => return (None, None),
            }
        }
    };
    match app.gh.user(&host).await {
        Ok(login) => (Some(login), None),
        Err(e) => {
            let found = classify(&e);
            if !matches!(
                found,
                AccessProblem::SignedOut | AccessProblem::Missing | AccessProblem::NoAccess
            ) {
                return (None, None);
            }
            let problem = Problem {
                kind: found.kind().to_string(),
                message: e.to_string(),
            };
            (None, Some(problem))
        }
    }
}

/// What stops the PRs of the task: the problem of a check, else the one found while
/// looking for the PR in its project.
async fn problem_of(app: &AppState, key: &TaskKey, rows: &[PrRow]) -> Option<Problem> {
    if let Some(row) = rows.iter().find(|r| r.error_kind.is_some()) {
        return Some(Problem {
            kind: row.error_kind.clone()?,
            message: row.error.clone().unwrap_or_default(),
        });
    }
    let project = app.store.get_task(key).await.ok().flatten()?.project?;
    let found = pr_loop::access::problem_of(app, &project).await?;
    Some(Problem {
        kind: found.kind,
        message: found.message,
    })
}

pub async fn task_prs(
    State(app): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<TaskPrs>, ApiError> {
    let key = known_task(&app, &key).await?;
    let rows = app.store.prs_of(&key).await?;
    let (account, lookup_problem) = account_of(&app, &key, &rows).await;
    let problem = match problem_of(&app, &key, &rows).await {
        Some(p) => Some(p),
        None => lookup_problem,
    };
    Ok(Json(TaskPrs {
        account,
        problem,
        prs: rows.iter().map(PrView::from).collect(),
    }))
}

/// Discards a notice without telling the agent: writes the dismissal of its inbox item
/// (`inbox_dismissals`, the one record, so `/inbox/restore` undoes it too). Idempotent:
/// 204 for a notice dismissed already, 404 only if it never existed.
pub async fn dismiss(
    State(app): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let notice = app
        .store
        .notice(id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("notice {id} does not exist")))?;
    // A kind the inbox never lists has nothing to dismiss.
    if let Some(item) = crate::inbox::notice_item_id(&notice) {
        app.store.dismiss(&[(item, notice.task_key)]).await?;
        app.emit(Topic::Inbox);
    }
    Ok(StatusCode::NO_CONTENT)
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
    // A check already running will read what the user wants refreshed.
    if !pr_loop::is_checking(&app, &key) {
        tokio::spawn(async move { pr_loop::check_task(&app, &key, true).await });
    }
    Ok(StatusCode::ACCEPTED)
}
pub async fn link(
    State(app): State<AppState>,
    Path(key): Path<String>,
    Json(req): Json<LinkRequest>,
) -> Result<Json<PrView>, ApiError> {
    let key = known_task(&app, &key).await?;
    let row = pr_loop::link_by_url(&app, &key, req.url.trim(), "pasted").await?;
    Ok(Json(PrView::from(&row)))
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
mod support {
    use std::sync::Arc;

    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    use crate::gh::fake::FakeGh;
    use crate::pr_testkit::{TempDir, task_with_workspace};
    use crate::state::AppState;

    pub(super) fn fake() -> Arc<FakeGh> {
        Arc::new(FakeGh::happy().with_user("me"))
    }

    pub(super) async fn setup(gh: Arc<FakeGh>) -> (Router, AppState, TempDir) {
        let (app, _, dir) = task_with_workspace(gh, None).await;
        (crate::api::router(app.clone()), app, dir)
    }

    pub(super) async fn call(
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
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use serde_json::{Value, json};

    use super::*;
    use crate::gh::GhError;
    use crate::gh::fake::{FakeGh, PR_URL};
    use crate::pr_api::support::{call, fake, setup};

    const SIGNED_OUT: &str = "To get started with GitHub CLI, please run: gh auth login";

    #[tokio::test]
    async fn lists_the_prs_of_a_task_with_account_and_snapshot_view() {
        let (router, app, _dir) = setup(fake()).await;
        let key = TaskKey::new("PROJ-1");
        let (s, v) = call(&router, "GET", "/api/v1/tasks/PROJ-1/prs", None).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v["prs"], json!([]));
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
    async fn the_account_shows_before_the_task_has_a_pr() {
        let (router, _app, _dir) = setup(fake()).await;
        let (_, v) = call(&router, "GET", "/api/v1/tasks/PROJ-1/prs", None).await;
        assert_eq!(v["prs"], json!([]));
        assert_eq!(v["account"], "me");

        // Null when anything fails: no repo to ask about, or gh not signed in.
        let (router, _app, _dir) = setup(Arc::new(FakeGh::happy())).await;
        let (_, v) = call(&router, "GET", "/api/v1/tasks/PROJ-1/prs", None).await;
        assert_eq!(v["account"], Value::Null);
    }

    #[tokio::test]
    async fn shows_the_problem_of_a_failed_check_by_its_stored_kind() {
        let (router, app, _dir) = setup(fake()).await;
        let key = TaskKey::new("PROJ-1");
        pr_loop::check_task(&app, &key, true).await;
        let id = app.store.prs_of(&key).await.unwrap()[0].id;
        // The text says nothing of the kind: it is not read back from the message.
        app.store
            .set_pr_error(id, Some(("signed-out", "odd words")))
            .await
            .unwrap();
        let (_, v) = call(&router, "GET", "/api/v1/tasks/PROJ-1/prs", None).await;
        assert_eq!(v["problem"]["kind"], "signed-out");
        assert_eq!(v["problem"]["message"], "odd words");
    }

    #[tokio::test]
    async fn a_signed_out_account_lookup_is_the_problem_when_nothing_else_is() {
        let gh = Arc::new(FakeGh::happy().on(
            &["api", "user"],
            Err(GhError::Failed {
                code: Some(4),
                stderr: SIGNED_OUT.into(),
            }),
        ));
        let (router, _app, _dir) = setup(gh).await;
        let (_, v) = call(&router, "GET", "/api/v1/tasks/PROJ-1/prs", None).await;
        assert_eq!(v["account"], Value::Null);
        assert_eq!(v["problem"]["kind"], "signed-out");
        assert!(
            v["problem"]["message"]
                .as_str()
                .unwrap()
                .contains("gh auth login")
        );
    }

    #[tokio::test]
    async fn a_missing_gh_is_reported_as_missing_and_clears_on_success() {
        let missing = Arc::new(FakeGh::new().on(&["repo", "view"], Err(GhError::NotFound)));
        let (router, app, _dir) = setup(missing).await;
        let key = TaskKey::new("PROJ-1");
        pr_loop::check_task(&app, &key, true).await;
        let (_, v) = call(&router, "GET", "/api/v1/tasks/PROJ-1/prs", None).await;
        assert_eq!(v["problem"]["kind"], "missing");

        // With a PR already linked, the failed check carries the kind itself.
        let app = app.with_gh(fake());
        let router = crate::api::router(app.clone());
        pr_loop::check_task(&app, &key, true).await;
        let (_, v) = call(&router, "GET", "/api/v1/tasks/PROJ-1/prs", None).await;
        assert_eq!(v["problem"], Value::Null);
        let app = app.with_gh(Arc::new(
            FakeGh::new().on(&["api", "graphql"], Err(GhError::NotFound)),
        ));
        let router = crate::api::router(app.clone());
        pr_loop::check_task(&app, &key, true).await;
        let (_, v) = call(&router, "GET", "/api/v1/tasks/PROJ-1/prs", None).await;
        assert_eq!(v["problem"]["kind"], "missing");
        let app = app.with_gh(fake());
        let router = crate::api::router(app.clone());
        pr_loop::check_task(&app, &key, true).await;
        let (_, v) = call(&router, "GET", "/api/v1/tasks/PROJ-1/prs", None).await;
        assert_eq!(v["problem"], Value::Null);
    }

    #[tokio::test]
    async fn a_task_without_rows_shows_the_detection_problem_of_its_project() {
        let signed_out = Arc::new(FakeGh::new().on(
            &["repo", "view"],
            Err(GhError::Failed {
                code: Some(4),
                stderr: SIGNED_OUT.into(),
            }),
        ));
        let (router, app, _dir) = setup(signed_out).await;
        let key = TaskKey::new("PROJ-1");
        pr_loop::check_task(&app, &key, true).await;
        let (_, v) = call(&router, "GET", "/api/v1/tasks/PROJ-1/prs", None).await;
        assert_eq!(v["prs"], json!([]));
        assert_eq!(v["problem"]["kind"], "signed-out");
        assert!(
            v["problem"]["message"]
                .as_str()
                .unwrap()
                .contains("gh auth login")
        );

        let app = app.with_gh(fake());
        let router = crate::api::router(app.clone());
        pr_loop::check_task(&app, &key, true).await;
        let (_, v) = call(&router, "GET", "/api/v1/tasks/PROJ-1/prs", None).await;
        assert_eq!(v["problem"], Value::Null);
    }

    #[tokio::test]
    async fn summaries_one_per_task_with_prs() {
        let (router, app, _dir) = setup(fake()).await;
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
        let (router, app, _dir) = setup(fake()).await;
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
    async fn refresh_does_not_start_a_check_when_one_is_running() {
        let gh = fake();
        let (router, app, _dir) = setup(gh.clone()).await;
        let key = TaskKey::new("PROJ-1");
        let running = pr_loop::claim(&app, &key).unwrap();
        let (s, _) = call(&router, "POST", "/api/v1/tasks/PROJ-1/prs/refresh", None).await;
        assert_eq!(s, StatusCode::ACCEPTED);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(gh.calls().is_empty());
        drop(running);
    }

    #[tokio::test]
    async fn linking_a_bad_url_is_400_and_a_good_one_moves_to_review() {
        let (router, app, _dir) = setup(fake()).await;
        let body = json!({ "url": "https://example.com/nope" });
        let (s, _) = call(&router, "POST", "/api/v1/tasks/PROJ-1/prs/link", Some(body)).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);

        let body = json!({ "url": format!("{PR_URL}?tab=files") });
        let (s, v) = call(&router, "POST", "/api/v1/tasks/PROJ-1/prs/link", Some(body)).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v["linked_by"], "pasted");
        assert_eq!(v["repo"], ".");
        assert_eq!(v["url"], PR_URL);
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
    async fn linking_a_pr_writes_one_pr_opened_aviso_that_both_dismiss_routes_agree_on() {
        let (router, app, _dir) = setup(fake()).await;
        let link = || async {
            let body = json!({ "url": PR_URL });
            call(&router, "POST", "/api/v1/tasks/PROJ-1/prs/link", Some(body)).await
        };
        let opened = |items: &Value| {
            items
                .as_array()
                .unwrap()
                .iter()
                .find(|i| i["kind"] == "pr-opened")
                .cloned()
        };
        link().await;
        let (_, items) = call(&router, "GET", "/api/v1/inbox", None).await;
        let item = opened(&items).expect("a pr-opened item");
        let id = item["id"].as_str().unwrap().to_string();
        let notice = item["pr"]["notice"].as_i64().unwrap();
        assert_eq!(item["pr"]["url"], PR_URL);

        let uri = format!("/api/v1/pr-notices/{notice}/dismiss");
        assert_eq!(
            call(&router, "POST", &uri, None).await.0,
            StatusCode::NO_CONTENT
        );
        assert!(app.store.dismissed().await.unwrap().contains(&id));
        let (_, items) = call(&router, "GET", "/api/v1/inbox", None).await;
        assert!(opened(&items).is_none());

        // Linking it again is not news.
        link().await;
        let (_, items) = call(&router, "GET", "/api/v1/inbox", None).await;
        assert!(opened(&items).is_none());

        let body = json!({ "ids": [id] });
        let (s, _) = call(&router, "POST", "/api/v1/inbox/restore", Some(body)).await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        let (_, items) = call(&router, "GET", "/api/v1/inbox", None).await;
        assert!(opened(&items).is_some());
    }

    #[tokio::test]
    async fn linking_a_pr_of_another_repo_is_409() {
        let (router, _app, _dir) = setup(fake()).await;
        let body = json!({ "url": "https://github.com/other/repo/pull/3" });
        let (s, v) = call(&router, "POST", "/api/v1/tasks/PROJ-1/prs/link", Some(body)).await;
        assert_eq!(s, StatusCode::CONFLICT);
        assert!(v["error"].as_str().unwrap().contains("not from any repo"));
    }

    #[tokio::test]
    async fn linking_a_pr_gh_cannot_read_is_409() {
        let failing = Arc::new(FakeGh::new().on(
            &["api", "graphql"],
            Err(GhError::Failed {
                code: Some(1),
                stderr: "boom".into(),
            }),
        ));
        let (router, _, _dir) = setup(failing).await;
        let body = json!({ "url": PR_URL });
        let (s, v) = call(&router, "POST", "/api/v1/tasks/PROJ-1/prs/link", Some(body)).await;
        assert_eq!(s, StatusCode::CONFLICT);
        assert!(v["error"].as_str().unwrap().contains("boom"));
    }

    #[tokio::test]
    async fn unlinking_deletes_and_the_pr_is_not_detected_again() {
        let (router, app, _dir) = setup(fake()).await;
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
