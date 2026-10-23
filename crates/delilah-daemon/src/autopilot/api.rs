//! `POST /api/v1/tasks/{key}/autopilot`.

use axum::Json;
use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use delilah_core::{HandoffStage, Task, TaskKey};

use super::pause::resume;
use super::{fresh, is_running, task_of};
use crate::api::{ActionReq, ApiError};
use crate::events::Topic;
use crate::handoff::Launcher;
use crate::state::AppState;

/// `POST /api/v1/tasks/{key}/autopilot`: `on`, `resume` or `off`.
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
        "on" => turn_on(&app, &key, &crate::run::launch_session_api).await?,
        other => {
            return Err(ApiError::bad_request(format!("unknown action `{other}`")));
        }
    }
    Ok(Json(task_of(&app, &key).await?))
}

/// Turns autopilot on (saving mode with it) in a task that can have saving mode, and
/// confirms a handoff offered meanwhile at once. A paused or finished autopilot is left
/// as it is: `resume` continues it.
async fn turn_on<L: Launcher>(app: &AppState, key: &TaskKey, launch: &L) -> Result<(), ApiError> {
    let task = task_of(app, key).await?;
    if !crate::autopilot::allowed(key, &task.workflow) {
        return Err(crate::autopilot::not_spec_workflow(key));
    }
    let written = app
        .store
        .update_autopilot(key, |current| current.is_none().then(|| Some(fresh())))
        .await?;
    if written {
        app.emit(Topic::Tasks);
    }
    // Only a running autopilot confirms, and only an offer; with none pending the next
    // turn end takes it.
    let task = task_of(app, key).await?;
    let offered = matches!(&task.handoff, Some(h) if h.stage == HandoffStage::Offered);
    if is_running(&task) && offered {
        match crate::handoff::confirm_with(app, key, launch).await {
            // Confirmed or declined by someone else in between.
            Ok(())
            | Err(ApiError {
                status: StatusCode::CONFLICT,
                ..
            }) => {}
            Err(e) => return Err(e),
        }
    }
    app.emit(Topic::Inbox);
    app.emit(Topic::Tasks);
    Ok(())
}

#[cfg(test)]
mod tests {

    use axum::Json;

    use delilah_core::AutopilotState;

    use super::*;
    use crate::autopilot::testkit::*;

    #[tokio::test]
    async fn the_route_resumes_and_turns_off() {
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

    #[tokio::test]
    async fn turning_on_confirms_a_pending_offer_at_once() {
        use delilah_core::{Handoff, Step};
        let (app, key, _dir) = task_with_commits(gh("[]"), None, &["first"]).await;
        app.store.set_saving_mode(&key, true).await.unwrap();
        app.store.set_autopilot(&key, None).await.unwrap();
        write_spec(&app, "- [ ] T001 One\n", false);
        let offered = Handoff {
            to: Step::Implement,
            stage: HandoffStage::Offered,
            error: None,
        };
        app.store.set_handoff(&key, Some(&offered)).await.unwrap();
        let launched = Launched::default();
        turn_on(&app, &key, &stub(&launched)).await.ok().unwrap();
        let task = task_of(&app, &key).await.ok().unwrap();
        assert!(is_running(&task), "{:?}", task.autopilot);
        assert!(task.handoff.is_none(), "{:?}", task.handoff);
        let launched = launched.lock().unwrap();
        assert_eq!(launched.len(), 1);
        assert_eq!(launched[0].0.step, Some(Step::Implement));
    }

    #[tokio::test]
    async fn turning_on_leaves_a_paused_autopilot_for_resume() {
        let (app, key, _dir) = paused_task(gh("[]"), None).await;
        let launched = Launched::default();
        turn_on(&app, &key, &stub(&launched)).await.ok().unwrap();
        let task = task_of(&app, &key).await.ok().unwrap();
        assert_eq!(task.autopilot.unwrap().state, AutopilotState::Paused);
        assert!(launched.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn turning_on_refuses_a_project_session_and_another_workflow() {
        let (app, key, _dir) = paused_task(gh("[]"), None).await;
        let mut task = task_of(&app, &key).await.ok().unwrap();
        assert_eq!(task.workflow, "feature");
        task.workflow = "prompt".into();
        let other = TaskKey::new("PROJ-2");
        task.key = other.clone();
        app.store.create_task(&task).await.unwrap();
        let err = turn_on(&app, &other, &stub(&Launched::default()))
            .await
            .unwrap_err();
        assert_eq!(err.reason.as_deref(), Some("not-spec-workflow"));
    }
}
