//! An idea conversation's history: what you said, what Claude answered, the cards it
//! asked, what it is doing now, and the messages waiting for its turn to end. Kept
//! from the session's own hooks and from the view's requests.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use delilah_agents::claude::Signal;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::api::ApiError;
use crate::events::Topic;
use crate::proposals::Proposal;
use crate::sessions::{Live, LiveState};
use crate::state::AppState;
use crate::store_sessions::EntryKind;

/// Longest message taken from the view, in characters.
const MAX_MESSAGE_CHARS: usize = 16_000;

/// The conversation's session record, or the 404 / 409 of the contract.
async fn ensure_conversation(app: &AppState, id: &str) -> Result<(), ApiError> {
    let record = app
        .store
        .session(id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("no session {id}")))?;
    if !record.conversation {
        return Err(ApiError::conflict("that session is not a conversation"));
    }
    Ok(())
}

/// `GET /api/v1/sessions/{id}/conversation`
pub async fn get(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ensure_conversation(&app, &id).await?;
    let mut entries = json!(app.store.entries(&id).await?);
    // Asks live in memory: a card whose ask is gone cannot be answered any more.
    for entry in entries.as_array_mut().into_iter().flatten() {
        if entry["kind"] == "card"
            && entry["outcome"] == "pending"
            && !entry["ask"]
                .as_str()
                .is_some_and(|ask| app.asks.get(ask).is_some())
        {
            entry["outcome"] = json!("withdrawn");
        }
    }
    let live = app.sessions.get(&id);
    Ok(Json(json!({
        "entries": entries,
        "activity": live.as_ref().and_then(Live::activity),
        "needs_terminal": live.as_ref().is_some_and(Live::needs_terminal),
    })))
}

#[derive(Debug, Deserialize)]
pub struct Message {
    pub text: String,
}

/// `POST /api/v1/sessions/{id}/messages`: typed now when Claude is idle, queued for
/// the end of its turn when it is working.
pub async fn send(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(message): Json<Message>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    ensure_conversation(&app, &id).await?;
    // What reaches the terminal is what is recorded: no control characters in it.
    let text = crate::actions::sanitize_prompt(message.text.trim());
    let text = text.trim();
    if text.is_empty() {
        return Err(ApiError::bad_request("the message is empty"));
    }
    if text.chars().count() > MAX_MESSAGE_CHARS {
        return Err(ApiError::bad_request(format!(
            "the message is longer than {MAX_MESSAGE_CHARS} characters"
        )));
    }
    let live = app
        .sessions
        .get(&id)
        .ok_or_else(|| ApiError::conflict("the session is not running; reopen it to write"))?;
    // A pending card holds the turn: what is typed now would land in the PTY under it.
    let queued = live.state() == LiveState::Working || app.asks.has_session(&id);
    let state = if queued { "queued" } else { "sent" };
    // Recorded before typing: its prompt hook may come back before this returns.
    let entry = app
        .store
        .add_entry(
            &id,
            EntryKind::User,
            &json!({ "text": text, "state": state, "from": "view" }),
        )
        .await?;
    if !queued {
        crate::actions::type_prompt(&live, text).await?;
        let _ = app.store.set_attention(&id, None).await;
        app.emit(Topic::Inbox);
    } else if live.state() != LiveState::Working && !app.asks.has_session(&id) {
        // The turn ended between reading the state and recording the entry: its Stop
        // hook has already flushed the queue without this message, so flush again.
        flush_queue(&app, &live).await;
    }
    app.emit(Topic::Conversations);
    Ok((StatusCode::CREATED, Json(json!(entry))))
}

/// `DELETE /api/v1/sessions/{id}/messages/{entry}`: takes back a queued message.
pub async fn remove(
    State(app): State<AppState>,
    Path((id, entry)): Path<(String, i64)>,
) -> Result<StatusCode, ApiError> {
    ensure_conversation(&app, &id).await?;
    if !app.store.delete_queued_entry(&id, entry).await? {
        return Err(ApiError::conflict("only a queued message can be removed"));
    }
    app.emit(Topic::Conversations);
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/v1/sessions/{id}/stop`: interrupts the current turn. Claude fires no
/// hook after an ESC, so the session is put back to waiting here, its pending cards
/// withdrawn and whatever was queued typed at once.
pub async fn stop(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    ensure_conversation(&app, &id).await?;
    let live = app
        .sessions
        .get(&id)
        .ok_or_else(|| ApiError::conflict("the session is not running"))?;
    live.pty
        .write(b"\x1b")
        .map_err(|e| ApiError::internal(e.to_string()))?;
    live.set_state(LiveState::Waiting);
    live.reset_turn_state();
    for ask in app.asks.release_session(&id) {
        card_outcome(&app, &id, &ask, CardOutcome::Withdrawn).await;
    }
    let _ = app.store.set_attention(&id, None).await;
    flush_queue(&app, &live).await;
    app.emit(Topic::Inbox);
    Ok(StatusCode::NO_CONTENT)
}

/// Types every queued message at once, joined by a blank line, and marks them sent.
/// The store hands each queued message to one caller only, so two flushes (the Stop
/// hook and a message's own re-flush) never type the same one twice.
pub async fn flush_queue(app: &AppState, live: &Live) {
    // Marked sent as they are claimed: the prompt hook that follows must find them so.
    let claimed = match app.store.claim_queued(&live.id).await {
        Ok(c) if !c.is_empty() => c,
        Ok(_) => return,
        Err(e) => {
            tracing::warn!(session = %live.id, "cannot read the queue: {e}");
            return;
        }
    };
    let text = claimed
        .iter()
        .filter_map(|e| e.body["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    if let Err(e) = crate::actions::type_prompt(live, &text).await {
        tracing::warn!(session = %live.id, "cannot type the queue: {}", e.message());
    }
    app.emit(Topic::Conversations);
}

/// What the hook shows of a tool, for "what Claude is doing".
fn activity_of(hook: &Value) -> Option<String> {
    let tool = hook["tool_name"].as_str()?;
    let input = &hook["tool_input"];
    let field = |name: &str| input[name].as_str().filter(|s| !s.is_empty());
    Some(match tool {
        "Read" => format!("Leyendo {}", field("file_path")?),
        "Grep" | "Glob" => format!("Buscando {}", field("pattern")?),
        "Bash" => field("command")?.chars().take(40).collect(),
        other => other.to_string(),
    })
}

/// Records what a conversation session's hook says: the prompts typed in its
/// terminal, Claude's closing message of each turn, its activity, and whether it
/// waits on the terminal.
pub async fn on_hook(
    app: &AppState,
    session: &str,
    live: Option<&Live>,
    signal: Signal,
    hook: &Value,
) {
    // The live state emits by itself when it changes; entries recorded emit here.
    match signal {
        Signal::PromptSubmitted => {
            if let Some(text) = hook["prompt"]
                .as_str()
                .map(str::trim)
                .filter(|t| !t.is_empty())
            {
                if let Err(e) = prompt_submitted(app, session, text).await {
                    tracing::warn!(session, "cannot record the prompt: {e}");
                }
                app.emit(Topic::Conversations);
            }
            if let Some(live) = live {
                live.reset_turn_state();
            }
        }
        Signal::Working => {
            if let Some(live) = live {
                let activity = activity_of(hook);
                live.update_convo(|c| {
                    c.activity = activity;
                    c.needs_terminal = false;
                });
            }
        }
        Signal::NeedsUser => {
            if hook["notification_type"].as_str() == Some("permission_prompt")
                && let Some(live) = live
            {
                live.update_convo(|c| c.needs_terminal = true);
            }
        }
        Signal::TurnEnded | Signal::TurnFailed | Signal::SessionEnded => {
            if let Some(live) = live {
                live.reset_turn_state();
            }
            if signal == Signal::TurnEnded {
                if let Some(text) = hook["last_assistant_message"]
                    .as_str()
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                {
                    let body = json!({ "text": text });
                    if let Err(e) = app.store.add_entry(session, EntryKind::Claude, &body).await {
                        tracing::warn!(session, "cannot record Claude's message: {e}");
                    }
                    app.emit(Topic::Conversations);
                }
                if let Some(live) = live {
                    flush_queue(app, live).await;
                }
            }
        }
        Signal::SessionStarted | Signal::Other => {}
    }
}

/// A prompt reached Claude: if it is what the view sent (alone or the queue joined),
/// those entries are delivered; otherwise it was typed in the terminal.
async fn prompt_submitted(
    app: &AppState,
    session: &str,
    text: &str,
) -> Result<(), crate::store::StoreError> {
    let sent: Vec<_> = app
        .store
        .entries(session)
        .await?
        .into_iter()
        .filter(|e| e.kind == EntryKind::User && e.body["state"] == "sent")
        .collect();
    let joined = |run: &[crate::store_sessions::Entry]| {
        run.iter()
            .map(|e| e.body["text"].as_str().unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    // A contiguous run of sent entries, the one ending newest first. A sent entry
    // that never showed in a prompt hook must not hide every later prompt.
    for end in (1..=sent.len()).rev() {
        for start in 0..end {
            if joined(&sent[start..end]).trim() != text {
                continue;
            }
            // What was sent before the match and never showed up is stale.
            for entry in &sent[..end] {
                let mut body = entry.body.clone();
                body["state"] = json!("delivered");
                app.store.update_entry(entry.id, &body).await?;
            }
            return Ok(());
        }
    }
    app.store
        .add_entry(
            session,
            EntryKind::User,
            &json!({ "text": text, "state": "delivered", "from": "terminal" }),
        )
        .await?;
    Ok(())
}

/// How a card stands: waiting for your pick, picked, answered in the terminal, or
/// withdrawn because the turn was stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CardOutcome {
    Pending,
    Answered,
    Terminal,
    Withdrawn,
}

/// The body of a card entry (see data-model.md).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    pub ask: String,
    pub questions: Value,
    pub proposal: Option<Proposal>,
    pub outcome: CardOutcome,
    pub answer: Option<Map<String, Value>>,
    pub task: Option<String>,
    pub error: Option<String>,
}

impl Card {
    pub fn pending(ask: &str, questions: &Value, proposal: Option<Proposal>) -> Self {
        Self {
            ask: ask.to_string(),
            questions: questions.clone(),
            proposal,
            outcome: CardOutcome::Pending,
            answer: None,
            task: None,
            error: None,
        }
    }
}

/// Whether the session is an idea conversation: its live state knows; a session that
/// is not running is read from its record.
pub async fn is_conversation(app: &AppState, session: &str) -> bool {
    match app.sessions.get(session) {
        Some(live) => live.is_conversation(),
        None => matches!(app.store.session(session).await, Ok(Some(r)) if r.conversation),
    }
}

/// A card of a conversation's session enters the history as pending.
pub async fn card_added(app: &AppState, session: &str, card: &Card) {
    if !is_conversation(app, session).await {
        return;
    }
    let body = match serde_json::to_value(card) {
        Ok(body) => body,
        Err(e) => {
            tracing::warn!(session, "cannot record the card: {e}");
            return;
        }
    };
    if let Err(e) = app.store.add_entry(session, EntryKind::Card, &body).await {
        tracing::warn!(session, "cannot record the card: {e}");
    }
    app.emit(Topic::Conversations);
}

/// The card of `ask` ended without an answer from Delilah.
pub async fn card_outcome(app: &AppState, session: &str, ask: &str, outcome: CardOutcome) {
    card_update(app, session, ask, |card| card.outcome = outcome).await;
}

/// The card of `ask` is withdrawn if nobody has answered it yet.
pub async fn card_withdraw_if_pending(app: &AppState, session: &str, ask: &str) {
    card_update(app, session, ask, |card| {
        if card.outcome == CardOutcome::Pending {
            card.outcome = CardOutcome::Withdrawn;
        }
    })
    .await;
}

/// The card of `ask` was answered; `task` / `error` tell how creating a task went.
pub async fn card_answered(
    app: &AppState,
    session: &str,
    ask: &str,
    answer: Map<String, Value>,
    task: Option<String>,
    error: Option<String>,
) {
    card_update(app, session, ask, |card| {
        card.outcome = CardOutcome::Answered;
        card.answer = Some(answer);
        card.task = task;
        card.error = error;
    })
    .await;
}

/// Changes the card of `ask`, if it is in a conversation's history.
async fn card_update(app: &AppState, session: &str, ask: &str, change: impl FnOnce(&mut Card)) {
    let Ok(Some(entry)) = app.store.card_entry(session, ask).await else {
        return;
    };
    let Ok(mut card) = serde_json::from_value::<Card>(entry.body) else {
        tracing::warn!(session, "a card in the history is malformed");
        return;
    };
    change(&mut card);
    match serde_json::to_value(&card) {
        Ok(body) => {
            if let Err(e) = app.store.update_entry(entry.id, &body).await {
                tracing::warn!(session, "cannot update the card: {e}");
            }
        }
        Err(e) => tracing::warn!(session, "cannot update the card: {e}"),
    }
    app.emit(Topic::Conversations);
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use axum::body::Body;
    use axum::http::Request;
    use portable_pty::CommandBuilder;
    use tower::ServiceExt;

    use super::*;
    use crate::config::Config;
    use crate::store::Store;
    use crate::store_sessions::{SessionKind, SessionRecord};

    const HOOK: &str = "/hooks/claude?task=@web&session=conv";

    fn config() -> Config {
        Config {
            home: std::env::temp_dir().join(format!("delilah-conv-{}", uuid::Uuid::new_v4())),
            addr: "127.0.0.1:7420".parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            agent_program: None,
        }
    }

    fn record(id: &str, conversation: bool) -> SessionRecord {
        SessionRecord {
            id: id.into(),
            task: "@web".into(),
            kind: SessionKind::Claude,
            label: "Idea".into(),
            primary: false,
            closed: false,
            read_only: false,
            conversation,
            step: None,
        }
    }

    struct Fixture {
        app: AppState,
        /// Everything written to the session's terminal.
        typed: PathBuf,
        live: Live,
    }

    impl Fixture {
        /// A conversation session "conv" whose terminal is a raw `cat` into a file.
        async fn new() -> Self {
            let app = AppState::new(Store::in_memory().await.unwrap(), config());
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
                .create_session(&record("conv", true))
                .await
                .unwrap();
            let typed =
                std::env::temp_dir().join(format!("delilah-typed-{}", uuid::Uuid::new_v4()));
            let mut cmd = CommandBuilder::new("bash");
            cmd.args([
                "--noprofile",
                "--norc",
                "-c",
                &format!("stty raw -echo; exec cat > {}", typed.display()),
            ]);
            let pty = delilah_agents::pty::PtySession::spawn(cmd, 80, 24, None).unwrap();
            let live = app
                .sessions
                .insert("conv", "@web", SessionKind::Claude, false, pty);
            live.set_conversation(true);
            // The file exists once `stty` has run.
            for _ in 0..100 {
                if typed.exists() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Self { app, typed, live }
        }

        async fn call(&self, req: Request<Body>) -> (StatusCode, Value) {
            let res = crate::api::router(self.app.clone())
                .oneshot(req)
                .await
                .unwrap();
            let status = res.status();
            let body = axum::body::to_bytes(res.into_body(), usize::MAX)
                .await
                .unwrap();
            (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
        }

        async fn post(&self, path: &str, body: Value) -> (StatusCode, Value) {
            let req = Request::post(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap();
            self.call(req).await
        }

        async fn hook(&self, body: Value) {
            assert_eq!(self.post(HOOK, body).await.0, StatusCode::NO_CONTENT);
        }

        async fn send(&self, text: &str) -> (StatusCode, Value) {
            self.post("/api/v1/sessions/conv/messages", json!({ "text": text }))
                .await
        }

        async fn conversation(&self) -> Value {
            let req = Request::get("/api/v1/sessions/conv/conversation")
                .body(Body::empty())
                .unwrap();
            let (status, body) = self.call(req).await;
            assert_eq!(status, StatusCode::OK);
            body
        }

        /// The terminal's input, once it has `len` bytes.
        async fn typed(&self, len: usize) -> String {
            for _ in 0..100 {
                let got = std::fs::read(&self.typed).unwrap_or_default();
                if got.len() >= len {
                    return String::from_utf8_lossy(&got).into_owned();
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            String::from_utf8_lossy(&std::fs::read(&self.typed).unwrap_or_default()).into_owned()
        }

        async fn entries(&self) -> Vec<Value> {
            self.conversation().await["entries"]
                .as_array()
                .unwrap()
                .clone()
        }
    }

    fn stop(message: &str) -> Value {
        json!({ "hook_event_name": "Stop", "last_assistant_message": message })
    }

    #[tokio::test]
    async fn the_ensure_conversation_an_unknown_or_plain_session_is_refused() {
        let f = Fixture::new().await;
        f.app
            .store
            .create_session(&record("plain", false))
            .await
            .unwrap();
        let get = |id: &str| {
            Request::get(format!("/api/v1/sessions/{id}/conversation"))
                .body(Body::empty())
                .unwrap()
        };
        assert_eq!(f.call(get("nope")).await.0, StatusCode::NOT_FOUND);
        assert_eq!(f.call(get("plain")).await.0, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn the_conversation_lists_its_entries_in_order() {
        let f = Fixture::new().await;
        f.app
            .store
            .add_entry(
                "conv",
                EntryKind::User,
                &json!({ "text": "idea", "state": "delivered", "from": "view" }),
            )
            .await
            .unwrap();
        f.hook(stop("hola")).await;
        let body = f.conversation().await;
        let entries = body["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["kind"], "user");
        assert_eq!(entries[0]["text"], "idea");
        assert_eq!(entries[1]["kind"], "claude");
        assert_eq!(entries[1]["text"], "hola");
        assert_eq!(body["activity"], Value::Null);
        assert_eq!(body["needs_terminal"], false);
    }

    #[tokio::test]
    async fn messages_sent_while_idle_are_typed_and_recorded_sent() {
        let f = Fixture::new().await;
        f.live.set_state(LiveState::Waiting);
        let (status, entry) = f.send("  hola  ").await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(entry["state"], "sent");
        assert_eq!(entry["from"], "view");
        assert_eq!(entry["text"], "hola");
        assert_eq!(f.typed(5).await, "hola\r");
        assert_eq!(f.entries().await.len(), 1);
        assert_eq!(f.send("  ").await.0, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn messages_sent_while_working_are_queued_and_typed_together_after_stop() {
        let f = Fixture::new().await;
        f.live.set_state(LiveState::Working);
        for text in ["a", "b"] {
            let (status, entry) = f.send(text).await;
            assert_eq!(status, StatusCode::CREATED);
            assert_eq!(entry["state"], "queued");
        }
        assert_eq!(
            std::fs::read(&f.typed).unwrap_or_default(),
            b"",
            "nothing typed yet"
        );

        f.hook(stop("listo")).await;
        let expected = "\x1b[200~a\n\nb\x1b[201~\r";
        assert_eq!(f.typed(expected.len()).await, expected);
        let states: Vec<_> = f
            .entries()
            .await
            .iter()
            .filter(|e| e["kind"] == "user")
            .map(|e| e["state"].clone())
            .collect();
        assert_eq!(states, [json!("sent"), json!("sent")]);
    }

    #[tokio::test]
    async fn a_queued_message_can_be_removed_but_a_sent_one_cannot() {
        let f = Fixture::new().await;
        f.live.set_state(LiveState::Working);
        let queued = f.send("later").await.1["id"].as_i64().unwrap();
        f.live.set_state(LiveState::Waiting);
        let sent = f.send("now").await.1["id"].as_i64().unwrap();
        let delete = |id: i64| {
            Request::delete(format!("/api/v1/sessions/conv/messages/{id}"))
                .body(Body::empty())
                .unwrap()
        };
        assert_eq!(f.call(delete(sent)).await.0, StatusCode::CONFLICT);
        assert_eq!(f.call(delete(queued)).await.0, StatusCode::NO_CONTENT);
        assert_eq!(f.call(delete(queued)).await.0, StatusCode::CONFLICT);
        assert_eq!(f.entries().await.len(), 1);
    }

    #[tokio::test]
    async fn stop_writes_escape_withdraws_pending_cards_and_flushes_the_queue() {
        let f = Fixture::new().await;
        f.live.set_state(LiveState::Working);
        f.live.set_activity(Some("Leyendo x".into()));
        f.send("later").await;
        let card = json!({ "hook_event_name": "PreToolUse", "tool_name": "AskUserQuestion", "tool_input": { "questions": [{
            "question": "¿Cuál?", "header": "H", "multiSelect": false,
            "options": [{ "label": "A", "description": "a" }, { "label": "B", "description": "b" }]
        }]}});
        let ask = tokio::spawn({
            let f = Fixture {
                app: f.app.clone(),
                typed: f.typed.clone(),
                live: f.live.clone(),
            };
            async move {
                f.post("/hooks/claude/ask?task=@web&session=conv", card)
                    .await
            }
        });
        for _ in 0..100 {
            if !f.app.asks.list().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(f.app.asks.list().len(), 1);

        let (status, _) = f.post("/api/v1/sessions/conv/stop", json!({})).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let expected = "\x1blater\r";
        assert_eq!(f.typed(expected.len()).await, expected);
        assert!(f.app.asks.list().is_empty());
        assert_eq!(ask.await.unwrap().1, json!({}), "the hook lets go");
        let entries = f.entries().await;
        let card = entries.iter().find(|e| e["kind"] == "card").unwrap();
        assert_eq!(card["outcome"], "withdrawn");
        let user = entries.iter().find(|e| e["kind"] == "user").unwrap();
        assert_eq!(user["state"], "sent");
        assert_eq!(f.conversation().await["activity"], Value::Null);
        // The flush typed a prompt: working again.
        assert_eq!(f.live.state(), LiveState::Working);
    }

    #[tokio::test]
    async fn stop_without_a_queue_leaves_the_session_waiting() {
        let f = Fixture::new().await;
        f.live.set_state(LiveState::Working);
        let (status, _) = f.post("/api/v1/sessions/conv/stop", json!({})).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(f.typed(1).await, "\x1b");
        assert_eq!(f.live.state(), LiveState::Waiting);
        let (status, _) = f.post("/api/v1/sessions/other/stop", json!({})).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn stop_refuses_a_session_that_is_not_a_conversation() {
        let f = Fixture::new().await;
        f.app
            .store
            .create_session(&record("plain", false))
            .await
            .unwrap();
        let (status, _) = f.post("/api/v1/sessions/plain/stop", json!({})).await;
        assert_eq!(status, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn messages_lose_control_characters_and_are_capped() {
        let f = Fixture::new().await;
        f.live.set_state(LiveState::Waiting);
        assert_eq!(f.send("\x1b\x03\x04").await.0, StatusCode::BAD_REQUEST);
        let long = "x".repeat(16_001);
        assert_eq!(f.send(&long).await.0, StatusCode::BAD_REQUEST);
        let (status, entry) = f.send("a\x1b[200~b").await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(entry["text"], "a[200~b");
        assert_eq!(f.typed(8).await, "a[200~b\r");
    }

    #[tokio::test]
    async fn another_sessions_queued_message_cannot_be_removed() {
        let f = Fixture::new().await;
        f.app
            .store
            .create_session(&record("other", true))
            .await
            .unwrap();
        let theirs = f
            .app
            .store
            .add_entry(
                "other",
                EntryKind::User,
                &json!({ "text": "x", "state": "queued", "from": "view" }),
            )
            .await
            .unwrap();
        let req = Request::delete(format!("/api/v1/sessions/conv/messages/{}", theirs.id))
            .body(Body::empty())
            .unwrap();
        assert_eq!(f.call(req).await.0, StatusCode::CONFLICT);
        assert_eq!(f.app.store.entries("other").await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_prompt_typed_in_the_terminal_is_recorded_once() {
        let f = Fixture::new().await;
        // As a launch with a prompt leaves it.
        f.app
            .store
            .add_entry(
                "conv",
                EntryKind::User,
                &json!({ "text": "mi idea", "state": "sent", "from": "view" }),
            )
            .await
            .unwrap();
        f.hook(json!({ "hook_event_name": "UserPromptSubmit", "prompt": "mi idea" }))
            .await;
        let entries = f.entries().await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["state"], "delivered");
        assert_eq!(entries[0]["from"], "view");

        f.hook(json!({ "hook_event_name": "UserPromptSubmit", "prompt": "desde la terminal" }))
            .await;
        let entries = f.entries().await;
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1]["text"], "desde la terminal");
        assert_eq!(entries[1]["from"], "terminal");
        assert_eq!(entries[1]["state"], "delivered");
    }

    #[tokio::test]
    async fn an_undelivered_sent_message_does_not_hide_the_next_prompt() {
        let f = Fixture::new().await;
        for text in ["nunca llega", "llega"] {
            let body = json!({ "text": text, "state": "sent", "from": "view" });
            f.app
                .store
                .add_entry("conv", EntryKind::User, &body)
                .await
                .unwrap();
        }
        f.hook(json!({ "hook_event_name": "UserPromptSubmit", "prompt": "llega" }))
            .await;
        let entries = f.entries().await;
        assert_eq!(entries.len(), 2, "no duplicate from the terminal");
        assert!(entries.iter().all(|e| e["state"] == "delivered"));
        assert!(entries.iter().all(|e| e["from"] == "view"));
    }

    #[tokio::test]
    async fn two_flushes_type_each_queued_message_once() {
        let f = Fixture::new().await;
        for text in ["a", "b"] {
            let body = json!({ "text": text, "state": "queued", "from": "view" });
            f.app
                .store
                .add_entry("conv", EntryKind::User, &body)
                .await
                .unwrap();
        }
        tokio::join!(
            flush_queue(&f.app, &f.live),
            flush_queue(&f.app, &f.live),
            flush_queue(&f.app, &f.live)
        );
        let expected = "\x1b[200~a\n\nb\x1b[201~\r";
        assert_eq!(f.typed(expected.len()).await, expected);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(f.typed(0).await, expected, "typed once");
    }

    #[tokio::test]
    async fn a_message_claimed_by_a_flush_cannot_be_removed() {
        let f = Fixture::new().await;
        f.live.set_state(LiveState::Working);
        let id = f.send("later").await.1["id"].as_i64().unwrap();
        f.app.store.claim_queued("conv").await.unwrap();
        let req = Request::delete(format!("/api/v1/sessions/conv/messages/{id}"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(f.call(req).await.0, StatusCode::CONFLICT);
        assert_eq!(f.entries().await[0]["state"], "sent");
    }

    fn card_hook() -> Value {
        json!({ "hook_event_name": "PreToolUse", "tool_name": "AskUserQuestion", "tool_input": { "questions": [{
            "question": "¿Cuál?", "header": "H", "multiSelect": false,
            "options": [{ "label": "A", "description": "a" }, { "label": "B", "description": "b" }]
        }]}})
    }

    #[tokio::test]
    async fn a_message_sent_while_a_card_is_pending_is_queued() {
        let f = Fixture::new().await;
        let ask = tokio::spawn({
            let f = Fixture {
                app: f.app.clone(),
                typed: f.typed.clone(),
                live: f.live.clone(),
            };
            async move {
                f.post("/hooks/claude/ask?task=@web&session=conv", card_hook())
                    .await
            }
        });
        for _ in 0..100 {
            if !f.app.asks.list().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(f.live.state(), LiveState::Waiting);
        let (status, entry) = f.send("espera").await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(entry["state"], "queued");
        assert_eq!(std::fs::read(&f.typed).unwrap_or_default(), b"");
        ask.abort();
    }

    #[tokio::test]
    async fn a_pending_card_without_a_waiting_ask_is_reported_withdrawn() {
        let f = Fixture::new().await;
        let card = Card::pending("ghost", &json!([]), None);
        f.app
            .store
            .add_entry(
                "conv",
                EntryKind::Card,
                &serde_json::to_value(card).unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(f.entries().await[0]["outcome"], "withdrawn");
    }

    #[tokio::test]
    async fn stop_records_claudes_last_message() {
        let f = Fixture::new().await;
        f.hook(stop("Esto es lo que encontré.")).await;
        f.hook(json!({ "hook_event_name": "Stop" })).await;
        let entries = f.entries().await;
        assert_eq!(entries.len(), 1, "a turn without text adds nothing");
        assert_eq!(entries[0]["kind"], "claude");
        assert_eq!(entries[0]["text"], "Esto es lo que encontré.");
    }

    #[tokio::test]
    async fn post_tool_use_sets_activity_and_stop_clears_it() {
        let f = Fixture::new().await;
        f.hook(
            json!({ "hook_event_name": "PostToolUse", "tool_name": "Read",
            "tool_input": { "file_path": "src/a.rs" } }),
        )
        .await;
        assert_eq!(f.conversation().await["activity"], "Leyendo src/a.rs");
        f.hook(stop("ok")).await;
        assert_eq!(f.conversation().await["activity"], Value::Null);
    }

    #[test]
    fn activity_names_what_the_tool_does() {
        let hook = |tool: &str, input: Value| json!({ "tool_name": tool, "tool_input": input });
        assert_eq!(
            activity_of(&hook("Grep", json!({ "pattern": "fn main" }))).as_deref(),
            Some("Buscando fn main")
        );
        assert_eq!(
            activity_of(&hook("Glob", json!({ "pattern": "*.rs" }))).as_deref(),
            Some("Buscando *.rs")
        );
        let long = "x".repeat(60);
        assert_eq!(
            activity_of(&hook("Bash", json!({ "command": long }))).unwrap(),
            "x".repeat(40)
        );
        assert_eq!(
            activity_of(&hook("Edit", json!({}))).as_deref(),
            Some("Edit")
        );
        assert_eq!(activity_of(&json!({})), None);
    }

    #[tokio::test]
    async fn a_permission_prompt_notification_sets_needs_terminal() {
        let f = Fixture::new().await;
        f.hook(json!({ "hook_event_name": "Notification", "notification_type": "idle_prompt" }))
            .await;
        assert_eq!(f.conversation().await["needs_terminal"], false);
        f.hook(
            json!({ "hook_event_name": "Notification", "notification_type": "permission_prompt" }),
        )
        .await;
        assert_eq!(f.conversation().await["needs_terminal"], true);
        f.hook(
            json!({ "hook_event_name": "PostToolUse", "tool_name": "Bash",
            "tool_input": { "command": "ls" } }),
        )
        .await;
        assert_eq!(f.conversation().await["needs_terminal"], false);
    }

    #[tokio::test]
    async fn entries_are_recorded_only_for_conversation_sessions() {
        let f = Fixture::new().await;
        f.app
            .store
            .create_session(&record("plain", false))
            .await
            .unwrap();
        let url = "/hooks/claude?task=@web&session=plain";
        for hook in [
            json!({ "hook_event_name": "UserPromptSubmit", "prompt": "hola" }),
            stop("respuesta"),
        ] {
            assert_eq!(f.post(url, hook).await.0, StatusCode::NO_CONTENT);
        }
        assert!(f.app.store.entries("plain").await.unwrap().is_empty());
        assert!(f.app.store.entries("conv").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn entries_survive_reopening_the_store_on_the_same_file() {
        let config = config();
        std::fs::create_dir_all(&config.home).unwrap();
        let db = config.home.join("delilah.db");
        let app = AppState::new(Store::open(&db).await.unwrap(), config);
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
            .create_session(&record("conv", true))
            .await
            .unwrap();
        let req = Request::post(HOOK)
            .header("content-type", "application/json")
            .body(Body::from(stop("sigo aquí").to_string()))
            .unwrap();
        let res = crate::api::router(app.clone()).oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        drop(app);

        let reopened = Store::open(&db).await.unwrap();
        let entries = reopened.entries("conv").await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].body["text"], "sigo aquí");
    }
}
