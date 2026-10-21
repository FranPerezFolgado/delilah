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

use crate::api::{ApiError, NewTask};
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
    /// A card that proposes a task (see `proposal_of`).
    pub proposal: Option<Proposal>,
}

/// The title and brief of a task Claude proposes in an idea conversation.
#[derive(Debug, Clone, Serialize)]
pub struct Proposal {
    pub title: String,
    pub brief: String,
}

/// A proposal is a card of a project's session (owner `@project`) with exactly one
/// question headed `Nueva tarea`, whose text is the title and whose `Crear` option
/// carries the brief as its preview.
fn proposal_of(owner: &str, questions: &Value) -> Option<Proposal> {
    crate::store_sessions::project_of(owner)?;
    let [question] = questions.as_array()?.as_slice() else {
        return None;
    };
    if question["header"].as_str() != Some("Nueva tarea") {
        return None;
    }
    let title = question["question"].as_str()?;
    let brief = question["options"]
        .as_array()?
        .iter()
        .find(|o| o["label"].as_str() == Some("Crear"))?["preview"]
        .as_str()?;
    Some(Proposal {
        title: title.to_string(),
        brief: brief.to_string(),
    })
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

    fn get(&self, id: &str) -> Option<Ask> {
        self.pending.lock().unwrap().get(id).map(|(a, _)| a.clone())
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
        proposal: proposal_of(&q.task, &questions),
        questions: questions.clone(),
        since: Utc::now(),
    };
    app.asks
        .pending
        .lock()
        .unwrap()
        .insert(id.clone(), (ask.clone(), tx));
    if let Some(session) = &q.session {
        crate::conversation::card_added(&app, session, &ask).await;
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
                crate::conversation::card_update(
                    &app,
                    session,
                    &ask.id,
                    json!({ "outcome": "terminal" }),
                )
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
    let mut reply = json!({ "task": null });
    if let Some(proposal) = &ask.proposal {
        let question = proposal.title.as_str();
        if answers.get(question).and_then(Value::as_str).map(str::trim) == Some("Crear") {
            let text = match create_from_proposal(&app, &ask, proposal).await {
                Ok(key) => {
                    app.emit(Topic::Tasks);
                    reply = json!({ "task": key });
                    format!("Crear — creada {key} en el backlog")
                }
                Err(e) => {
                    reply = json!({ "task": null, "error": e.message });
                    format!("Crear — no se pudo crear: {}", e.message)
                }
            };
            answers.insert(question.to_string(), Value::String(text));
        }
    }
    if let Some(session) = &ask.session {
        let patch = json!({
            "outcome": "answered",
            "answer": given,
            "task": reply["task"],
            "error": reply["error"],
        });
        crate::conversation::card_update(&app, session, &ask.id, patch).await;
    }
    let _ = tx.send(Outcome::Answered(answers));
    app.emit(Topic::Inbox);
    Ok(Json(reply))
}

/// Creates the proposal's free task in the conversation's project, with the brief as
/// its notes, and records where it came from.
async fn create_from_proposal(
    app: &AppState,
    ask: &Ask,
    proposal: &Proposal,
) -> Result<String, ApiError> {
    let project = crate::store_sessions::project_of(&ask.task)
        .ok_or_else(|| ApiError::bad_request("only a project's conversation can create tasks"))?;
    let record = match &ask.session {
        Some(id) => app.store.session(id).await?,
        None => None,
    };
    let label = record.as_ref().map_or("Idea", |r| r.label.as_str());
    let now = Utc::now();
    let notes = format!(
        "> Brief escrito por Claude en la conversación «{label}», {}.\n\n{}",
        now.format("%Y-%m-%d"),
        proposal.brief
    );
    let new = NewTask {
        key: String::new(),
        title: proposal.title.clone(),
        project: Some(project.to_string()),
        notes: Some(notes),
        free: true,
        in_checkout: None,
        workflow: None,
    };
    let task = crate::api::build_task(app, new).await?;
    match record {
        Some(r) => {
            app.store
                .create_task_from_conversation(&task, &r.id, &r.label, now)
                .await?
        }
        None => app.store.create_task(&task).await?,
    }
    Ok(task.key.to_string())
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
        crate::conversation::card_update(&app, &session, &id, json!({ "outcome": "terminal" }))
            .await;
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

    async fn app() -> AppState {
        let config = Config {
            home: std::env::temp_dir().join(format!("delilah-asks-{}", uuid::Uuid::new_v4())),
            addr: "127.0.0.1:7420".parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
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

    const TITLE: &str = "Reintentos con backoff";

    fn proposal_card(title: &str, header: &str, preview: Option<&str>) -> Value {
        let mut crear = json!({ "label": "Crear", "description": "Guardar" });
        if let Some(p) = preview {
            crear["preview"] = json!(p);
        }
        json!({ "hook_event_name": "PreToolUse", "tool_name": "AskUserQuestion", "tool_input": { "questions": [{
            "question": title, "header": header, "multiSelect": false,
            "options": [crear, { "label": "Cambiar algo", "description": "x" }, { "label": "Descartar", "description": "x" }]
        }]}})
    }

    async fn project_and_conversation(app: &AppState) {
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
            })
            .await
            .unwrap();
    }

    /// Posts the card, picks `choice`, returns (answer status, reply, hook output).
    async fn pick(
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

    const CONV: &str = "/hooks/claude/ask?task=@web&session=conv";

    #[tokio::test]
    async fn a_crear_answer_on_a_project_proposal_creates_a_backlog_task_with_the_brief() {
        let app = app().await;
        project_and_conversation(&app).await;
        let card = proposal_card(TITLE, "Nueva tarea", Some("## Idea\nreintentar"));
        let hook = tokio::spawn({
            let app = app.clone();
            async move { call(&app, post(CONV, card)).await }
        });
        let item = waiting_ask(&app).await;
        assert_eq!(item["ask"]["proposal"]["title"], TITLE);
        assert_eq!(item["ask"]["proposal"]["brief"], "## Idea\nreintentar");
        assert_eq!(
            item["ask"]["questions"][0]["options"][0]["preview"],
            "## Idea\nreintentar"
        );
        let id = item["ask"]["id"].as_str().unwrap();
        let (status, reply) = call(
            &app,
            post(
                &format!("/api/v1/asks/{id}/answer"),
                json!({ "answers": { TITLE: "Crear" } }),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let key = reply["task"].as_str().unwrap().to_string();
        assert!(key.starts_with("LIBRE-"));

        let (_, out) = hook.await.unwrap();
        assert_eq!(
            out["hookSpecificOutput"]["updatedInput"]["answers"][TITLE],
            format!("Crear — creada {key} en el backlog")
        );
        let task = app
            .store
            .get_task(&TaskKey::new(&key))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(task.title, TITLE);
        assert_eq!(task.project.as_deref(), Some("web"));
        assert_eq!(task.status, delilah_core::TaskStatus::Queued);
        let notes = task.notes.unwrap();
        assert!(notes.starts_with("> Brief escrito por Claude en la conversación «Idea»,"));
        assert!(notes.ends_with("\n\n## Idea\nreintentar"));
        let events = app.store.events(&task.key).await.unwrap();
        assert!(events.iter().any(|e| matches!(&e.event,
            Event::CreatedFromConversation { session, label } if session == "conv" && label == "Idea")));
    }

    #[tokio::test]
    async fn a_task_sessions_card_with_the_same_shape_creates_nothing() {
        let app = app().await;
        let card = proposal_card(TITLE, "Nueva tarea", Some("brief"));
        let (reply, out) = pick(&app, "/hooks/claude/ask?task=PROJ-1", card, TITLE, "Crear").await;
        assert_eq!(reply, json!({ "task": null }));
        assert_eq!(
            out["hookSpecificOutput"]["updatedInput"]["answers"][TITLE],
            "Crear"
        );
        assert_eq!(app.store.list_tasks().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_cambiar_answer_creates_nothing() {
        let app = app().await;
        project_and_conversation(&app).await;
        let card = proposal_card(TITLE, "Nueva tarea", Some("brief"));
        let (reply, out) = pick(&app, CONV, card, TITLE, "Cambiar algo").await;
        assert_eq!(reply, json!({ "task": null }));
        assert_eq!(
            out["hookSpecificOutput"]["updatedInput"]["answers"][TITLE],
            "Cambiar algo"
        );
        assert_eq!(app.store.list_tasks().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_second_answer_cannot_create_a_second_task() {
        let app = app().await;
        project_and_conversation(&app).await;
        let card = proposal_card(TITLE, "Nueva tarea", Some("brief"));
        let hook = tokio::spawn({
            let app = app.clone();
            async move { call(&app, post(CONV, card)).await }
        });
        let id = waiting_ask(&app).await["ask"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let uri = format!("/api/v1/asks/{id}/answer");
        let body = json!({ "answers": { TITLE: "Crear" } });
        let (first, _) = call(&app, post(&uri, body.clone())).await;
        let (second, _) = call(&app, post(&uri, body)).await;
        assert_eq!(first, StatusCode::OK);
        assert_eq!(second, StatusCode::NOT_FOUND);
        hook.await.unwrap();
        assert_eq!(app.store.list_tasks().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn an_empty_title_answers_with_the_reason_and_creates_nothing() {
        let app = app().await;
        project_and_conversation(&app).await;
        let card = proposal_card(" ", "Nueva tarea", Some("brief"));
        let (reply, out) = pick(&app, CONV, card, " ", "Crear").await;
        assert_eq!(reply["task"], Value::Null);
        let error = reply["error"].as_str().unwrap();
        assert!(!error.is_empty());
        let answer = out["hookSpecificOutput"]["updatedInput"]["answers"][" "]
            .as_str()
            .unwrap();
        assert!(answer.starts_with("Crear — no se pudo crear: "));
        assert_eq!(app.store.list_tasks().await.unwrap().len(), 1);
    }

    async fn card(app: &AppState) -> Value {
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

    #[test]
    fn a_card_without_preview_or_with_two_questions_is_not_a_proposal() {
        let qs = |v: Value| v["tool_input"]["questions"].clone();
        let good = qs(proposal_card(TITLE, "Nueva tarea", Some("b")));
        assert!(proposal_of("@web", &good).is_some());
        assert!(proposal_of("PROJ-1", &good).is_none());
        let no_preview = qs(proposal_card(TITLE, "Nueva tarea", None));
        assert!(proposal_of("@web", &no_preview).is_none());
        let other_header = qs(proposal_card(TITLE, "Otra", Some("b")));
        assert!(proposal_of("@web", &other_header).is_none());
        let first = good[0].clone();
        assert!(proposal_of("@web", &json!([first.clone(), first])).is_none());
    }
}
