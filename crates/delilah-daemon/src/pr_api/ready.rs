//! `POST /tasks/{key}/prs/{id}/ready`: marks a draft PR ready for review through `gh`.
//! Only on the user's request; the daemon never does it on its own.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

use super::dto::{PrView, TaskContext};
use super::known_task;
use crate::api::ApiError;
use crate::events::Topic;
use crate::gh::classify;
use crate::pr_loop;
use crate::pr_types::PrState;
use crate::state::AppState;

pub async fn ready(
    State(app): State<AppState>,
    Path((key, id)): Path<(String, i64)>,
) -> Result<Json<PrView>, ApiError> {
    let key = known_task(&app, &key).await?;
    let row = app
        .store
        .pr(id)
        .await?
        .filter(|r| r.task_key == key.as_str())
        .ok_or_else(|| ApiError::not_found(format!("{key} has no PR {id}")))?;
    if row
        .snapshot
        .as_ref()
        .is_some_and(|s| s.state != PrState::Open)
    {
        return Err(ApiError::conflict_reason("the PR is not open", "not-open"));
    }
    if let Err(e) = app.gh.pr_ready(&row.url).await {
        return Err(ApiError {
            status: StatusCode::BAD_GATEWAY,
            message: e.to_string(),
            unpushed: false,
            fields: Vec::new(),
            resume_at: None,
            reason: Some(classify(&e).kind().to_string()),
            refusal: None,
        });
    }
    pr_loop::check_task(&app, &key, true).await;
    app.emit(Topic::Prs);
    let row = app.store.pr(id).await?.unwrap_or(row);
    let ctx = TaskContext::of(&app, &key).await?;
    Ok(Json(PrView::with(&row, &ctx)))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::gh::fake::{FakeGh, PR_LIST, REPO_VIEW, ci_snapshot};
    use crate::pr_api::support::{call, setup};
    use delilah_core::TaskKey;

    #[tokio::test]
    async fn ready_calls_gh_pr_ready_and_only_then() {
        let draft =
            ci_snapshot("OPEN", "abc", "SUCCESS").replace("\"isDraft\":false", "\"isDraft\":true");
        let open = ci_snapshot("OPEN", "abc", "SUCCESS");
        let gh = Arc::new(
            FakeGh::new()
                .on(&["repo", "view"], Ok(REPO_VIEW))
                .on(&["pr", "list"], Ok(PR_LIST))
                .on(&["api", "graphql"], Ok(&draft))
                .on(&["api", "graphql"], Ok(&open))
                .with_user("me")
                .on(&["pr", "ready"], Ok("")),
        );
        let (router, app, _dir) = setup(gh.clone()).await;
        let key = TaskKey::new("PROJ-1");
        pr_loop::check_task(&app, &key, true).await;
        let readies = |gh: &FakeGh| {
            gh.calls()
                .into_iter()
                .filter(|c| c[..2] == ["pr", "ready"])
                .count()
        };
        assert_eq!(readies(&gh), 0);
        let id = app.store.prs_of(&key).await.unwrap()[0].id;
        let uri = format!("/api/v1/tasks/PROJ-1/prs/{id}/ready");
        let (s, v) = call(&router, "POST", &uri, None).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert_eq!(v["snapshot"]["draft"], false);
        assert_eq!(readies(&gh), 1);
        let (s, _) = call(
            &router,
            "POST",
            &format!("/api/v1/tasks/PROJ-1/prs/{}/ready", id + 9),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }
}
