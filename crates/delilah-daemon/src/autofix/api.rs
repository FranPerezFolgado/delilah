//! HTTP handlers of CI autofix (`POST /tasks/{key}/autofix`).

use axum::Json;
use axum::extract::{Path as AxPath, State};
use delilah_core::{Task, TaskKey};
use serde::Deserialize;

use crate::api::ApiError;
use crate::state::AppState;

/// `{ "action": "on" | "off" | "retry", "repo": "." }`; `repo` only for `retry`.
#[derive(Debug, Deserialize)]
pub(crate) struct AutofixReq {
    action: String,
    #[serde(default)]
    repo: Option<String>,
}

/// `POST /api/v1/tasks/{key}/autofix`: `on`, `off` or `retry` (of one PR).
pub(crate) async fn act(
    State(app): State<AppState>,
    AxPath(key): AxPath<String>,
    Json(req): Json<AutofixReq>,
) -> Result<Json<Task>, ApiError> {
    let key = TaskKey::new(key);
    match req.action.as_str() {
        "on" => super::set(&app, &key, true).await?,
        "off" => super::set(&app, &key, false).await?,
        "retry" => super::retry(&app, &key, req.repo.as_deref()).await?,
        other => return Err(ApiError::bad_request(format!("unknown action `{other}`"))),
    }
    let task = app
        .store
        .get_task(&key)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("no task {key}")))?;
    Ok(Json(task))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use delilah_core::SpecPhase;
    use http_body_util::BodyExt;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use crate::autofix::{PrAutofix, State, StopReason, claim};
    use crate::gh::fake::{FakeGh, PR_LIST, REPO_VIEW, ci_snapshot};
    use crate::pr_loop;
    use crate::pr_testkit::task_with_workspace;
    use crate::sessions::LiveState;

    use super::*;

    async fn post(app: &AppState, key: &str, body: Value) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/tasks/{key}/autofix"))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let res = crate::api::router(app.clone()).oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, value)
    }

    fn gh() -> Arc<FakeGh> {
        Arc::new(
            FakeGh::new()
                .on(&["repo", "view"], Ok(REPO_VIEW))
                .on(&["pr", "list"], Ok(PR_LIST))
                .on(
                    &["api", "graphql"],
                    Ok(&ci_snapshot("OPEN", "abc", "FAILURE")),
                )
                .with_user("me"),
        )
    }

    /// A task with its PR linked (head `abc`, failing), autofix off, and a `cat` session
    /// waiting for input.
    async fn setup() -> (AppState, TaskKey, crate::pr_testkit::TempDir) {
        use delilah_agents::pty::PtySession;
        use portable_pty::CommandBuilder;

        let (app, key, dir) = task_with_workspace(gh(), Some(SpecPhase::Review)).await;
        pr_loop::check_task(&app, &key, true).await;
        let mut cmd = CommandBuilder::new("cat");
        cmd.args(["-u"]);
        let pty = PtySession::spawn(cmd, 200, 24, None).unwrap();
        app.sessions.insert(
            "s-1",
            "PROJ-1",
            crate::store_sessions::SessionKind::Claude,
            true,
            pty,
        );
        app.sessions
            .primary("PROJ-1")
            .unwrap()
            .set_state(LiveState::Waiting);
        (app, key, dir)
    }

    async fn stored(app: &AppState, key: &TaskKey) -> Option<PrAutofix> {
        app.store.prs_of(key).await.unwrap().remove(0).autofix
    }

    async fn write(app: &AppState, key: &TaskKey, next: PrAutofix) {
        let row = app.store.prs_of(key).await.unwrap().remove(0);
        assert!(
            claim(app, row.id, row.autofix.as_ref(), &next)
                .await
                .unwrap()
        );
    }

    fn state(state: State, attempts: u32, sha: &str) -> PrAutofix {
        PrAutofix {
            state,
            attempts,
            sha: Some(sha.into()),
            session: Some("s-1".into()),
            sent_at: Some(chrono::Utc::now().to_rfc3339()),
            reason: (state == State::Stopped).then_some(StopReason::Limit),
        }
    }

    #[tokio::test]
    async fn on_with_a_failed_finished_head_and_an_idle_agent_sends_attempt_one() {
        let (app, key, _dir) = setup().await;
        let (status, task) = post(&app, key.as_str(), json!({"action": "on"})).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(task["ci_autofix"], true);
        let a = stored(&app, &key).await.unwrap();
        assert_eq!((a.state, a.attempts), (State::Fixing, 1));
        assert_eq!(a.sha.as_deref(), Some("abc"));
    }

    #[tokio::test]
    async fn on_resets_stopped_prs_and_their_notice() {
        let (app, key, _dir) = setup().await;
        write(&app, &key, state(State::Stopped, 3, "old")).await;
        let repo = app.store.prs_of(&key).await.unwrap().remove(0).repo;
        app.store
            .insert_notice(
                &key,
                &crate::store_prs::NewNotice {
                    repo: repo.clone(),
                    kind: "autofix-stopped".into(),
                    cause: "old:limit:x".into(),
                    text: "stopped".into(),
                    detail: None,
                },
            )
            .await
            .unwrap();
        let (status, _) = post(&app, key.as_str(), json!({"action": "on"})).await;
        assert_eq!(status, StatusCode::OK);
        // Reset to attempt 0, then the red head `abc` (not the sent `old`) goes out.
        let a = stored(&app, &key).await.unwrap();
        assert_eq!((a.state, a.attempts, a.reason), (State::Fixing, 1, None));
        let open = app.store.open_notices().await.unwrap();
        assert!(open.iter().all(|n| n.kind != "autofix-stopped"), "{open:?}");
    }

    #[tokio::test]
    async fn off_while_fixing_goes_back_to_watching_and_leaves_the_session() {
        let (app, key, _dir) = setup().await;
        post(&app, key.as_str(), json!({"action": "on"})).await;
        assert_eq!(stored(&app, &key).await.unwrap().state, State::Fixing);
        let (status, task) = post(&app, key.as_str(), json!({"action": "off"})).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(task["ci_autofix"], false);
        let a = stored(&app, &key).await.unwrap();
        assert_eq!((a.state, a.attempts), (State::Watching, 0));
        let live = app.sessions.primary("PROJ-1").expect("session untouched");
        assert!(!live.runtime.has_exited());
    }

    #[tokio::test]
    async fn retry_clears_the_sha_and_sends_the_current_failure() {
        let (app, key, _dir) = setup().await;
        write(&app, &key, state(State::Stopped, 3, "abc")).await;
        let repo = app.store.prs_of(&key).await.unwrap().remove(0).repo;
        let (status, task) =
            post(&app, key.as_str(), json!({"action": "retry", "repo": repo})).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(task["ci_autofix"], true);
        let a = stored(&app, &key).await.unwrap();
        assert_eq!((a.state, a.attempts), (State::Fixing, 1));
    }

    #[tokio::test]
    async fn errors_follow_the_contract() {
        let (app, key, _dir) = setup().await;
        let (status, _) = post(&app, "NOPE-1", json!({"action": "on"})).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _) = post(&app, key.as_str(), json!({"action": "dance"})).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, body) = post(&app, key.as_str(), json!({"action": "retry"})).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["fields"][0]["field"], "repo");
        let (status, _) = post(
            &app,
            key.as_str(),
            json!({"action": "retry", "repo": "nowhere"}),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        // A failed retry does not turn autofix on.
        assert!(!app.store.get_task(&key).await.unwrap().unwrap().ci_autofix);

        app.transition(&key, delilah_core::Event::MarkDone)
            .await
            .unwrap();
        for body in [
            json!({"action": "on"}),
            json!({"action": "off"}),
            json!({"action": "retry", "repo": "x"}),
        ] {
            let (status, body) = post(&app, key.as_str(), body).await;
            assert_eq!(status, StatusCode::CONFLICT);
            assert_eq!(body["reason"], "terminal");
        }
    }

    #[tokio::test]
    async fn patch_ci_autofix_has_the_toggle_side_effects_and_the_409() {
        let (app, key, _dir) = setup().await;
        let router = crate::api::router(app.clone());
        let patch = |body: Value| {
            let router = router.clone();
            let uri = format!("/api/v1/tasks/{key}");
            async move {
                let req = Request::builder()
                    .method("PATCH")
                    .uri(uri)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap();
                router.oneshot(req).await.unwrap().status()
            }
        };
        assert_eq!(patch(json!({"ci_autofix": true})).await, StatusCode::OK);
        assert_eq!(stored(&app, &key).await.unwrap().state, State::Fixing);
        assert_eq!(patch(json!({"ci_autofix": false})).await, StatusCode::OK);
        assert_eq!(stored(&app, &key).await.unwrap().state, State::Watching);
        app.transition(&key, delilah_core::Event::MarkDone)
            .await
            .unwrap();
        assert_eq!(
            patch(json!({"ci_autofix": true})).await,
            StatusCode::CONFLICT
        );
        assert_eq!(patch(json!({"ci_autofix": false})).await, StatusCode::OK);
    }
}
