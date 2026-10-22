//! `POST /api/v1/tasks/{key}/autopilot`.

use axum::Json;
use axum::extract::{Path as AxPath, State};
use delilah_core::{Task, TaskKey};

use super::pause::resume;
use super::task_of;
use crate::api::{ActionReq, ApiError};
use crate::events::Topic;
use crate::state::AppState;

/// `POST /api/v1/tasks/{key}/autopilot`: `resume` or `off`.
pub(crate) async fn act(
    State(app): State<AppState>,
    AxPath(key): AxPath<String>,
    Json(req): Json<ActionReq>,
) -> Result<Json<Task>, ApiError> {
    let key = TaskKey::new(key);
    match req.action.as_str() {
        "resume" => resume(&app, &key).await?,
        "off" => {
            task_of(&app, &key).await?;
            // A plain write: a finish that read the autopilot before cannot put it back
            // (its writes are compare-and-sets that find it gone).
            app.store.set_autopilot(&key, None).await?;
            app.emit(Topic::Inbox);
            app.emit(Topic::Tasks);
        }
        // TODO(WP-12): `on` (saving mode too, and confirm a pending offer).
        "on" => return Err(ApiError::bad_request("`on` is not available yet")),
        other => {
            return Err(ApiError::bad_request(format!("unknown action `{other}`")));
        }
    }
    Ok(Json(task_of(&app, &key).await?))
}

#[cfg(test)]
mod tests {

    use axum::Json;

    use super::*;
    use crate::autopilot::testkit::*;

    #[tokio::test]
    async fn the_route_resumes_turns_off_and_refuses_on() {
        use axum::extract::{Path as AxPath, State};
        let (app, key, _dir) = paused_task(gh("[]"), None).await;
        let call = |action: &str| {
            act(
                State(app.clone()),
                AxPath("PROJ-1".to_string()),
                Json(ActionReq {
                    action: action.into(),
                }),
            )
        };
        assert_eq!(
            call("on").await.unwrap_err().status,
            axum::http::StatusCode::BAD_REQUEST
        );
        assert_eq!(
            call("what").await.unwrap_err().status,
            axum::http::StatusCode::BAD_REQUEST
        );
        let task = call("off").await.ok().unwrap().0;
        assert!(task.autopilot.is_none());
        assert!(task.saving_mode, "saving mode stays");
        let err = call("resume").await.unwrap_err();
        assert_eq!(err.reason.as_deref(), Some("not-paused"));
        let _ = key;
    }
}
