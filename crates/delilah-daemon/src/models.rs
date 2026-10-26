//! The Claude model of each agent session, chosen locally at the session's start: the
//! task's own model, else its phase's (the `agent-models` preference). A session keeps
//! the model it started on, also when resumed, so its prompt cache is never thrown away;
//! a new model only takes effect in a new session (each saving-mode step is one).

use std::time::Duration;

use axum::Json;
use axum::extract::State;
use delilah_agents::{model_suggest, oneshot};
use delilah_core::{AgentModel, PhaseModels, SpecPhase, Step, Task, TaskKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::ApiError;

use crate::state::AppState;
use crate::store_sessions::{SessionKind, SessionRecord, project_of};

/// Preference with the model of each phase.
pub const MODELS_PREF: &str = "agent-models";
/// Preference that turns on the suggestion when creating a task (off by default).
pub const SUGGEST_PREF: &str = "model-suggestion";

/// A value the `agent-models` preference may take: `null` (the defaults again) or every
/// phase with a known model.
pub fn validate(value: &Value) -> Result<(), ApiError> {
    if value.is_null() {
        return Ok(());
    }
    serde_json::from_value::<PhaseModels>(value.clone())
        .map(|_| ())
        .map_err(|e| {
            ApiError::bad_request(format!(
                "`{MODELS_PREF}` needs specify, plan, implement and review, each one of opus, \
                 sonnet, opusplan, haiku or default: {e}"
            ))
        })
}

/// The phases' models; the defaults when unset (or unreadable).
pub async fn phase_models(app: &AppState) -> PhaseModels {
    let value: Option<Value> = app
        .store
        .setting(&format!("pref:{MODELS_PREF}"))
        .await
        .ok()
        .flatten();
    value
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

/// The phase a new session of `task` works in. A saving-mode session runs its step (the
/// plan step covers specify too); any other, the task's phase, or before it has one,
/// specify on the spec workflow and implement elsewhere.
pub fn session_phase(task: &Task, step: Option<Step>) -> SpecPhase {
    let phase = task.phase.as_deref().and_then(SpecPhase::parse);
    match step {
        Some(Step::Plan) => match phase {
            Some(SpecPhase::Specify) | None => SpecPhase::Specify,
            Some(_) => SpecPhase::Plan,
        },
        Some(Step::Implement) => SpecPhase::Implement,
        Some(Step::Review) => SpecPhase::Review,
        None => phase.unwrap_or(match task.workflow.as_str() {
            "feature" => SpecPhase::Specify,
            _ => SpecPhase::Implement,
        }),
    }
}

/// The `--model` for a Claude session about to start; `None` passes none. A session that
/// already has a model keeps it; a resumed one from before models were chosen keeps the
/// account's default it ran on. Only task sessions get one: a project's sessions and idea
/// conversations run on the default.
pub async fn for_session(
    app: &AppState,
    session: &SessionRecord,
    resume: bool,
) -> Result<Option<String>, ApiError> {
    if session.kind != SessionKind::Claude || project_of(&session.task).is_some() {
        return Ok(None);
    }
    if let Some(model) = app.store.session_model(&session.id).await? {
        return Ok(Some(model));
    }
    if resume {
        return Ok(None);
    }
    let Some(task) = app.store.get_task(&TaskKey::new(&session.task)).await? else {
        return Ok(None);
    };
    let phase = session_phase(&task, session.step);
    let model = delilah_core::model::resolve(task.model, phase, &phase_models(app).await);
    app.store.set_session_model(&session.id, model).await?;
    tracing::info!(task = %session.task, session = %session.id, phase = phase.name(), model = model.unwrap_or("default"), "agent model chosen");
    Ok(model.map(str::to_string))
}

/// Tells a `model` that is absent (`None`: leave it) from one that is `null`
/// (`Some(None)`: clear it).
pub fn present<'de, D>(de: D) -> Result<Option<Option<AgentModel>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    <Option<AgentModel> as Deserialize>::deserialize(de).map(Some)
}

async fn suggestion_on(app: &AppState) -> bool {
    let value: Option<Value> = app
        .store
        .setting(&format!("pref:{SUGGEST_PREF}"))
        .await
        .ok()
        .flatten();
    value.as_ref().and_then(Value::as_bool).unwrap_or(false)
}

#[derive(Debug, Deserialize)]
pub struct SuggestRequest {
    pub title: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub workflow: Option<String>,
    /// A Jira key: its type, labels and description are read from the ticket.
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub free: bool,
}

#[derive(Debug, Serialize)]
pub struct SuggestResponse {
    pub model: AgentModel,
    pub reason: String,
    /// `claude` when Haiku answered, `heuristic` when only the local guess did.
    pub source: &'static str,
}

const SUGGEST_TIMEOUT: Duration = Duration::from_secs(30);

/// `POST /api/v1/model-suggestion`: a model for a task about to be created, for the user
/// to confirm. 409 `suggestion-off` unless the `model-suggestion` preference is on.
pub async fn suggest(
    State(app): State<AppState>,
    Json(req): Json<SuggestRequest>,
) -> Result<Json<SuggestResponse>, ApiError> {
    if !suggestion_on(&app).await {
        return Err(ApiError::conflict_reason(
            "the model suggestion is off in Settings",
            "suggestion-off",
        ));
    }
    if req.title.trim().is_empty() {
        return Err(ApiError::bad_request("say what the task is about"));
    }
    let issue = match (&req.key, req.free) {
        (Some(key), false) if !key.trim().is_empty() => {
            crate::jira::fetch_issue(&app, key.trim()).await
        }
        _ => None,
    };
    let notes = req.notes.clone().unwrap_or_default();
    let description = match &issue {
        Some(issue) if notes.trim().is_empty() => issue.description.clone(),
        Some(issue) => format!("{}\n\n{notes}", issue.description),
        None => notes,
    };
    let no_labels = Vec::new();
    let ticket = model_suggest::Ticket {
        title: &req.title,
        description: &description,
        issue_type: issue.as_ref().map_or("", |i| i.issue_type.as_str()),
        labels: issue.as_ref().map_or(&no_labels, |i| &i.components),
        workflow: req.workflow.as_deref().unwrap_or(""),
    };
    let (guess, why) = model_suggest::heuristic(&ticket);
    let prompt = model_suggest::prompt(&ticket, guess, app.lang().name());
    let mut cmd = tokio::process::Command::new(&app.config.claude_bin);
    cmd.env_clear()
        .envs(oneshot::env())
        .args(model_suggest::args(&prompt))
        .current_dir(std::env::temp_dir())
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    let answer = match tokio::time::timeout(SUGGEST_TIMEOUT, cmd.output()).await {
        Ok(Ok(output)) => model_suggest::parse(&String::from_utf8_lossy(&output.stdout)),
        Ok(Err(e)) => Err(format!("cannot run claude: {e}")),
        Err(_) => Err(format!("claude took more than {SUGGEST_TIMEOUT:?}")),
    };
    Ok(Json(match answer {
        Ok((model, reason)) => SuggestResponse {
            model,
            reason: if reason.is_empty() { why } else { reason },
            source: "claude",
        },
        Err(e) => {
            tracing::info!("model suggestion from the heuristic only: {e}");
            SuggestResponse {
                model: guess,
                reason: why,
                source: "heuristic",
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn task(workflow: &str, phase: Option<&str>) -> Task {
        let mut t = Task::new(TaskKey::new("PROJ-1"), "t", workflow);
        t.phase = phase.map(String::from);
        t
    }

    #[test]
    fn the_session_phase_follows_the_step_then_the_task() {
        let fresh = task("feature", None);
        assert_eq!(session_phase(&fresh, None), SpecPhase::Specify);
        assert_eq!(
            session_phase(&task("plain", None), None),
            SpecPhase::Implement
        );
        assert_eq!(
            session_phase(&task("plain", Some("review")), None),
            SpecPhase::Review
        );
        // The plan step covers specify and plan.
        assert_eq!(session_phase(&fresh, Some(Step::Plan)), SpecPhase::Specify);
        let planning = task("feature", Some("plan"));
        assert_eq!(session_phase(&planning, Some(Step::Plan)), SpecPhase::Plan);
        // A saving-mode step wins over a phase the watcher has not moved yet.
        assert_eq!(
            session_phase(&planning, Some(Step::Implement)),
            SpecPhase::Implement
        );
        assert_eq!(
            session_phase(&planning, Some(Step::Review)),
            SpecPhase::Review
        );
    }

    #[test]
    fn the_preference_must_name_every_phase_with_a_known_model() {
        assert!(validate(&Value::Null).is_ok());
        assert!(
            validate(&json!({"specify": "opus", "plan": "opusplan", "implement": "sonnet", "review": "default"}))
                .is_ok()
        );
        for bad in [
            json!({"specify": "opus"}),
            json!({"specify": "gpt", "plan": "opus", "implement": "opus", "review": "opus"}),
            json!({"specify": "opus", "plan": "opus", "implement": "opus", "review": "opus", "extra": "opus"}),
            json!("opus"),
            json!(1),
        ] {
            assert!(validate(&bad).is_err(), "{bad}");
        }
    }
}
