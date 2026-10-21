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
use crate::conversation::{Card, CardOutcome};
use crate::events::Topic;
use crate::hooks::HookQuery;
use crate::proposals::{self, Proposal};
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
    /// A card that proposes a task (see `proposals::proposal_of`).
    pub proposal: Option<Proposal>,
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

    /// Lets go of every question of one session (its turn was stopped); returns the
    /// released ids.
    pub fn release_session(&self, session: &str) -> Vec<String> {
        let ids: Vec<String> = {
            let pending = self.pending.lock().unwrap();
            pending
                .iter()
                .filter(|(_, (ask, _))| ask.session.as_deref() == Some(session))
                .map(|(id, _)| id.clone())
                .collect()
        };
        ids.into_iter()
            .filter(|id| self.resolve(id, Outcome::Released))
            .collect()
    }

    pub(crate) fn get(&self, id: &str) -> Option<Ask> {
        self.pending.lock().unwrap().get(id).map(|(a, _)| a.clone())
    }

    /// Whether the session has a question waiting for your pick.
    pub fn has_session(&self, session: &str) -> bool {
        self.pending
            .lock()
            .unwrap()
            .values()
            .any(|(a, _)| a.session.as_deref() == Some(session))
    }

    /// Takes the ask off the list, so only one answer can ever act on it.
    fn take(&self, id: &str) -> Option<(Ask, oneshot::Sender<Outcome>)> {
        self.pending.lock().unwrap().remove(id)
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
        let removed = self.app.asks.pending.lock().unwrap().remove(&self.id);
        let Some((ask, _)) = removed else {
            return;
        };
        self.app.emit(Topic::Inbox);
        // Still on the list: nobody answered it, so its card cannot stay pending.
        // (A timeout already marked it `terminal`; that one stands.)
        if let Some(session) = ask.session
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            let app = self.app.clone();
            runtime.spawn(async move {
                crate::conversation::card_withdraw_if_pending(&app, &session, &ask.id).await;
            });
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
        proposal: proposals::for_hook(&app, &q.task, q.session.as_deref(), &questions).await,
        questions: questions.clone(),
        since: Utc::now(),
    };
    app.asks
        .pending
        .lock()
        .unwrap()
        .insert(id.clone(), (ask.clone(), tx));
    if let Some(session) = &q.session {
        let card = Card::pending(&ask.id, &ask.questions, ask.proposal.clone());
        crate::conversation::card_added(&app, session, &card).await;
    }
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
        _ => {
            // Timed out (a release already marked its card): answered in the terminal.
            if let Some(session) = &q.session
                && outcome.is_err()
            {
                crate::conversation::card_outcome(&app, session, &ask.id, CardOutcome::Terminal)
                    .await;
            }
            Json(json!({}))
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Answer {
    /// Question text → the label chosen (several, comma-separated, for multiSelect),
    /// or your own words.
    pub answers: Map<String, Value>,
}

/// `POST /api/v1/asks/{id}/answer`: `{ task, error? }`. Answering `Crear` on a
/// proposal creates the backlog task first and tells Claude the outcome.
pub async fn answer(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(answer): Json<Answer>,
) -> Result<Json<Value>, ApiError> {
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
    let (ask, tx) = app
        .asks
        .take(&id)
        .ok_or_else(|| ApiError::not_found("that question is no longer waiting"))?;
    let given = answer.answers.clone();
    let mut answers = answer.answers;
    let reply = proposals::on_answer(&app, &ask, &mut answers).await;
    if let Some(session) = &ask.session {
        crate::conversation::card_answered(
            &app,
            session,
            &ask.id,
            given,
            reply.task.clone(),
            reply.error.clone(),
        )
        .await;
    }
    let _ = tx.send(Outcome::Answered(answers));
    app.emit(Topic::Inbox);
    Ok(Json(json!(reply)))
}

/// `POST /api/v1/asks/{id}/release`: answer in the terminal instead.
pub async fn release(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let ask = app.asks.get(&id);
    if !app.asks.resolve(&id, Outcome::Released) {
        return Err(ApiError::not_found("that question is no longer waiting"));
    }
    if let Some(session) = ask.and_then(|a| a.session) {
        crate::conversation::card_outcome(&app, &session, &id, CardOutcome::Terminal).await;
    }
    app.emit(Topic::Inbox);
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
pub(crate) mod tests {
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
                proposal: None,
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

    pub(crate) async fn app() -> AppState {
        let config = Config {
            home: std::env::temp_dir().join(format!("delilah-asks-{}", uuid::Uuid::new_v4())),
            addr: "127.0.0.1:7420".parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            agent_program: None,
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

    pub(crate) async fn call(app: &AppState, req: Request<Body>) -> (StatusCode, Value) {
        let res = crate::api::router(app.clone()).oneshot(req).await.unwrap();
        let status = res.status();
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
    }

    pub(crate) fn post(path: &str, body: Value) -> Request<Body> {
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

    pub(crate) async fn waiting_ask(app: &AppState) -> Value {
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
        let (status, reply) = call(&app, post(&format!("/api/v1/asks/{id}/answer"), answers)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(reply, json!({ "task": null }));

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

    pub(crate) const TITLE: &str = "Reintentos con backoff";

    pub(crate) fn proposal_card(title: &str, header: &str, preview: Option<&str>) -> Value {
        let mut crear = json!({ "label": "Crear", "description": "Guardar" });
        if let Some(p) = preview {
            crear["preview"] = json!(p);
        }
        json!({ "hook_event_name": "PreToolUse", "tool_name": "AskUserQuestion", "tool_input": { "questions": [{
            "question": title, "header": header, "multiSelect": false,
            "options": [crear, { "label": "Cambiar algo", "description": "x" }, { "label": "Descartar", "description": "x" }]
        }]}})
    }

    pub(crate) async fn project_and_conversation(app: &AppState) {
        app.store
            .create_project(&delilah_core::Project {
                name: "web".into(),
                path: "/w/web".into(),
                kind: delilah_core::ProjectKind::Repo,
                workflow: "feature".into(),
            })
            .await
            .unwrap();
        app.store
            .create_session(&crate::store_sessions::SessionRecord {
                id: "conv".into(),
                task: "@web".into(),
                kind: crate::store_sessions::SessionKind::Claude,
                label: "Idea".into(),
                primary: false,
                closed: false,
                read_only: false,
                conversation: true,
                step: None,
            })
            .await
            .unwrap();
    }

    /// Posts the card, picks `choice`, returns (answer status, reply, hook output).
    pub(crate) async fn pick(
        app: &AppState,
        uri: &str,
        card: Value,
        title: &str,
        choice: &str,
    ) -> (Value, Value) {
        let hook = tokio::spawn({
            let app = app.clone();
            let uri = uri.to_string();
            async move { call(&app, post(&uri, card)).await }
        });
        let item = waiting_ask(app).await;
        let id = item["ask"]["id"].as_str().unwrap().to_string();
        let (status, reply) = call(
            app,
            post(
                &format!("/api/v1/asks/{id}/answer"),
                json!({ "answers": { title: choice } }),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (_, out) = hook.await.unwrap();
        (reply, out)
    }

    pub(crate) const CONV: &str = "/hooks/claude/ask?task=@web&session=conv";

    pub(crate) async fn card(app: &AppState) -> Value {
        let (_, body) = call(
            app,
            Request::get("/api/v1/sessions/conv/conversation")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        body["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["kind"] == "card")
            .cloned()
            .unwrap_or(Value::Null)
    }

    #[tokio::test]
    async fn a_conversation_card_follows_its_ask_in_the_history() {
        let app = app().await;
        project_and_conversation(&app).await;

        // Pending, then answered with the task it created.
        let hook = tokio::spawn({
            let app = app.clone();
            async move {
                call(
                    &app,
                    post(CONV, proposal_card(TITLE, "Nueva tarea", Some("brief"))),
                )
                .await
            }
        });
        let id = waiting_ask(&app).await["ask"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let pending = card(&app).await;
        assert_eq!(pending["outcome"], "pending");
        assert_eq!(pending["ask"], id);
        assert_eq!(pending["proposal"]["title"], TITLE);
        assert_eq!(pending["questions"][0]["header"], "Nueva tarea");
        call(
            &app,
            post(
                &format!("/api/v1/asks/{id}/answer"),
                json!({ "answers": { TITLE: "Crear" } }),
            ),
        )
        .await;
        hook.await.unwrap();
        let answered = card(&app).await;
        assert_eq!(answered["outcome"], "answered");
        assert_eq!(answered["answer"], json!({ TITLE: "Crear" }));
        assert!(answered["task"].as_str().unwrap().starts_with("LIBRE-"));
        assert_eq!(answered["error"], Value::Null);

        // A failed creation keeps the reason.
        let hook = tokio::spawn({
            let app = app.clone();
            async move {
                call(
                    &app,
                    post(CONV, proposal_card(" ", "Nueva tarea", Some("b"))),
                )
                .await
            }
        });
        let id = waiting_ask(&app).await["ask"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        call(
            &app,
            post(
                &format!("/api/v1/asks/{id}/answer"),
                json!({ "answers": { " ": "Crear" } }),
            ),
        )
        .await;
        hook.await.unwrap();
        let (_, body) = call(
            &app,
            Request::get("/api/v1/sessions/conv/conversation")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        let failed = body["entries"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "card")
            .nth(1)
            .unwrap()
            .clone();
        assert_eq!(failed["outcome"], "answered");
        assert_eq!(failed["task"], Value::Null);
        assert!(!failed["error"].as_str().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_released_conversation_card_is_marked_terminal() {
        let app = app().await;
        project_and_conversation(&app).await;
        let hook = tokio::spawn({
            let app = app.clone();
            async move {
                call(
                    &app,
                    post(CONV, proposal_card(TITLE, "Nueva tarea", Some("b"))),
                )
                .await
            }
        });
        let id = waiting_ask(&app).await["ask"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        call(&app, post(&format!("/api/v1/asks/{id}/release"), json!({}))).await;
        hook.await.unwrap();
        assert_eq!(card(&app).await["outcome"], "terminal");
    }

    #[tokio::test]
    async fn a_dropped_hook_withdraws_its_card() {
        let app = app().await;
        project_and_conversation(&app).await;
        let hook = tokio::spawn({
            let app = app.clone();
            async move {
                call(
                    &app,
                    post(CONV, proposal_card(TITLE, "Nueva tarea", Some("b"))),
                )
                .await
            }
        });
        waiting_ask(&app).await;
        for _ in 0..100 {
            if card(&app).await["outcome"] == "pending" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(card(&app).await["outcome"], "pending");
        // Claude went away: the connection closes and the handler's future is dropped.
        hook.abort();
        for _ in 0..100 {
            if card(&app).await["outcome"] == "withdrawn" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(card(&app).await["outcome"], "withdrawn");
        assert!(app.asks.list().is_empty());
    }

    #[tokio::test]
    async fn releasing_a_session_withdraws_only_its_asks() {
        let asks = Asks::default();
        let mut waiting = Vec::new();
        for (id, session) in [("a", "conv"), ("b", "other")] {
            let (tx, rx) = oneshot::channel();
            let ask = Ask {
                id: id.into(),
                task: "@web".into(),
                session: Some(session.into()),
                questions: Value::Null,
                proposal: None,
                since: Utc::now(),
            };
            asks.pending.lock().unwrap().insert(id.into(), (ask, tx));
            waiting.push(rx);
        }
        assert_eq!(asks.release_session("conv"), ["a"]);
        assert!(matches!(waiting[0].try_recv(), Ok(Outcome::Released)));
        assert_eq!(asks.list().len(), 1);
    }
}
