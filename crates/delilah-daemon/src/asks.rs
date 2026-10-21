//! Questions an agent asks with Claude Code's AskUserQuestion tool, answered from
//! Delilah. The tool's PreToolUse hook posts here and waits: Delilah shows the
//! question with its options (the inbox, the session's tab) and, once you pick, the
//! hook hands your answer to the tool (`updatedInput.answers`), so the agent gets it
//! as if chosen in its terminal. "Responder en la terminal" lets the hook go without
//! an answer and the usual question shows up in the terminal.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use delilah_core::{Event, TaskKey};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tokio::sync::oneshot;

use crate::api::ApiError;
use crate::events::Topic;
use crate::hooks::HookQuery;
use crate::sessions::LiveState;
use crate::state::AppState;
use crate::store::StoreError;

/// As long as the hook's own timeout (see `claude::settings`).
pub const MAX_WAIT: Duration = Duration::from_secs(delilah_agents::claude::ASK_TIMEOUT_SECS);

#[derive(Debug, Clone, Serialize)]
pub struct Ask {
    pub id: String,
    pub task: String,
    pub session: Option<String>,
    /// AskUserQuestion's `questions`: question, header, options (label,
    /// description) and multiSelect, as Claude sent them.
    pub questions: Value,
    pub since: DateTime<Utc>,
}

enum Outcome {
    Answered(Map<String, Value>),
    /// Answer in the terminal instead.
    Released,
}

#[derive(Default)]
pub struct Asks {
    pending: Mutex<HashMap<String, (Ask, oneshot::Sender<Outcome>)>>,
}

impl Asks {
    pub fn list(&self) -> Vec<Ask> {
        let mut asks: Vec<Ask> = self
            .pending
            .lock()
            .unwrap()
            .values()
            .map(|(a, _)| a.clone())
            .collect();
        asks.sort_by_key(|a| a.since);
        asks
    }

    /// Lets go of every question of `task` (it is being deleted): the terminal asks.
    pub fn release_task(&self, task: &str) {
        let ids: Vec<String> = {
            let pending = self.pending.lock().unwrap();
            pending
                .iter()
                .filter(|(_, (ask, _))| ask.task == task)
                .map(|(id, _)| id.clone())
                .collect()
        };
        for id in ids {
            self.resolve(&id, Outcome::Released);
        }
    }

    fn get(&self, id: &str) -> Option<Ask> {
        self.pending.lock().unwrap().get(id).map(|(a, _)| a.clone())
    }

    fn resolve(&self, id: &str, outcome: Outcome) -> bool {
        match self.pending.lock().unwrap().remove(id) {
            Some((_, tx)) => tx.send(outcome).is_ok(),
            None => false,
        }
    }
}

/// Takes the question off the list however the hook ends: answered, timed out, or
/// dropped because Claude went away and closed the connection.
struct Pending<'a> {
    app: &'a AppState,
    id: String,
}

impl Drop for Pending<'_> {
    fn drop(&mut self) {
        if self
            .app
            .asks
            .pending
            .lock()
            .unwrap()
            .remove(&self.id)
            .is_some()
        {
            self.app.emit(Topic::Inbox);
        }
    }
}

/// `POST /hooks/claude/ask?task=…&session=…`: AskUserQuestion's PreToolUse hook. Held
/// until the question is answered or released; the reply is the hook's output.
pub async fn hook(
    State(app): State<AppState>,
    Query(q): Query<HookQuery>,
    Json(hook): Json<Value>,
) -> Json<Value> {
    let questions = hook["tool_input"]["questions"].clone();
    if !questions.as_array().is_some_and(|qs| !qs.is_empty()) {
        // Not something Delilah can show: let the terminal ask.
        return Json(json!({}));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel();
    let ask = Ask {
        id: id.clone(),
        task: q.task.clone(),
        session: q.session.clone(),
        questions: questions.clone(),
        since: Utc::now(),
    };
    app.asks
        .pending
        .lock()
        .unwrap()
        .insert(id.clone(), (ask, tx));
    let _pending = Pending { app: &app, id };

    let live = q.session.as_deref().and_then(|s| app.sessions.get(s));
    if let Some(live) = &live {
        live.set_state(LiveState::Waiting);
        // The question replaces whatever the session last said.
        let _ = app.store.set_attention(&live.id, None).await;
    }
    let of_task = crate::store_sessions::project_of(&q.task).is_none();
    if of_task && live.as_ref().is_none_or(|l| l.primary) {
        match app
            .transition(&TaskKey::new(&q.task), Event::AgentAsked)
            .await
        {
            Ok(_) | Err(StoreError::Transition(_)) => {}
            Err(e) => tracing::warn!(task = %q.task, "question not recorded: {e}"),
        }
    }
    app.emit(Topic::Inbox);

    let outcome = tokio::time::timeout(MAX_WAIT, rx).await;
    match outcome {
        Ok(Ok(Outcome::Answered(answers))) => {
            if let Some(live) = &live {
                live.set_state(LiveState::Working);
            }
            Json(json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "allow",
                    "updatedInput": { "questions": questions, "answers": answers },
                }
            }))
        }
        _ => Json(json!({})),
    }
}

#[derive(Debug, Deserialize)]
pub struct Answer {
    /// Question text → the label chosen (several, comma-separated, for multiSelect),
    /// or your own words.
    pub answers: Map<String, Value>,
}

/// `POST /api/v1/asks/{id}/answer`
pub async fn answer(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(answer): Json<Answer>,
) -> Result<StatusCode, ApiError> {
    let ask = app
        .asks
        .get(&id)
        .ok_or_else(|| ApiError::not_found("that question is no longer waiting"))?;
    let unanswered = ask
        .questions
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|q| q["question"].as_str())
        .find(|q| {
            answer
                .answers
                .get(*q)
                .and_then(Value::as_str)
                .is_none_or(|a| a.trim().is_empty())
        });
    if let Some(q) = unanswered {
        return Err(ApiError::unprocessable(format!("no answer for “{q}”")));
    }
    if !app.asks.resolve(&id, Outcome::Answered(answer.answers)) {
        return Err(ApiError::not_found("that question is no longer waiting"));
    }
    app.emit(Topic::Inbox);
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/v1/asks/{id}/release`: answer in the terminal instead.
pub async fn release(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if !app.asks.resolve(&id, Outcome::Released) {
        return Err(ApiError::not_found("that question is no longer waiting"));
    }
    app.emit(Topic::Inbox);
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use delilah_core::Task;
    use tower::ServiceExt;

    use super::*;
    use crate::config::Config;
    use crate::store::Store;

    #[test]
    fn a_deleted_task_lets_go_of_its_questions_only() {
        let asks = Asks::default();
        let mut waiting = Vec::new();
        for (id, task) in [("a", "PROJ-1"), ("b", "PROJ-1"), ("c", "PROJ-2")] {
            let (tx, rx) = oneshot::channel();
            let ask = Ask {
                id: id.into(),
                task: task.into(),
                session: None,
                questions: Value::Null,
                since: Utc::now(),
            };
            asks.pending.lock().unwrap().insert(id.into(), (ask, tx));
            waiting.push(rx);
        }

        asks.release_task("PROJ-1");
        let left: Vec<String> = asks.list().into_iter().map(|a| a.id).collect();
        assert_eq!(left, ["c"]);
        for mut rx in waiting.drain(..2) {
            assert!(matches!(rx.try_recv(), Ok(Outcome::Released)));
        }
    }

    async fn app() -> AppState {
        let config = Config {
            home: std::env::temp_dir().join(format!("delilah-asks-{}", uuid::Uuid::new_v4())),
            addr: "127.0.0.1:7420".parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            claude_bin: "claude".into(),
        };
        let app = AppState::new(Store::in_memory().await.unwrap(), config);
        let key = TaskKey::new("PROJ-1");
        app.store
            .create_task(&Task::new(key.clone(), "Card retries", "feature"))
            .await
            .unwrap();
        app.transition(&key, Event::Start).await.unwrap();
        app.transition(&key, Event::WorkspaceReady).await.unwrap();
        app
    }

    async fn call(app: &AppState, req: Request<Body>) -> (StatusCode, Value) {
        let res = crate::api::router(app.clone()).oneshot(req).await.unwrap();
        let status = res.status();
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
    }

    fn post(path: &str, body: Value) -> Request<Body> {
        Request::post(path)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    fn question() -> Value {
        json!({ "hook_event_name": "PreToolUse", "tool_name": "AskUserQuestion", "tool_input": { "questions": [{
            "question": "¿Cómo pagino las facturas?", "header": "Paginación", "multiSelect": false,
            "options": [{ "label": "Cursor", "description": "Estable" }, { "label": "Offset", "description": "Simple" }]
        }]}})
    }

    async fn waiting_ask(app: &AppState) -> Value {
        for _ in 0..50 {
            let (_, inbox) = call(
                app,
                Request::get("/api/v1/inbox").body(Body::empty()).unwrap(),
            )
            .await;
            if let Some(item) = inbox
                .as_array()
                .unwrap()
                .iter()
                .find(|i| i["ask"].is_object())
            {
                return item.clone();
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("the question never reached the inbox");
    }

    #[tokio::test]
    async fn an_answer_from_delilah_reaches_the_tool() {
        let app = app().await;
        let hook = tokio::spawn({
            let app = app.clone();
            async move { call(&app, post("/hooks/claude/ask?task=PROJ-1", question())).await }
        });

        let item = waiting_ask(&app).await;
        assert_eq!(item["kind"], "question");
        assert_eq!(item["text"], "¿Cómo pagino las facturas?");
        assert_eq!(item["ask"]["questions"][0]["options"][1]["label"], "Offset");
        let id = item["ask"]["id"].as_str().unwrap();

        let (status, _) = call(
            &app,
            post(
                &format!("/api/v1/asks/{id}/answer"),
                json!({ "answers": {} }),
            ),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "every question needs an answer"
        );
        let answers = json!({ "answers": { "¿Cómo pagino las facturas?": "Cursor" } });
        let (status, _) = call(&app, post(&format!("/api/v1/asks/{id}/answer"), answers)).await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        let (_, out) = hook.await.unwrap();
        let output = &out["hookSpecificOutput"];
        assert_eq!(output["permissionDecision"], "allow");
        assert_eq!(
            output["updatedInput"]["answers"]["¿Cómo pagino las facturas?"],
            "Cursor"
        );
        assert_eq!(
            output["updatedInput"]["questions"][0]["header"],
            "Paginación"
        );
        assert!(app.asks.list().is_empty());
    }

    #[tokio::test]
    async fn released_questions_go_back_to_the_terminal() {
        let app = app().await;
        let hook = tokio::spawn({
            let app = app.clone();
            async move { call(&app, post("/hooks/claude/ask?task=PROJ-1", question())).await }
        });
        let id = waiting_ask(&app).await["ask"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let (status, _) = call(&app, post(&format!("/api/v1/asks/{id}/release"), json!({}))).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (_, out) = hook.await.unwrap();
        assert_eq!(
            out,
            json!({}),
            "an empty reply lets Claude ask in its terminal"
        );
    }
}
