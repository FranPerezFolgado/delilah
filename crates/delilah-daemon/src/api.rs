//! HTTP API of the daemon. Every client (CLI, desktop app, mobile web) goes through
//! it: nothing else reads the database.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::Utc;
use delilah_core::{Event, Task, TaskKey};
use serde::{Deserialize, Serialize};

use crate::store::{EventRecord, Store, StoreError};

pub fn router(store: Store) -> Router {
    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/tasks", get(list_tasks).post(create_task))
        .route("/api/v1/tasks/{key}", get(get_task))
        .route(
            "/api/v1/tasks/{key}/events",
            get(list_events).post(post_event),
        )
        .with_state(store)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Health {
    pub version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NewTask {
    pub key: String,
    pub title: String,
    #[serde(default = "default_workflow")]
    pub workflow: String,
}

fn default_workflow() -> String {
    "feature".into()
}

async fn health() -> Json<Health> {
    Json(Health {
        version: env!("CARGO_PKG_VERSION").into(),
    })
}

async fn list_tasks(State(store): State<Store>) -> Result<Json<Vec<Task>>, ApiError> {
    Ok(Json(store.list_tasks().await?))
}

async fn create_task(
    State(store): State<Store>,
    Json(new): Json<NewTask>,
) -> Result<(StatusCode, Json<Task>), ApiError> {
    let task = Task::new(TaskKey::new(new.key), new.title, new.workflow);
    store.create_task(&task).await?;
    Ok((StatusCode::CREATED, Json(task)))
}

async fn get_task(
    State(store): State<Store>,
    Path(key): Path<String>,
) -> Result<Json<Task>, ApiError> {
    let key = TaskKey::new(key);
    let task = store
        .get_task(&key)
        .await?
        .ok_or(StoreError::NotFound(key))?;
    Ok(Json(task))
}

async fn list_events(
    State(store): State<Store>,
    Path(key): Path<String>,
) -> Result<Json<Vec<EventRecord>>, ApiError> {
    Ok(Json(store.events(&TaskKey::new(key)).await?))
}

async fn post_event(
    State(store): State<Store>,
    Path(key): Path<String>,
    Json(event): Json<Event>,
) -> Result<Json<Task>, ApiError> {
    Ok(Json(
        store
            .apply_event(&TaskKey::new(key), event, Utc::now())
            .await?,
    ))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorBody {
    pub error: String,
}

pub struct ApiError(StoreError);

impl From<StoreError> for ApiError {
    fn from(e: StoreError) -> Self {
        Self(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self.0 {
            StoreError::NotFound(_) => StatusCode::NOT_FOUND,
            StoreError::AlreadyExists(_) | StoreError::Transition(_) => StatusCode::CONFLICT,
            StoreError::Db(_) | StoreError::Migrate(_) | StoreError::Json(_) => {
                tracing::error!(error = %self.0, "request failed");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        (
            status,
            Json(ErrorBody {
                error: self.0.to_string(),
            }),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use super::*;

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

    async fn app() -> Router {
        router(Store::in_memory().await.unwrap())
    }

    #[tokio::test]
    async fn creates_and_lists_tasks() {
        let app = app().await;
        let (status, task) = call(
            &app,
            "POST",
            "/api/v1/tasks",
            Some(json!({"key": "PROJ-1", "title": "Card retries"})),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(task["status"], json!({"type": "queued"}));
        assert_eq!(task["workflow"], "feature");

        let (status, _) = call(
            &app,
            "POST",
            "/api/v1/tasks",
            Some(json!({"key": "PROJ-1", "title": "Again"})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);

        let (status, list) = call(&app, "GET", "/api/v1/tasks", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(list.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn applies_events_and_reports_invalid_ones() {
        let app = app().await;
        call(
            &app,
            "POST",
            "/api/v1/tasks",
            Some(json!({"key": "PROJ-1", "title": "Card retries"})),
        )
        .await;

        let (status, task) = call(
            &app,
            "POST",
            "/api/v1/tasks/PROJ-1/events",
            Some(json!({"type": "start"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(task["status"]["type"], "preparing-workspace");

        let (status, body) = call(
            &app,
            "POST",
            "/api/v1/tasks/PROJ-1/events",
            Some(json!({"type": "agent-finished"})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(body["error"].as_str().unwrap().contains("agent-finished"));

        let (status, history) = call(&app, "GET", "/api/v1/tasks/PROJ-1/events", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(history[0]["event"], json!({"type": "start"}));
        assert_eq!(history.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn unknown_tasks_are_not_found() {
        let app = app().await;
        let (status, _) = call(&app, "GET", "/api/v1/tasks/PROJ-404", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _) = call(&app, "GET", "/api/v1/tasks/PROJ-404/events", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
