//! A task Claude proposes in an idea conversation: a card of a project's session with
//! one question headed `Nueva tarea` whose `Crear` option carries the brief. Answering
//! `Crear` creates the backlog task before Claude hears about it. The protocol strings
//! are the contract with the idea prompt; they live here and nowhere else.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::api::{ApiError, NewTask};
use crate::asks::Ask;
use crate::events::Topic;
use crate::lang::Lang;
use crate::state::AppState;

/// Header of the question that proposes a task.
pub const NEW_TASK_HEADER: &str = "Nueva tarea";
/// The options of a proposal; only `Crear` carries a brief (as its preview).
pub const CREATE: &str = "Crear";
pub const CHANGE: &str = "Cambiar algo";
pub const DISCARD: &str = "Descartar";
/// Starts the notes of a task created from a proposal.
pub const NOTES_MARK: &str = "> Brief escrito por Claude en la conversación";

/// What Claude is told after `Crear` worked.
pub fn created_text(lang: Lang, key: &str) -> String {
    let tail = lang.pick("creada {} en el backlog", "created {} in the backlog");
    format!("{CREATE} — {}", tail.replace("{}", key))
}

/// What Claude is told after `Crear` failed.
pub fn failed_text(lang: Lang, error: &str) -> String {
    format!(
        "{CREATE} — {}: {error}",
        lang.pick("no se pudo crear", "could not create")
    )
}

/// What Claude is told after `Crear` created the task and it was started on autopilot.
pub fn launched_text(lang: Lang, key: &str) -> String {
    let tail = lang.pick(
        "creada {} y lanzada en autopiloto",
        "created {} and launched on autopilot",
    );
    format!("{CREATE} — {}", tail.replace("{}", key))
}

/// What Claude is told after the task was created but could not be started.
pub fn not_launched_text(lang: Lang, key: &str, error: &str) -> String {
    let tail = lang.pick(
        "creada {} en el backlog; no se pudo lanzar",
        "created {} in the backlog; could not launch it",
    );
    format!("{CREATE} — {}: {error}", tail.replace("{}", key))
}

/// The `launch` value of an answer that starts the new task on autopilot.
pub const LAUNCH_AUTOPILOT: &str = "autopilot";

/// Longest title and brief taken from a card; beyond it the card stays a plain one.
const MAX_TITLE_CHARS: usize = 200;
const MAX_BRIEF_BYTES: usize = 64 * 1024;
/// Times a task creation is tried again when its key was taken meanwhile.
const KEY_RETRIES: usize = 3;

/// The title and brief of a task Claude proposes in an idea conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub title: String,
    pub brief: String,
}

/// The reply of an answer: the task created, or why not.
#[derive(Debug, Default, Serialize)]
pub struct AnswerReply {
    pub task: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A proposal is a card of a project's session (owner `@project`) with exactly one
/// question headed `Nueva tarea`, whose text is the title (one short line) and whose
/// `Crear` option carries the brief as its preview.
pub fn proposal_of(owner: &str, questions: &Value) -> Option<Proposal> {
    crate::store_sessions::project_of(owner)?;
    let [question] = questions.as_array()?.as_slice() else {
        return None;
    };
    if question["header"].as_str() != Some(NEW_TASK_HEADER) {
        return None;
    }
    let title = question["question"].as_str()?;
    let brief = question["options"]
        .as_array()?
        .iter()
        .find(|o| o["label"].as_str() == Some(CREATE))?["preview"]
        .as_str()?;
    if title.contains(['\n', '\r'])
        || title.chars().count() > MAX_TITLE_CHARS
        || brief.len() > MAX_BRIEF_BYTES
    {
        return None;
    }
    Some(Proposal {
        title: title.to_string(),
        brief: brief.to_string(),
    })
}

/// The proposal of a card posted by a hook, only when the session it names exists and
/// is open and owned by the same project: a hook cannot claim a conversation that is not its own.
pub async fn for_hook(
    app: &AppState,
    owner: &str,
    session: Option<&str>,
    questions: &Value,
) -> Option<Proposal> {
    let record = app.store.session(session?).await.ok()??;
    if record.task != owner || record.closed {
        return None;
    }
    proposal_of(owner, questions)
}

/// Runs the proposal side of an answer: on `Crear` creates the task and replaces the
/// answer Claude gets with what happened. With `launch` = `autopilot` the task is also
/// started on autopilot; if that fails the task stays in the backlog and the reply
/// carries the error next to it. Anything else is left as it is.
pub async fn on_answer(
    app: &AppState,
    ask: &Ask,
    answers: &mut Map<String, Value>,
    launch: Option<&str>,
) -> AnswerReply {
    let mut reply = AnswerReply::default();
    let Some(proposal) = &ask.proposal else {
        return reply;
    };
    let question = proposal.title.as_str();
    if answers.get(question).and_then(Value::as_str).map(str::trim) != Some(CREATE) {
        return reply;
    }
    let autopilot = launch == Some(LAUNCH_AUTOPILOT);
    let lang = app.lang();
    let text = match create_from_proposal(app, ask, proposal, autopilot).await {
        Ok((key, declined)) => {
            app.emit(Topic::Tasks);
            let started = match (autopilot, declined) {
                (false, _) => None,
                (true, Some(e)) => Some(Err(e)),
                (true, None) => Some(start(app, &key, ask).await),
            };
            let text = match started {
                None => created_text(lang, &key),
                Some(Ok(())) => launched_text(lang, &key),
                Some(Err(e)) => {
                    let text = not_launched_text(lang, &key, &e.message);
                    reply.error = Some(e.message);
                    text
                }
            };
            app.emit(Topic::Tasks);
            reply.task = Some(key);
            text
        }
        Err(e) => {
            let text = failed_text(lang, &e.message);
            reply.error = Some(e.message);
            text
        }
    };
    answers.insert(question.to_string(), Value::String(text));
    reply
}

/// Starts the backlog task `key` in the conversation's project, on autopilot, with the
/// project's default work mode. A fresh run: the conversation is not adopted.
async fn start(app: &AppState, key: &str, ask: &Ask) -> Result<(), ApiError> {
    let project = crate::store_sessions::project_of(&ask.task)
        .ok_or_else(|| ApiError::bad_request("only a project's conversation can start tasks"))?;
    let req: crate::run::RunRequest = serde_json::from_value(serde_json::json!({
        "key": key,
        "project": project,
        "autopilot": true,
        "saving_mode": true,
    }))
    .map_err(|e| ApiError::internal(e.to_string()))?;
    crate::run::start_run(axum::extract::State(app.clone()), axum::Json(req))
        .await
        .map(|_| ())
}

/// Creates the proposal's free task in the conversation's project, with the brief as
/// its notes, and records where it came from.
async fn create_from_proposal(
    app: &AppState,
    ask: &Ask,
    proposal: &Proposal,
    autopilot: bool,
) -> Result<(String, Option<ApiError>), ApiError> {
    let project = crate::store_sessions::project_of(&ask.task)
        .ok_or_else(|| ApiError::bad_request("only a project's conversation can create tasks"))?;
    // The origin event is part of the feature: without the conversation, no task.
    let record = match &ask.session {
        Some(id) => app.store.session(id).await?,
        None => None,
    }
    .ok_or_else(|| ApiError::not_found("la conversación ya no existe"))?;
    let now = Utc::now();
    let notes = format!(
        "{NOTES_MARK} «{}», {}.\n\n{}",
        record.label,
        now.format("%Y-%m-%d"),
        proposal.brief
    );
    // Autopilot is for the spec workflow only: elsewhere the task is still created,
    // without it, and the refusal (naming the task actually made) goes back next to it.
    let mut declined = false;
    let new = |autopilot: bool| NewTask {
        key: String::new(),
        title: proposal.title.clone(),
        project: Some(project.to_string()),
        notes: Some(notes.clone()),
        free: true,
        in_checkout: None,
        saving_mode: autopilot.then_some(true),
        autopilot: autopilot.then_some(true),
        workflow: None,
        view: None,
    };
    let mut autopilot = autopilot;
    // Another conversation may take the key between allocating it and storing the
    // task: allocate again (the counter has moved on).
    let mut attempts = 0;
    let task = loop {
        let task = match crate::api::build_task(app, new(autopilot)).await {
            Err(e) if autopilot && e.reason.as_deref() == Some("not-spec-workflow") => {
                declined = true;
                autopilot = false;
                continue;
            }
            built => built?,
        };
        match app
            .store
            .create_task_from_conversation(&task, &record.id, &record.label, now)
            .await
        {
            Err(crate::store::StoreError::AlreadyExists(_)) if attempts < KEY_RETRIES => {
                attempts += 1;
            }
            other => {
                other?;
                break task;
            }
        }
    };
    let refusal = declined.then(|| crate::autopilot::refusal(task.key.as_str(), app.lang()));
    Ok((task.key.to_string(), refusal))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use delilah_core::{Event, TaskKey};
    use serde_json::json;

    use super::*;
    use crate::asks::tests::{
        CONV, TITLE, app, call, pick, post, project_and_conversation, proposal_card, waiting_ask,
    };

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
    async fn two_proposals_in_one_conversation_create_two_tasks() {
        let app = app().await;
        project_and_conversation(&app).await;
        let mut keys = Vec::new();
        for (title, brief) in [("Primera idea", "brief uno"), ("Segunda idea", "brief dos")] {
            let card = proposal_card(title, NEW_TASK_HEADER, Some(brief));
            let (reply, _) = pick(&app, CONV, card, title, CREATE).await;
            keys.push(reply["task"].as_str().unwrap().to_string());
        }
        assert_eq!(keys.len(), 2);
        assert_ne!(keys[0], keys[1]);
        let n = |k: &str| k.strip_prefix("LIBRE-").unwrap().parse::<u32>().unwrap();
        assert_eq!(n(&keys[1]), n(&keys[0]) + 1);

        for (key, title, brief) in [
            (&keys[0], "Primera idea", "brief uno"),
            (&keys[1], "Segunda idea", "brief dos"),
        ] {
            let task = app
                .store
                .get_task(&TaskKey::new(key))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(task.title, title);
            assert!(task.notes.unwrap().ends_with(&format!("\n\n{brief}")));
        }

        let cards: Vec<Value> = app
            .store
            .entries("conv")
            .await
            .unwrap()
            .into_iter()
            .filter(|e| e.kind == crate::store_sessions::EntryKind::Card)
            .map(|e| serde_json::to_value(e).unwrap())
            .collect();
        assert_eq!(cards.len(), 2);
        for (card, key) in cards.iter().zip(&keys) {
            assert_eq!(card["outcome"], "answered");
            assert_eq!(card["task"], key.as_str());
        }
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
    async fn a_descartar_answer_creates_nothing() {
        let app = app().await;
        project_and_conversation(&app).await;
        let card = proposal_card(TITLE, "Nueva tarea", Some("brief"));
        let (reply, out) = pick(&app, CONV, card, TITLE, DISCARD).await;
        assert_eq!(reply, json!({ "task": null }));
        assert_eq!(
            out["hookSpecificOutput"]["updatedInput"]["answers"][TITLE],
            DISCARD
        );
        assert_eq!(app.store.list_tasks().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn the_conversation_stays_usable_after_crear() {
        let app = app().await;
        project_and_conversation(&app).await;
        let card = proposal_card("Primera", NEW_TASK_HEADER, Some("uno"));
        let (reply, _) = pick(&app, CONV, card, "Primera", CREATE).await;
        assert!(reply["task"].is_string());
        let record = app.store.session("conv").await.unwrap().unwrap();
        assert!(!record.closed && record.conversation);
        let (status, body) = call(
            &app,
            axum::http::Request::get("/api/v1/sessions/conv/conversation")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["entries"][0]["outcome"], "answered");
        // A later proposal still creates its own task.
        let card = proposal_card("Segunda", NEW_TASK_HEADER, Some("dos"));
        let (second, _) = pick(&app, CONV, card, "Segunda", CREATE).await;
        assert!(second["task"].is_string());
        assert_ne!(second["task"], reply["task"]);
        assert_eq!(app.store.list_tasks().await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn crear_after_the_project_was_deleted_fails_and_creates_nothing() {
        let app = app().await;
        project_and_conversation(&app).await;
        let card = proposal_card(TITLE, NEW_TASK_HEADER, Some("brief"));
        let hook = tokio::spawn({
            let app = app.clone();
            async move { call(&app, post(CONV, card)).await }
        });
        let id = waiting_ask(&app).await["ask"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        app.store.delete_project("web").await.unwrap();
        let (status, reply) = call(
            &app,
            post(
                &format!("/api/v1/asks/{id}/answer"),
                json!({ "answers": { TITLE: CREATE } }),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(reply["task"], Value::Null);
        assert!(!reply["error"].as_str().unwrap().is_empty());
        let (_, out) = hook.await.unwrap();
        let answer = out["hookSpecificOutput"]["updatedInput"]["answers"][TITLE]
            .as_str()
            .unwrap();
        assert!(answer.starts_with("Crear — no se pudo crear: "), "{answer}");
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

    #[tokio::test]
    async fn a_hook_cannot_claim_a_session_it_does_not_own() {
        let app = app().await;
        project_and_conversation(&app).await;
        let qs =
            proposal_card(TITLE, NEW_TASK_HEADER, Some("b"))["tool_input"]["questions"].clone();
        // The conversation belongs to @web.
        assert!(for_hook(&app, "@web", Some("conv"), &qs).await.is_some());
        assert!(for_hook(&app, "@other", Some("conv"), &qs).await.is_none());
        assert!(for_hook(&app, "@web", Some("nope"), &qs).await.is_none());
        assert!(for_hook(&app, "@web", None, &qs).await.is_none());

        let card = proposal_card(TITLE, NEW_TASK_HEADER, Some("brief"));
        let (reply, out) = pick(
            &app,
            "/hooks/claude/ask?task=@web&session=PROJ-1",
            card,
            TITLE,
            CREATE,
        )
        .await;
        assert_eq!(reply, json!({ "task": null }));
        assert_eq!(
            out["hookSpecificOutput"]["updatedInput"]["answers"][TITLE],
            CREATE
        );
        assert_eq!(app.store.list_tasks().await.unwrap().len(), 1);
    }

    #[test]
    fn an_oversized_or_multiline_title_or_brief_is_not_a_proposal() {
        let qs = |title: &str, brief: &str| {
            proposal_card(title, NEW_TASK_HEADER, Some(brief))["tool_input"]["questions"].clone()
        };
        assert!(proposal_of("@web", &qs(TITLE, "b")).is_some());
        assert!(proposal_of("@web", &qs("uno\ndos", "b")).is_none());
        assert!(proposal_of("@web", &qs(&"t".repeat(201), "b")).is_none());
        assert!(proposal_of("@web", &qs(&"t".repeat(200), "b")).is_some());
        assert!(proposal_of("@web", &qs(TITLE, &"b".repeat(64 * 1024 + 1))).is_none());
    }

    #[tokio::test]
    async fn creating_without_the_conversation_is_refused() {
        let app = app().await;
        project_and_conversation(&app).await;
        let ask = Ask {
            id: "a".into(),
            task: "@web".into(),
            session: Some("gone".into()),
            questions: Value::Null,
            proposal: Some(Proposal {
                title: TITLE.into(),
                brief: "b".into(),
            }),
            since: Utc::now(),
        };
        let mut answers = Map::new();
        answers.insert(TITLE.into(), json!(CREATE));
        let reply = on_answer(&app, &ask, &mut answers, None).await;
        assert_eq!(reply.task, None);
        assert_eq!(reply.error.as_deref(), Some("la conversación ya no existe"));
        assert_eq!(app.store.list_tasks().await.unwrap().len(), 1);
    }

    /// A plain project session (a terminal, not a conversation) of `@web`.
    async fn project_session(app: &AppState, read_only: bool) {
        project_and_conversation(app).await;
        app.store
            .create_session(&crate::store_sessions::SessionRecord {
                id: "plain".into(),
                task: "@web".into(),
                kind: crate::store_sessions::SessionKind::Claude,
                label: "Claude 1".into(),
                primary: false,
                closed: false,
                read_only,
                conversation: false,
                step: None,
            })
            .await
            .unwrap();
    }

    const PLAIN: &str = "/hooks/claude/ask?task=@web&session=plain";

    #[tokio::test]
    async fn a_project_sessions_proposal_creates_a_task_and_records_its_answered_card() {
        let app = app().await;
        project_session(&app, true).await;
        let card = proposal_card(TITLE, NEW_TASK_HEADER, Some("brief"));
        let (reply, _) = pick(&app, PLAIN, card, TITLE, CREATE).await;
        let key = reply["task"].as_str().unwrap().to_string();
        assert!(key.starts_with("LIBRE-"));
        assert_eq!(app.store.list_tasks().await.unwrap().len(), 2);
        // Every Claude session shows its cards inline: the one entry is the answered card.
        let entries = app.store.entries("plain").await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].body["outcome"], "answered");
    }

    #[tokio::test]
    async fn a_project_session_after_allow_edits_can_still_create() {
        let app = app().await;
        project_session(&app, true).await;
        app.store.set_read_only("plain", false).await.unwrap();
        let card = proposal_card(TITLE, NEW_TASK_HEADER, Some("brief"));
        let (reply, _) = pick(&app, PLAIN, card, TITLE, CREATE).await;
        assert!(reply["task"].as_str().unwrap().starts_with("LIBRE-"));
        assert_eq!(app.store.list_tasks().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_session_converted_into_a_task_cannot_create() {
        let app = app().await;
        project_session(&app, false).await;
        // "Convertir en tarea": the session now belongs to the task.
        app.store
            .adopt_session("plain", "PROJ-1", "Principal")
            .await
            .unwrap();
        let card = proposal_card(TITLE, NEW_TASK_HEADER, Some("brief"));
        let (reply, out) = pick(&app, PLAIN, card, TITLE, CREATE).await;
        assert_eq!(reply, json!({ "task": null }));
        assert_eq!(
            out["hookSpecificOutput"]["updatedInput"]["answers"][TITLE],
            CREATE
        );
        assert_eq!(app.store.list_tasks().await.unwrap().len(), 1);
    }

    /// Posts the proposal card, answers `Crear` with `launch`, returns the reply.
    async fn crear_with(app: &AppState, launch: Option<&str>, answer: &str) -> Value {
        let hook = tokio::spawn({
            let app = app.clone();
            async move { call(&app, proposal_post(&app)).await }
        });
        let item = waiting_ask(app).await;
        let id = item["ask"]["id"].as_str().unwrap().to_string();
        let mut body = json!({ "answers": { TITLE: answer } });
        if let Some(l) = launch {
            body["launch"] = json!(l);
        }
        let (status, reply) = call(app, post(&format!("/api/v1/asks/{id}/answer"), body)).await;
        assert_eq!(status, StatusCode::OK);
        hook.await.unwrap();
        reply
    }

    fn proposal_post(_app: &AppState) -> axum::http::Request<axum::body::Body> {
        post(
            CONV,
            proposal_card(TITLE, "Nueva tarea", Some("## Idea\nreintentar")),
        )
    }

    #[tokio::test]
    async fn a_failed_start_keeps_the_backlog_task_with_autopilot_on() {
        let app = app().await;
        // The project's folder is not there: the start fails.
        project_and_conversation(&app).await;
        let reply = crear_with(&app, Some("autopilot"), "Crear").await;
        let key = reply["task"].as_str().unwrap();
        assert!(reply["error"].is_string(), "{reply}");
        let task = app
            .store
            .get_task(&TaskKey::new(key))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(task.status, delilah_core::TaskStatus::Queued);
        assert!(task.autopilot.is_some());
        assert!(task.saving_mode);
    }

    #[tokio::test]
    async fn launch_is_ignored_unless_the_answer_is_crear() {
        let app = app().await;
        project_and_conversation(&app).await;
        let reply = crear_with(&app, Some("autopilot"), "Descartar").await;
        assert_eq!(reply["task"], Value::Null);
        assert!(reply.get("error").is_none());
        assert_eq!(app.store.list_tasks().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn crear_without_launch_creates_a_plain_backlog_task() {
        let app = app().await;
        project_and_conversation(&app).await;
        let reply = crear_with(&app, None, "Crear").await;
        let key = reply["task"].as_str().unwrap();
        let task = app
            .store
            .get_task(&TaskKey::new(key))
            .await
            .unwrap()
            .unwrap();
        assert!(task.autopilot.is_none());
        assert_eq!(task.status, delilah_core::TaskStatus::Queued);
    }

    #[tokio::test]
    async fn another_workflow_gets_the_task_and_the_refusal() {
        let app = app().await;
        project_and_conversation(&app).await;
        sqlx_set_workflow(&app).await;
        let reply = crear_with(&app, Some("autopilot"), "Crear").await;
        let key = reply["task"].as_str().unwrap();
        assert!(
            reply["error"]
                .as_str()
                .unwrap()
                .contains("workflow de spec")
        );
        let task = app
            .store
            .get_task(&TaskKey::new(key))
            .await
            .unwrap()
            .unwrap();
        assert!(task.autopilot.is_none());
        assert_eq!(task.status, delilah_core::TaskStatus::Queued);
    }

    async fn conversation(app: &AppState) {
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

    /// Replaces the `web` project by one on another workflow.
    async fn sqlx_set_workflow(app: &AppState) {
        app.store.delete_project("web").await.unwrap();
        app.store
            .create_project(&delilah_core::Project {
                name: "web".into(),
                path: "/w/web".into(),
                kind: delilah_core::ProjectKind::Repo,
                workflow: "prompt".into(),
                preview_command: None,
            })
            .await
            .unwrap();
        conversation(app).await;
    }

    #[tokio::test]
    async fn crear_and_launch_starts_a_fresh_autopilot_run_in_the_project() {
        fn git(dir: &std::path::Path, args: &[&str]) {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(dir)
                .args(["-c", "user.name=t", "-c", "user.email=t@t"])
                .args(args)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?}");
        }
        let tmp = tempfile::TempDir::new().unwrap();
        let home = tmp.path().to_path_buf();
        let repo = home.join("web");
        let fake = home.join("fake-claude.sh");
        std::fs::write(
            &fake,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nsleep 1\n",
                home.join("args.txt").display()
            ),
        )
        .unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("README.md"), "web\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-qm", "init"]);
        let config = crate::config::Config {
            home: home.clone(),
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            agent_program: Some(fake),
            claude_projects: crate::config::Config::scratch_projects(),
            claude_bin: "claude".into(),
        };
        let app = AppState::new(crate::store::Store::in_memory().await.unwrap(), config);
        project_and_conversation(&app).await;
        app.store.delete_project("web").await.unwrap();
        app.store
            .create_project(&delilah_core::Project {
                name: "web".into(),
                path: repo.clone(),
                kind: delilah_core::ProjectKind::Repo,
                workflow: "feature".into(),
                preview_command: None,
            })
            .await
            .unwrap();
        conversation(&app).await;

        let reply = crear_with(&app, Some("autopilot"), "Crear").await;
        assert!(reply.get("error").is_none(), "{reply}");
        let key = reply["task"].as_str().unwrap().to_string();
        let task = app
            .store
            .get_task(&TaskKey::new(&key))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(task.status, delilah_core::TaskStatus::Running);
        assert!(task.autopilot.is_some());
        assert!(task.saving_mode);
        // The default work mode: a worktree, not the project's checkout.
        assert_eq!(task.in_checkout, Some(false));
        // The brief reaches the first session: it is in the notes file the prompt points to.
        let notes = std::fs::read_to_string(app.config.task_dir(&key).join("notes.md")).unwrap();
        assert!(notes.ends_with("\n\n## Idea\nreintentar"), "{notes}");
        let mut args = String::new();
        for _ in 0..50 {
            args = std::fs::read_to_string(home.join("args.txt")).unwrap_or_default();
            if !args.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert!(args.contains("notes.md"), "{args}");
        assert!(reply["task"].is_string());
        // A fresh session of the task; the conversation is still the project's.
        let conv = app.store.session("conv").await.unwrap().unwrap();
        assert_eq!(conv.task, "@web");
        for live in app.sessions.of_task(&key) {
            let _ = live.pty.kill();
        }
    }

    #[tokio::test]
    async fn a_closed_session_of_the_project_cannot_post_a_proposal() {
        let app = app().await;
        project_and_conversation(&app).await;
        let qs =
            proposal_card(TITLE, NEW_TASK_HEADER, Some("b"))["tool_input"]["questions"].clone();
        assert!(for_hook(&app, "@web", Some("conv"), &qs).await.is_some());
        app.store.close_session("conv").await.unwrap();
        assert!(for_hook(&app, "@web", Some("conv"), &qs).await.is_none());
    }

    #[test]
    fn the_texts_follow_the_app_language() {
        assert_eq!(
            created_text(Lang::Es, "K-1"),
            "Crear — creada K-1 en el backlog"
        );
        assert_eq!(
            created_text(Lang::En, "K-1"),
            "Crear — created K-1 in the backlog"
        );
        assert_eq!(
            launched_text(Lang::Es, "K-1"),
            "Crear — creada K-1 y lanzada en autopiloto"
        );
        assert_eq!(
            launched_text(Lang::En, "K-1"),
            "Crear — created K-1 and launched on autopilot"
        );
        assert!(not_launched_text(Lang::Es, "K-1", "x").ends_with("no se pudo lanzar: x"));
        assert!(not_launched_text(Lang::En, "K-1", "x").ends_with("could not launch it: x"));
        assert_eq!(failed_text(Lang::Es, "x"), "Crear — no se pudo crear: x");
        assert_eq!(failed_text(Lang::En, "x"), "Crear — could not create: x");
    }

    #[tokio::test]
    async fn the_refusal_follows_the_app_language() {
        for (lang, expected) in [
            (Lang::Es, "no está en el workflow de spec"),
            (Lang::En, "is not on the spec workflow"),
        ] {
            let app = app().await;
            app.lang.set(lang);
            project_and_conversation(&app).await;
            sqlx_set_workflow(&app).await;
            let reply = crear_with(&app, Some("autopilot"), "Crear").await;
            let error = reply["error"].as_str().unwrap();
            assert!(error.contains(expected), "{error}");
            assert!(error.starts_with("LIBRE-"), "{error}");
        }
    }
}
