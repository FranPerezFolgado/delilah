//! An idea conversation's history: what you said, what Claude answered, the cards it
//! asked, what it is doing now, and the messages waiting for its turn to end. Kept
//! from the session's own hooks and from the view's requests.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use delilah_agents::claude::Signal;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::ApiError;
use crate::asks::Ask;
use crate::events::Topic;
use crate::sessions::{Live, LiveState};
use crate::state::AppState;
use crate::store_sessions::EntryKind;

/// The conversation's session record, or the 404 / 409 of the contract.
async fn conversation_of(app: &AppState, id: &str) -> Result<(), ApiError> {
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
    conversation_of(&app, &id).await?;
    let entries = app.store.entries(&id).await?;
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
    conversation_of(&app, &id).await?;
    let text = message.text.trim();
    if text.is_empty() {
        return Err(ApiError::bad_request("the message is empty"));
    }
    let live = app
        .sessions
        .get(&id)
        .ok_or_else(|| ApiError::conflict("the session is not running; reopen it to write"))?;
    let queued = live.state() == LiveState::Working;
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
    }
    app.emit(Topic::Conversations);
    Ok((StatusCode::CREATED, Json(json!(entry))))
}

/// `DELETE /api/v1/sessions/{id}/messages/{entry}`: takes back a queued message.
pub async fn remove(
    State(app): State<AppState>,
    Path((id, entry)): Path<(String, i64)>,
) -> Result<StatusCode, ApiError> {
    conversation_of(&app, &id).await?;
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
    let live = app
        .sessions
        .get(&id)
        .ok_or_else(|| ApiError::conflict("the session is not running"))?;
    live.pty
        .write(b"\x1b")
        .map_err(|e| ApiError::internal(e.to_string()))?;
    live.set_state(LiveState::Waiting);
    live.set_activity(None);
    live.set_needs_terminal(false);
    for ask in app.asks.release_session(&id) {
        card_update(&app, &id, &ask, json!({ "outcome": "withdrawn" })).await;
    }
    let _ = app.store.set_attention(&id, None).await;
    flush_queue(&app, &live).await;
    app.emit(Topic::Conversations);
    app.emit(Topic::Inbox);
    Ok(StatusCode::NO_CONTENT)
}

/// Types every queued message at once, joined by a blank line, and marks them sent.
pub async fn flush_queue(app: &AppState, live: &Live) {
    let queued = match app.store.queued_entries(&live.id).await {
        Ok(q) if !q.is_empty() => q,
        Ok(_) => return,
        Err(e) => {
            tracing::warn!(session = %live.id, "cannot read the queue: {e}");
            return;
        }
    };
    let text = queued
        .iter()
        .filter_map(|e| e.body["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    // Marked first: the prompt hook that follows must find them as `sent`.
    for entry in &queued {
        let mut body = entry.body.clone();
        body["state"] = json!("sent");
        if let Err(e) = app.store.update_entry(entry.id, &body).await {
            tracing::warn!(session = %live.id, "cannot mark a message sent: {e}");
        }
    }
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
    match signal {
        Signal::PromptSubmitted => {
            if let Some(text) = hook["prompt"]
                .as_str()
                .map(str::trim)
                .filter(|t| !t.is_empty())
                && let Err(e) = prompt_submitted(app, session, text).await
            {
                tracing::warn!(session, "cannot record the prompt: {e}");
            }
            if let Some(live) = live {
                live.set_activity(None);
                live.set_needs_terminal(false);
            }
        }
        Signal::Working => {
            if let Some(live) = live {
                live.set_activity(activity_of(hook));
                live.set_needs_terminal(false);
            }
        }
        Signal::NeedsUser => {
            if hook["notification_type"].as_str() == Some("permission_prompt")
                && let Some(live) = live
            {
                live.set_needs_terminal(true);
            }
        }
        Signal::TurnEnded | Signal::TurnFailed | Signal::SessionEnded => {
            if let Some(live) = live {
                live.set_activity(None);
                live.set_needs_terminal(false);
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
                }
                if let Some(live) = live {
                    flush_queue(app, live).await;
                }
            }
        }
        Signal::SessionStarted | Signal::Other => return,
    }
    app.emit(Topic::Conversations);
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
    for n in 1..=sent.len() {
        let joined = sent[..n]
            .iter()
            .map(|e| e.body["text"].as_str().unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\n\n");
        if joined.trim() == text {
            for entry in &sent[..n] {
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

/// A card of a conversation's session enters the history as pending.
pub async fn card_added(app: &AppState, session: &str, ask: &Ask) {
    if !is_conversation(app, session).await {
        return;
    }
    let body = json!({
        "ask": ask.id,
        "questions": ask.questions,
        "proposal": ask.proposal,
        "outcome": "pending",
        "answer": null,
        "task": null,
        "error": null,
    });
    if let Err(e) = app.store.add_entry(session, EntryKind::Card, &body).await {
        tracing::warn!(session, "cannot record the card: {e}");
    }
    app.emit(Topic::Conversations);
}

/// Merges `patch` into the card of `ask`, if it is in a conversation's history.
pub async fn card_update(app: &AppState, session: &str, ask: &str, patch: Value) {
    let Ok(entries) = app.store.entries(session).await else {
        return;
    };
    let Some(card) = entries
        .into_iter()
        .find(|e| e.kind == EntryKind::Card && e.body["ask"] == ask)
    else {
        return;
    };
    let mut body = card.body;
    if let (Some(body), Some(patch)) = (body.as_object_mut(), patch.as_object()) {
        body.extend(patch.clone());
    }
    if let Err(e) = app.store.update_entry(card.id, &body).await {
        tracing::warn!(session, "cannot update the card: {e}");
    }
    app.emit(Topic::Conversations);
}

async fn is_conversation(app: &AppState, session: &str) -> bool {
    matches!(app.store.session(session).await, Ok(Some(r)) if r.conversation)
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
    async fn the_conversation_of_an_unknown_or_plain_session_is_refused() {
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
        assert_eq!(status, StatusCode::CONFLICT);
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
