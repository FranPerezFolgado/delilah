//! The HTTP side of a task's preview (the process side is in `preview.rs`).

use axum::Json;
use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use delilah_core::TaskKey;

use crate::api::ApiError;
use crate::preview::{PreviewView, start_for};
use crate::state::AppState;

/// `GET /api/v1/tasks/{key}/preview`: the preview, or `null` when there is none.
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
    let view = start_for(&app, &TaskKey::new(key)).await?;
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
    use std::sync::Arc;
    use std::time::Duration;

    use axum::Router;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    use super::*;
    use crate::gh::fake::FakeGh;
    use crate::pr_testkit::task_with_workspace;
    use crate::preview::PreviewState;

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

        for _ in 0..200 {
            if app
                .previews
                .view(key.as_str())
                .is_some_and(|v| v.state == PreviewState::Running)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
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
