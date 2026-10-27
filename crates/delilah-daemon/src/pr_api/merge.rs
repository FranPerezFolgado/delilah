//! `POST /tasks/{key}/prs/{id}/merge`: merges a pull request through `gh`.
//!
//! Before anything runs: the PR must be the task's, open, not a draft, with passing
//! checks (or none), the repository's merge methods must be known and allow the one
//! asked for, and the `gh` account signed in on the PR's host must be the one the PR tab
//! showed (credentials differ between the personal and the work account).

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use delilah_core::TaskKey;
use serde::Deserialize;

use super::dto::{PrView, TaskContext};
use super::known_task;
use crate::api::ApiError;
use crate::events::Topic;
use crate::gh::classify;
use crate::pr_loop;
use crate::pr_types::{MergeMethod, PrState, Rollup};
use crate::prs;
use crate::state::AppState;

#[derive(Deserialize, Default)]
struct MergeRequest {
    #[serde(default)]
    method: Option<MergeMethod>,
}

/// A 409 the client tells apart by `reason`.
fn refuse(message: impl Into<String>, reason: &str) -> ApiError {
    ApiError::conflict_reason(message, reason)
}

pub async fn merge(
    State(app): State<AppState>,
    Path((key, id)): Path<(String, i64)>,
    body: Bytes,
) -> Result<Json<PrView>, ApiError> {
    let key = known_task(&app, &key).await?;
    let request: MergeRequest = if body.iter().all(u8::is_ascii_whitespace) {
        MergeRequest::default()
    } else {
        serde_json::from_slice(&body).map_err(|e| ApiError::bad_request(e.to_string()))?
    };
    let row = app
        .store
        .pr(id)
        .await?
        .filter(|r| r.task_key == key.as_str())
        .ok_or_else(|| ApiError::not_found(format!("{key} has no PR {id}")))?;

    // The account first: nothing else is worth saying with the wrong credentials.
    let shown = app.gh.cached_user(&row.host);
    match app.gh.user_fresh(&row.host).await {
        Ok(now) => match shown {
            None => {
                return Err(refuse(
                    format!(
                        "the gh account on {} is not known yet (now {now}): check the PR tab and try again",
                        row.host
                    ),
                    "account-unknown",
                ));
            }
            Some(shown) if shown != now => {
                return Err(refuse(
                    format!(
                        "the gh account on {} changed to {now}: check the PR tab and try again",
                        row.host
                    ),
                    "account-changed",
                ));
            }
            Some(_) => {}
        },
        Err(e) => return Err(refuse(e.to_string(), classify(&e).kind())),
    }

    let snapshot = row
        .snapshot
        .as_ref()
        .ok_or_else(|| refuse("the PR has not been read yet", "not-read"))?;
    if snapshot.state != PrState::Open {
        return Err(refuse("the PR is not open", "not-open"));
    }
    if snapshot.draft {
        return Err(refuse("the PR is a draft", "draft"));
    }
    if !matches!(
        prs::ci_outcome(&snapshot.checks),
        Rollup::Success | Rollup::None
    ) {
        return Err(refuse("the checks of the PR are not passing", "checks"));
    }
    let ctx = TaskContext::of(&app, &key).await?;
    let method = match request.method {
        _ if snapshot.merge_methods.is_empty() => {
            return Err(refuse(
                "the merge methods the repository allows are unknown",
                "merge-methods-unknown",
            ));
        }
        Some(m) if snapshot.merge_methods.contains(&m) => m,
        Some(m) => {
            return Err(refuse(
                format!("the repository does not allow the {m:?} method"),
                "method-not-allowed",
            ));
        }
        None => ctx
            .method(&snapshot.merge_methods)
            .unwrap_or(snapshot.merge_methods[0]),
    };

    if let Err(e) = app.gh.pr_merge(&row.url, method).await {
        let problem = classify(&e);
        return Err(ApiError {
            status: StatusCode::BAD_GATEWAY,
            message: e.to_string(),
            unpushed: false,
            fields: Vec::new(),
            resume_at: None,
            reason: Some(problem.kind().to_string()),
            refusal: None,
        });
    }

    // Read the PR again: archive-on-merge runs from that check, as for any merge.
    refresh(&app, &key).await;
    app.emit(Topic::Prs);
    app.emit(Topic::Tasks);
    let row = app.store.pr(id).await?.unwrap_or(row);
    Ok(Json(PrView::with(&row, &ctx)))
}

async fn refresh(app: &AppState, key: &TaskKey) {
    pr_loop::check_task(app, key, true).await;
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::{Value, json};

    use super::*;
    use crate::gh::GhError;
    use crate::gh::fake::{FakeGh, PR_LIST, REPO_VIEW, bare_snapshot, ci_snapshot};
    use crate::pr_api::support::{call, setup};

    const KEY: &str = "PROJ-1";

    /// A repo whose PR reads as `first` and, after one read, as `then`; signed in as `me`.
    fn gh(first: &str, then: &str) -> FakeGh {
        FakeGh::new()
            .on(&["repo", "view"], Ok(REPO_VIEW))
            .on(&["pr", "list"], Ok(PR_LIST))
            .on(&["api", "graphql"], Ok(first))
            .on(&["api", "graphql"], Ok(then))
            .with_user("me")
            .on(&["pr", "merge"], Ok(""))
    }

    fn open_green() -> String {
        ci_snapshot("OPEN", "abc", "SUCCESS")
    }

    /// The router with the PR read once, and its id.
    async fn ready(gh: FakeGh) -> (axum::Router, AppState, Arc<FakeGh>, i64, impl Sized) {
        let gh = Arc::new(gh);
        let (router, app, dir) = setup(gh.clone()).await;
        pr_loop::check_task(&app, &TaskKey::new(KEY), true).await;
        let id = app.store.prs_of(&TaskKey::new(KEY)).await.unwrap()[0].id;
        (router, app, gh, id, dir)
    }

    fn uri(id: i64) -> String {
        format!("/api/v1/tasks/{KEY}/prs/{id}/merge")
    }

    fn merges(gh: &FakeGh) -> Vec<Vec<String>> {
        gh.calls()
            .into_iter()
            .filter(|c| c.starts_with(&["pr".to_string(), "merge".to_string()]))
            .collect()
    }

    #[tokio::test]
    async fn merging_a_passing_pr_calls_gh_and_refreshes() {
        let merged = ci_snapshot("MERGED", "abc", "SUCCESS");
        let (router, app, gh, id, _dir) = ready(gh(&open_green(), &merged)).await;
        let mut events = app.events.subscribe();
        let (s, v) = call(&router, "POST", &uri(id), None).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert_eq!(v["snapshot"]["state"], "merged");
        assert_eq!(
            merges(&gh),
            [[
                "pr",
                "merge",
                "--merge",
                "--",
                "https://github.com/o/r/pull/7"
            ]]
        );
        let mut topics = vec![];
        while let Ok(t) = events.try_recv() {
            topics.push(t);
        }
        assert!(topics.contains(&Topic::Prs) && topics.contains(&Topic::Tasks));
    }

    #[tokio::test]
    async fn a_draft_or_failing_pr_is_409() {
        for (snapshot, reason) in [
            (
                open_green().replace("\"isDraft\":false", "\"isDraft\":true"),
                "draft",
            ),
            (ci_snapshot("OPEN", "abc", "FAILURE"), "checks"),
            (ci_snapshot("CLOSED", "abc", "SUCCESS"), "not-open"),
        ] {
            let (router, _app, gh, id, _dir) = ready(gh(&snapshot, &snapshot)).await;
            let (s, v) = call(&router, "POST", &uri(id), None).await;
            assert_eq!(s, StatusCode::CONFLICT, "{reason}");
            assert_eq!(v["reason"], reason);
            assert!(merges(&gh).is_empty());
        }
    }

    #[tokio::test]
    async fn unknown_merge_methods_are_409() {
        let none = bare_snapshot("OPEN", false);
        let (router, _app, gh, id, _dir) = ready(gh(&none, &none)).await;
        let (s, v) = call(&router, "POST", &uri(id), None).await;
        assert_eq!(s, StatusCode::CONFLICT);
        assert_eq!(v["reason"], "merge-methods-unknown");

        // A method the repository does not allow (the fake allows merge and squash).
        let green = open_green();
        let (router, _app, gh2, id, _dir) = ready(gh_with(&green)).await;
        let (s, v) = call(&router, "POST", &uri(id), Some(json!({"method": "rebase"}))).await;
        assert_eq!(s, StatusCode::CONFLICT);
        assert_eq!(v["reason"], "method-not-allowed");
        assert!(merges(&gh).is_empty() && merges(&gh2).is_empty());
    }

    fn gh_with(snapshot: &str) -> FakeGh {
        gh(snapshot, snapshot)
    }

    #[tokio::test]
    async fn the_default_method_is_the_first_the_repo_allows() {
        let green = open_green().replace(
            "\"mergeCommitAllowed\":true",
            "\"mergeCommitAllowed\":false",
        );
        let (router, _app, gh, id, _dir) = ready(gh_with(&green)).await;
        let (s, _) = call(&router, "POST", &uri(id), None).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(merges(&gh)[0][2], "--squash");

        // An explicit, allowed method wins over the default.
        let (router, _app, gh, id, _dir) = ready(gh_with(&open_green())).await;
        let (s, _) = call(&router, "POST", &uri(id), Some(json!({"method": "squash"}))).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(merges(&gh)[0][2], "--squash");

        // A method that does not exist is a bad request, not the default.
        let (router, _app, gh, id, _dir) = ready(gh_with(&open_green())).await;
        let (s, _) = call(&router, "POST", &uri(id), Some(json!({"method": "yolo"}))).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert!(merges(&gh).is_empty());
    }

    #[tokio::test]
    async fn the_gh_account_is_checked_before_merging() {
        // The account the tab showed was `me`; now `gh` answers `other`.
        let green = open_green();
        let fake = gh_with(&green).on(&["api", "user"], Ok("other\n"));
        let (router, _app, gh, id, _dir) = ready(fake).await;
        // The check of `ready` asked who `me` is (cached); the next lookup is `other`.
        let (s, v) = call(&router, "POST", &uri(id), None).await;
        assert_eq!(s, StatusCode::CONFLICT, "{v}");
        assert_eq!(v["reason"], "account-changed");
        assert!(merges(&gh).is_empty());

        // No account remembered (the PR tab never read it): refused, nothing run.
        let unread = FakeGh::new()
            .on(&["repo", "view"], Ok(REPO_VIEW))
            .on(&["pr", "list"], Ok(PR_LIST))
            .on(&["api", "graphql"], Ok(&green))
            .on(&["pr", "merge"], Ok(""))
            .on(
                &["api", "user"],
                Err(GhError::Failed {
                    code: Some(1),
                    stderr: "boom".into(),
                }),
            )
            .on(&["api", "user"], Ok("me\n"));
        let (router, app, gh, id, _dir) = ready(unread).await;
        assert!(app.gh.cached_user("github.com").is_none());
        let (s, v) = call(&router, "POST", &uri(id), None).await;
        assert_eq!(s, StatusCode::CONFLICT, "{v}");
        assert_eq!(v["reason"], "account-unknown");
        assert!(merges(&gh).is_empty());

        // Signed out: the problem's kind, nothing run.
        let signed_out = FakeGh::new()
            .on(&["repo", "view"], Ok(REPO_VIEW))
            .on(&["pr", "list"], Ok(PR_LIST))
            .on(&["api", "graphql"], Ok(&green))
            .on(
                &["api", "user"],
                Err(GhError::Failed {
                    code: Some(4),
                    stderr: "run: gh auth login".into(),
                }),
            );
        let (router, _app, gh, id, _dir) = ready(signed_out).await;
        let (s, v) = call(&router, "POST", &uri(id), None).await;
        assert_eq!(s, StatusCode::CONFLICT);
        assert_eq!(v["reason"], "signed-out");
        assert!(merges(&gh).is_empty());
    }

    #[tokio::test]
    async fn unknown_fields_in_the_body_are_tolerated() {
        let (router, _app, gh, id, _dir) = ready(gh_with(&open_green())).await;
        let body = json!({"method": "squash", "extra": 1});
        let (s, _) = call(&router, "POST", &uri(id), Some(body)).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(merges(&gh).len(), 1);
    }

    #[tokio::test]
    async fn a_gh_failure_is_classified() {
        let green = open_green();
        let fake = FakeGh::new()
            .on(&["repo", "view"], Ok(REPO_VIEW))
            .on(&["pr", "list"], Ok(PR_LIST))
            .on(&["api", "graphql"], Ok(&green))
            .with_user("me")
            .on(
                &["pr", "merge"],
                Err(GhError::Failed {
                    code: Some(1),
                    stderr: "gh: Resource not accessible (HTTP 403)".into(),
                }),
            );
        let (router, _app, _gh, id, _dir) = ready(fake).await;
        let (s, v) = call(&router, "POST", &uri(id), None).await;
        assert_eq!(s, StatusCode::BAD_GATEWAY);
        assert_eq!(v["reason"], "no-access");
        assert!(v["error"].as_str().unwrap().contains("403"));
    }

    #[tokio::test]
    async fn another_tasks_pr_or_unknown_ids_are_404() {
        let green = open_green();
        let (router, _app, _gh, id, _dir) = ready(gh_with(&green)).await;
        let (s, _) = call(&router, "POST", &uri(id + 100), None).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        let wrong = format!("/api/v1/tasks/NOPE-1/prs/{id}/merge");
        let (s, v) = call(&router, "POST", &wrong, None).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        let _: Value = v;
    }

    /// Makes `PROJ-1` a subtask of a started parent task on `feature/P-1`.
    async fn as_subtask(app: &AppState) {
        use delilah_core::hierarchy::TaskKind;
        let parent = TaskKey::new("P-1");
        let mut p = delilah_core::Task::new(parent.clone(), "parent", "project");
        p.kind = TaskKind::ParentTask;
        app.store.create_task(&p).await.unwrap();
        let ws = delilah_git::Workspace {
            path: "/tmp/parent-ws".into(),
            branch: "feature/P-1".into(),
            kind: delilah_core::ProjectKind::Repo,
            submodules: vec![],
        };
        app.store
            .save_workspace(&parent, std::path::Path::new("/parent"), &ws)
            .await
            .unwrap();
        app.store
            .set_kind(&TaskKey::new(KEY), TaskKind::Subtask, Some(&parent))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn a_subtask_merges_by_squash_unless_the_request_says_otherwise() {
        let merged = ci_snapshot("MERGED", "abc", "SUCCESS");
        let (router, app, gh, id, _dir) = ready(gh(&open_green(), &merged)).await;
        as_subtask(&app).await;
        let (s, v) = call(&router, "POST", &uri(id), None).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert_eq!(merges(&gh)[0][2], "--squash");
    }

    #[tokio::test]
    async fn a_subtask_falls_back_to_the_first_allowed_method_when_squash_is_not() {
        let green = open_green().replace(
            "\"squashMergeAllowed\":true",
            "\"squashMergeAllowed\":false",
        );
        let (router, app, gh, id, _dir) = ready(gh_with(&green)).await;
        as_subtask(&app).await;
        let (s, v) = call(&router, "POST", &uri(id), None).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert_eq!(merges(&gh)[0][2], "--merge");
    }

    #[tokio::test]
    async fn the_pr_view_carries_the_method_and_the_bases() {
        let (_router, app, _gh, id, _dir) = ready(gh_with(&open_green())).await;
        let key = TaskKey::new(KEY);
        let view = async |app: &AppState| {
            let row = app.store.pr(id).await.unwrap().unwrap();
            let Ok(ctx) = TaskContext::of(app, &key).await else {
                panic!("no context")
            };
            serde_json::to_value(PrView::with(&row, &ctx)).unwrap()
        };
        let v = view(&app).await;
        assert_eq!(v["method"], "merge");
        assert!(v["expected_base"].is_null());
        assert_eq!(v["base_branch"], v["snapshot"]["base"]);
        as_subtask(&app).await;
        let v = view(&app).await;
        assert_eq!(v["method"], "squash");
        assert_eq!(v["expected_base"], "feature/P-1");
    }

    #[tokio::test]
    async fn get_prs_returns_method_and_expected_base_for_a_subtask() {
        let (router, app, _gh, _id, _dir) = ready(gh_with(&open_green())).await;
        let uri = format!("/api/v1/tasks/{KEY}/prs");
        let (_, v) = call(&router, "GET", &uri, None).await;
        assert_eq!(v["prs"][0]["method"], "merge");
        assert!(v["prs"][0]["expected_base"].is_null());
        as_subtask(&app).await;
        let (_, v) = call(&router, "GET", &uri, None).await;
        assert_eq!(v["prs"][0]["method"], "squash");
        assert_eq!(v["prs"][0]["expected_base"], "feature/P-1");
    }
}
