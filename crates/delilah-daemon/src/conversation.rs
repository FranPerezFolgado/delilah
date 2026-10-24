//! An idea conversation's history: what you said, what Claude answered, the cards it
//! asked, what it is doing now, and the messages waiting for its turn to end. Kept
//! from the session's own hooks and from the view's requests.

use axum::Json;
use std::collections::HashSet;
use std::path::PathBuf;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncSeekExt};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use delilah_agents::claude::Signal;
use delilah_agents::transcript::{self, Entry as TEntry};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::ApiError;
use crate::events::Topic;
use crate::idea::{CardOutcome, card_outcome};
use crate::sessions::{Live, LiveState};
use crate::state::AppState;
use crate::store_sessions::{EntryKind, SessionKind, SessionRecord};

/// Longest message taken from the view (and longest first prompt), in characters.
pub(crate) const MAX_MESSAGE_CHARS: usize = 16_000;

/// The session the composer writes to, or the 404 / 409 of the contract: any open
/// Claude session, not a shell.
async fn ensure_conversation(app: &AppState, id: &str) -> Result<(), ApiError> {
    let record = readable(app, id).await?;
    if record.closed {
        return Err(ApiError::conflict("the session is closed"));
    }
    Ok(())
}

/// Most transcript entries one call returns; `complete: false` tells there are more.
const PAGE: usize = 500;

/// The session's record, or the 404 / 409 of the contract: a conversation can be read
/// from any Claude session, not from a shell.
async fn readable(app: &AppState, id: &str) -> Result<SessionRecord, ApiError> {
    let record = app
        .store
        .session(id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("no session {id}")))?;
    if record.kind == SessionKind::Shell {
        return Err(ApiError::conflict("a shell session has no conversation"));
    }
    Ok(record)
}

/// Where the session's transcript is meant to be: the path its hooks reported, else the
/// one Claude derives from the workspace and the session id (kept on the live session so
/// it is derived once). Not checked: see [`resolve_transcript`].
pub(crate) async fn candidate_transcript(
    app: &AppState,
    task: &str,
    id: &str,
    live: Option<&Live>,
) -> Option<PathBuf> {
    if let Some(path) = live.and_then(Live::transcript) {
        return Some(path);
    }
    let ws = crate::owners::workspace(app, task).await.ok()?;
    let path = transcript::transcript_path(&ws.path, id)?;
    if let Some(live) = live {
        live.set_transcript(path.clone());
    }
    Some(path)
}

/// The session's transcript, resolved and checked against Claude's projects folder at
/// every read, so a link or file swapped after the hook is refused.
pub(crate) async fn resolve_transcript(
    app: &AppState,
    task: &str,
    id: &str,
    live: Option<&Live>,
) -> Option<PathBuf> {
    let path = candidate_transcript(app, task, id, live).await?;
    transcript::within(&app.config.claude_projects, &path)
}

async fn transcript_file(
    app: &AppState,
    record: &SessionRecord,
    live: Option<&Live>,
) -> Option<PathBuf> {
    resolve_transcript(app, &record.task, &record.id, live).await
}

/// Largest transcript read at all (a bigger one is reported as missing).
const MAX_FILE: u64 = 256 * 1024 * 1024;
/// Most bytes one page reads past its cursor.
const WINDOW: u64 = 8 * 1024 * 1024;
/// Most bytes of a tool call's input or result in the detail response.
const MAX_DETAIL: usize = 1_000_000;

/// The transcript opened, only when it is a regular file of a readable size.
async fn open_transcript(path: &std::path::Path) -> Option<(tokio::fs::File, u64)> {
    // Checked before opening: opening a FIFO would block.
    let ok = |m: &std::fs::Metadata| m.is_file() && m.len() <= MAX_FILE;
    if !tokio::fs::metadata(path).await.ok().is_some_and(|m| ok(&m)) {
        return None;
    }
    let file = tokio::fs::File::open(path).await.ok()?;
    let meta = file.metadata().await.ok()?;
    ok(&meta).then_some((file, meta.len()))
}

/// Up to `WINDOW` bytes from `from` on.
async fn read_window(file: &mut tokio::fs::File, from: u64) -> std::io::Result<Vec<u8>> {
    file.seek(std::io::SeekFrom::Start(from)).await?;
    let mut bytes = Vec::new();
    file.take(WINDOW).read_to_end(&mut bytes).await?;
    Ok(bytes)
}

/// The lines among the first `upto` bytes that mention any of `ids`, newline included;
/// the rest of the file is never held in memory.
async fn lines_mentioning(
    file: tokio::fs::File,
    upto: u64,
    ids: &[&str],
) -> std::io::Result<Vec<u8>> {
    let mut reader = tokio::io::BufReader::new(file.take(upto));
    let (mut out, mut line) = (Vec::new(), Vec::new());
    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line).await? == 0 {
            return Ok(out);
        }
        if ids.iter().any(|id| transcript::line_mentions(&line, id)) {
            out.extend_from_slice(&line);
            if !line.ends_with(b"\n") {
                out.push(b'\n');
            }
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ConversationQuery {
    cursor: Option<String>,
}

fn at_of(entry: &Value) -> DateTime<Utc> {
    entry["at"]
        .as_str()
        .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
        .map_or(DateTime::<Utc>::MIN_UTC, |at| at.with_timezone(&Utc))
}

/// `GET /api/v1/sessions/{id}/conversation`: the transcript's entries (after `cursor`,
/// at most one page) merged by time with the entries the daemon stores (cards, messages
/// from the view), which are always all returned.
///
/// A tool result that comes after the cursor while its tool call came before it is
/// returned as an update: the tool entry again, with its result and the same id.
pub async fn get(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<ConversationQuery>,
) -> Result<Json<Value>, ApiError> {
    let record = readable(&app, &id).await?;
    let offset = match &q.cursor {
        Some(c) => Some(
            c.parse::<u64>()
                .map_err(|_| ApiError::bad_request("the cursor is not a byte offset"))?,
        ),
        None => None,
    };
    let live = app.sessions.get(&id);
    let mut entries: Vec<Value> = Vec::new();
    let path = transcript_file(&app, &record, live.as_ref()).await;
    let opened = match &path {
        Some(path) => open_transcript(path).await,
        None => None,
    };
    let (cursor, complete, state) = match opened {
        Some((mut file, len)) => {
            let from = offset.unwrap_or(0).min(len);
            match read_window(&mut file, from).await {
                Ok(window) => {
                    let page = transcript::parse_page(&window, 0, PAGE);
                    let mut next = usize::try_from(page.next_offset).unwrap_or(usize::MAX);
                    let more_file = from + (window.len() as u64) < len;
                    // A line longer than the window cannot be read whole: skip past it.
                    if next == 0 && more_file && !window.contains(&b'\n') {
                        next = window.len();
                    }
                    let complete =
                        !more_file && !window.get(next..).is_some_and(|rest| rest.contains(&b'\n'));
                    if offset.is_some() && !page.orphan_results.is_empty() {
                        let wanted: HashSet<&str> = page
                            .orphan_results
                            .iter()
                            .map(|(id, _, _)| id.as_str())
                            .collect();
                        let ids: Vec<&str> = wanted.iter().copied().collect();
                        // The earlier tool calls, found among the bytes before the cursor,
                        // then the page's own lines so their results fill them in.
                        if let Some((reopened, _)) = open_transcript(path.as_deref().unwrap()).await
                            && let Ok(mut before) = lines_mentioning(reopened, from, &ids).await
                        {
                            before.extend_from_slice(&window[..next.min(window.len())]);
                            entries.extend(
                                transcript::parse(&before, 0)
                                    .entries
                                    .iter()
                                    .filter(|e| {
                                        matches!(e, TEntry::Tool { tool_use_id, .. } if wanted.contains(tool_use_id.as_str()))
                                    })
                                    .filter_map(|e| serde_json::to_value(e).ok()),
                            );
                        }
                    }
                    entries.extend(
                        page.entries
                            .iter()
                            .filter_map(|e| serde_json::to_value(e).ok()),
                    );
                    (from + next as u64, complete, "present")
                }
                Err(_) => (offset.unwrap_or(0), true, missing(live.as_ref())),
            }
        }
        None => (offset.unwrap_or(0), true, missing(live.as_ref())),
    };
    let mut stored = json!(app.store.entries(&id).await?);
    let has_transcript = state == "present";
    // Asks live in memory: a card whose ask is gone cannot be answered any more.
    for entry in stored.as_array_mut().into_iter().flatten() {
        if entry["kind"] == "card"
            && entry["outcome"] == "pending"
            && !entry["ask"]
                .as_str()
                .is_some_and(|ask| app.asks.get(ask).is_some())
        {
            entry["outcome"] = json!("withdrawn");
        }
        // Ids are strings in the conversation: the row id.
        entry["id"] = json!(entry["id"].to_string());
    }
    let mut stored: Vec<Value> = stored.as_array().cloned().unwrap_or_default();
    // The transcript has the same messages as the stored ones: hidden once it is there
    // (an idea keeps none of its own, a task keeps what is not delivered yet).
    if has_transcript {
        let idea = record.conversation;
        stored.retain(|e| match e["kind"].as_str() {
            Some("claude") => !idea,
            Some("user") => e["state"] == "queued" || (!idea && e["state"] != "delivered"),
            _ => true,
        });
    }
    // An AskUserQuestion call of the transcript is shown as its card, in its place.
    for entry in &mut entries {
        if entry["kind"] != "tool" || entry["tool"] != "AskUserQuestion" {
            continue;
        }
        let Some(tool_use_id) = entry["tool_use_id"].as_str() else {
            continue;
        };
        if let Some(i) = stored
            .iter()
            .position(|c| c["kind"] == "card" && c["tool_use_id"].as_str() == Some(tool_use_id))
        {
            let mut card = stored.remove(i);
            card["at"] = entry["at"].clone();
            *entry = card;
        }
    }
    entries.extend(stored);
    entries.sort_by_cached_key(at_of);
    Ok(Json(json!({
        "entries": entries,
        "cursor": cursor.to_string(),
        "complete": complete,
        "transcript": state,
        "activity": live.as_ref().and_then(Live::activity),
        "needs_terminal": live.as_ref().is_some_and(Live::needs_terminal),
        "runtime": "pty",
    })))
}

/// A transcript file that is not there: expected until the session's first turn.
fn missing(live: Option<&Live>) -> &'static str {
    if live.is_some_and(|l| l.state() == LiveState::Starting) {
        "pending"
    } else {
        "missing"
    }
}

/// `GET /api/v1/sessions/{id}/conversation/tools/{tool_use_id}`: the whole input and
/// result of one tool call, which entries only carry cut.
pub async fn tool(
    State(app): State<AppState>,
    Path((id, tool_use_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let record = readable(&app, &id).await?;
    let not_found = || ApiError::not_found(format!("no tool call {tool_use_id}"));
    if !transcript::is_safe_id(&tool_use_id) {
        return Err(not_found());
    }
    let live = app.sessions.get(&id);
    let path = transcript_file(&app, &record, live.as_ref())
        .await
        .ok_or_else(not_found)?;
    let (file, len) = open_transcript(&path).await.ok_or_else(not_found)?;
    // Only the lines that mention the call are parsed.
    let lines = lines_mentioning(file, len, &[tool_use_id.as_str()])
        .await
        .map_err(|_| not_found())?;
    let detail = transcript::tool_detail(&lines, &tool_use_id).ok_or_else(not_found)?;
    let (result, cut_result) = cap_text(detail.result.unwrap_or_default());
    // An input too big for the response is given as its JSON text, cut.
    let (input, cut_input) = match serde_json::to_string(&detail.input) {
        Ok(text) if text.len() > MAX_DETAIL => {
            let (text, _) = cap_text(text);
            (json!(text), true)
        }
        _ => (detail.input, false),
    };
    Ok(Json(json!({
        "input": input,
        "result": result,
        "is_error": detail.is_error,
        "truncated": cut_result || cut_input,
    })))
}

/// `text` cut to `MAX_DETAIL` bytes (at a character boundary), and whether it was.
fn cap_text(mut text: String) -> (String, bool) {
    if text.len() <= MAX_DETAIL {
        return (text, false);
    }
    let mut end = MAX_DETAIL;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    (text, true)
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
    let mut body = json!(entry);
    body["id"] = json!(entry.id.to_string());
    Ok((StatusCode::CREATED, Json(body)))
}

/// `DELETE /api/v1/sessions/{id}/messages/{entry}`: takes back a queued message.
pub async fn remove(
    State(app): State<AppState>,
    Path((id, entry)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    ensure_conversation(&app, &id).await?;
    // Ids are strings: a stored row's number. A transcript entry (`t:…`) is not one.
    let entry: i64 = entry
        .parse()
        .map_err(|_| ApiError::not_found(format!("no message {entry}")))?;
    if !app.store.delete_queued_entry(&id, entry).await? {
        return Err(ApiError::conflict("only a queued message can be removed"));
    }
    app.emit(Topic::Conversations);
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/v1/sessions/{id}/stop`: interrupts the current turn. Claude fires no
/// hook after an ESC, so the session is put back to waiting here, its pending cards
/// withdrawn. What was queued stays queued: it is typed at the next turn's end.
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

/// Records what a Claude session's hook says: the prompts typed in its
/// terminal, Claude's closing message of each turn, its activity, and whether it
/// waits on the terminal.
pub async fn on_hook(
    app: &AppState,
    session: &str,
    live: Option<&Live>,
    signal: Signal,
    hook: &Value,
) {
    // An idea keeps its whole history; a task's is in its transcript, so only what the
    // view sent is tracked.
    let idea = crate::idea::is_idea(app, session).await;
    // The live state emits by itself when it changes; entries recorded emit here.
    match signal {
        Signal::PromptSubmitted => {
            if let Some(text) = hook["prompt"]
                .as_str()
                .map(str::trim)
                .filter(|t| !t.is_empty())
            {
                if let Err(e) = prompt_submitted(app, session, text, idea).await {
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
            if matches!(
                hook["notification_type"].as_str(),
                Some("permission_prompt" | "elicitation_dialog")
            ) && let Some(live) = live
            {
                live.update_convo(|c| c.needs_terminal = true);
            }
        }
        Signal::TurnEnded | Signal::TurnFailed | Signal::SessionEnded => {
            if let Some(live) = live {
                live.reset_turn_state();
            }
            if signal == Signal::TurnEnded {
                if idea
                    && let Some(text) = hook["last_assistant_message"]
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
    record_terminal: bool,
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
    if record_terminal {
        app.store
            .add_entry(
                session,
                EntryKind::User,
                &json!({ "text": text, "state": "delivered", "from": "terminal" }),
            )
            .await?;
    }
    Ok(())
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
    use crate::idea::Card;
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
            claude_projects: crate::config::Config::scratch_projects(),
            claude_bin: "claude".into(),
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
                    preview_command: None,
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
            live.set_idea(true);
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
    async fn an_unknown_session_is_not_found_and_a_shell_has_no_conversation() {
        let f = Fixture::new().await;
        let mut shell = record("sh", false);
        shell.kind = SessionKind::Shell;
        f.app.store.create_session(&shell).await.unwrap();
        let get = |id: &str| {
            Request::get(format!("/api/v1/sessions/{id}/conversation"))
                .body(Body::empty())
                .unwrap()
        };
        assert_eq!(f.call(get("nope")).await.0, StatusCode::NOT_FOUND);
        let (status, body) = f.call(get("sh")).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"], "a shell session has no conversation");
    }

    fn fixture_bytes(name: &str) -> Vec<u8> {
        std::fs::read(format!(
            "{}/../delilah-agents/tests/fixtures/transcripts/{name}.jsonl",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    fn assistant_line(n: usize) -> String {
        format!(
            "{{\"type\":\"assistant\",\"uuid\":\"x{n}\",\"timestamp\":\"2025-02-01T10:00:00.000Z\",\"message\":{{\"id\":\"m{n}\",\"role\":\"assistant\",\"content\":[{{\"type\":\"text\",\"text\":\"t{n}\"}}]}}}}\n"
        )
    }

    impl Fixture {
        /// A plain task session (not an idea one) with a live terminal.
        async fn task_session(&self) -> Live {
            self.app
                .store
                .create_session(&record("task1", false))
                .await
                .unwrap();
            let mut cmd = CommandBuilder::new("sleep");
            cmd.arg("30");
            let pty = delilah_agents::pty::PtySession::spawn(cmd, 80, 24, None).unwrap();
            self.app
                .sessions
                .insert("task1", "@web", SessionKind::Claude, false, pty)
        }

        /// Puts `bytes` where the live session's hooks say its transcript is.
        fn transcript(&self, live: &Live, bytes: &[u8]) -> PathBuf {
            let root = self.app.config.claude_projects.clone();
            std::fs::create_dir_all(&root).unwrap();
            let path = root.join(format!("delilah-tr-{}.jsonl", uuid::Uuid::new_v4()));
            std::fs::write(&path, bytes).unwrap();
            live.set_transcript(path.clone());
            path
        }

        async fn get_of(&self, uri: &str) -> (StatusCode, Value) {
            self.call(Request::get(uri).body(Body::empty()).unwrap())
                .await
        }
    }

    #[tokio::test]
    async fn a_task_session_merges_the_transcript_with_stored_entries() {
        let f = Fixture::new().await;
        let live = f.task_session().await;
        let bytes = fixture_bytes("basic");
        f.transcript(&live, &bytes);
        f.app
            .store
            .add_entry(
                "task1",
                EntryKind::User,
                &json!({ "text": "mine", "state": "sent", "from": "view" }),
            )
            .await
            .unwrap();
        let (status, body) = f.get_of("/api/v1/sessions/task1/conversation").await;
        assert_eq!(status, StatusCode::OK);
        let entries = body["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 9);
        assert_eq!(entries[0]["id"], "t:u1:0");
        let last = entries.last().unwrap();
        assert_eq!(
            (last["kind"].as_str(), last["text"].as_str()),
            (Some("user"), Some("mine"))
        );
        assert!(last["id"].is_string());
        assert!(entries.iter().all(|e| e["id"].is_string()));
        assert_eq!(body["cursor"], bytes.len().to_string());
        assert_eq!(body["complete"], true);
        assert_eq!(body["transcript"], "present");
        assert_eq!(body["runtime"], "pty");
    }

    #[tokio::test]
    async fn a_cursor_returns_only_later_transcript_entries_and_all_stored_ones() {
        let f = Fixture::new().await;
        let live = f.task_session().await;
        let path = f.transcript(&live, &fixture_bytes("basic"));
        f.app
            .store
            .add_entry(
                "task1",
                EntryKind::User,
                &json!({ "text": "mine", "state": "sent", "from": "view" }),
            )
            .await
            .unwrap();
        let first = f.get_of("/api/v1/sessions/task1/conversation").await.1;
        let cursor = first["cursor"].as_str().unwrap().to_string();
        let mut more = std::fs::read(&path).unwrap();
        more.extend_from_slice(assistant_line(1).as_bytes());
        std::fs::write(&path, more).unwrap();
        let (status, body) = f
            .get_of(&format!(
                "/api/v1/sessions/task1/conversation?cursor={cursor}"
            ))
            .await;
        assert_eq!(status, StatusCode::OK);
        let texts: Vec<_> = body["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["text"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(texts, ["t1", "mine"]);
        let bad = f
            .get_of("/api/v1/sessions/task1/conversation?cursor=abc")
            .await;
        assert_eq!(bad.0, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn more_than_a_page_of_new_entries_is_incomplete() {
        let f = Fixture::new().await;
        let live = f.task_session().await;
        let lines: String = (0..600).map(assistant_line).collect();
        f.transcript(&live, lines.as_bytes());
        let first = f.get_of("/api/v1/sessions/task1/conversation").await.1;
        assert_eq!(first["entries"].as_array().unwrap().len(), 500);
        assert_eq!(first["complete"], false);
        let cursor = first["cursor"].as_str().unwrap();
        let second = f
            .get_of(&format!(
                "/api/v1/sessions/task1/conversation?cursor={cursor}"
            ))
            .await
            .1;
        assert_eq!(second["entries"].as_array().unwrap().len(), 100);
        assert_eq!(second["complete"], true);
    }

    #[tokio::test]
    async fn a_missing_transcript_is_pending_before_the_first_turn_and_missing_after() {
        let f = Fixture::new().await;
        let gone = f
            .app
            .config
            .claude_projects
            .join(format!("delilah-gone-{}.jsonl", uuid::Uuid::new_v4()));
        f.live.set_transcript(gone);
        f.app
            .store
            .add_entry("conv", EntryKind::Claude, &json!({ "text": "hola" }))
            .await
            .unwrap();
        let body = f.conversation().await;
        assert_eq!(body["transcript"], "pending");
        assert_eq!(body["entries"].as_array().unwrap().len(), 1);
        f.live.set_state(LiveState::Working);
        let body = f.conversation().await;
        assert_eq!(body["transcript"], "missing");
        assert_eq!(body["entries"][0]["text"], "hola");
    }

    #[tokio::test]
    async fn a_result_after_the_cursor_updates_the_earlier_tool_entry() {
        let f = Fixture::new().await;
        let live = f.task_session().await;
        let bytes = fixture_bytes("basic");
        f.transcript(&live, &bytes);
        let mut off = 0;
        for _ in 0..3 {
            off += bytes[off..].iter().position(|b| *b == b'\n').unwrap() + 1;
        }
        let body = f
            .get_of(&format!("/api/v1/sessions/task1/conversation?cursor={off}"))
            .await
            .1;
        let bash = body["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["tool_use_id"] == "tb")
            .expect("the tool entry again");
        assert_eq!(bash["result"], "1 failed");
        assert_eq!(bash["id"], "t:a2:0");
    }

    #[tokio::test]
    async fn tool_detail_gives_the_full_call_or_404() {
        let f = Fixture::new().await;
        let live = f.task_session().await;
        f.transcript(&live, &fixture_bytes("large"));
        let (status, body) = f
            .get_of("/api/v1/sessions/task1/conversation/tools/tl")
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["input"]["command"], "cat big");
        assert_eq!(body["result"].as_str().unwrap().len(), 20000);
        assert_eq!(body["is_error"], false);
        for id in ["nope", "a.b"] {
            let uri = format!("/api/v1/sessions/task1/conversation/tools/{id}");
            assert_eq!(f.get_of(&uri).await.0, StatusCode::NOT_FOUND, "{id}");
        }
    }

    #[tokio::test]
    async fn a_transcript_outside_the_projects_folder_or_not_a_file_reads_as_missing() {
        let f = Fixture::new().await;
        let live = f.task_session().await;
        let root = f.app.config.claude_projects.clone();
        std::fs::create_dir_all(&root).unwrap();
        let outside =
            std::env::temp_dir().join(format!("delilah-out-{}.jsonl", uuid::Uuid::new_v4()));
        std::fs::write(&outside, fixture_bytes("basic")).unwrap();
        let link = root.join(format!("delilah-link-{}.jsonl", uuid::Uuid::new_v4()));
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let dir = root.join(format!("delilah-dir-{}.jsonl", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        live.set_state(LiveState::Working);
        for path in [outside, link, dir] {
            live.set_transcript(path);
            let (status, body) = f.get_of("/api/v1/sessions/task1/conversation").await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(body["transcript"], "missing");
            assert!(body["entries"].as_array().unwrap().is_empty());
            let uri = "/api/v1/sessions/task1/conversation/tools/ta";
            assert_eq!(f.get_of(uri).await.0, StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn a_cursor_read_does_not_need_the_bytes_before_it() {
        let f = Fixture::new().await;
        let live = f.task_session().await;
        let prefix = vec![b'\xff'; 4096];
        let mut bytes = prefix.clone();
        bytes.extend_from_slice(b"\n");
        let cursor = bytes.len();
        bytes.extend_from_slice(assistant_line(7).as_bytes());
        f.transcript(&live, &bytes);
        let body = f
            .get_of(&format!(
                "/api/v1/sessions/task1/conversation?cursor={cursor}"
            ))
            .await
            .1;
        assert_eq!(body["entries"][0]["text"], "t7");
        assert_eq!(body["cursor"], bytes.len().to_string());
        assert_eq!(body["complete"], true);
    }

    #[tokio::test]
    async fn an_orphan_result_finds_its_tool_call_among_unparseable_earlier_lines() {
        let f = Fixture::new().await;
        let live = f.task_session().await;
        let mut bytes = b"not json at all\n".to_vec();
        bytes.extend_from_slice(&fixture_bytes("basic"));
        f.transcript(&live, &bytes);
        let mut off = 0;
        for _ in 0..4 {
            off += bytes[off..].iter().position(|b| *b == b'\n').unwrap() + 1;
        }
        let body = f
            .get_of(&format!("/api/v1/sessions/task1/conversation?cursor={off}"))
            .await
            .1;
        let bash = body["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["tool_use_id"] == "tb")
            .expect("the tool entry again");
        assert_eq!(bash["result"], "1 failed");
    }

    #[tokio::test]
    async fn tool_detail_is_capped_at_a_megabyte() {
        let f = Fixture::new().await;
        let live = f.task_session().await;
        let big = "x".repeat(MAX_DETAIL + 10);
        let lines = format!(
            "{{\"type\":\"assistant\",\"uuid\":\"a\",\"message\":{{\"id\":\"m\",\"role\":\"assistant\",\"content\":[{{\"type\":\"tool_use\",\"id\":\"tbig\",\"name\":\"Bash\",\"input\":{{\"command\":\"{big}\"}}}}]}}}}\n\
             {{\"type\":\"user\",\"uuid\":\"u\",\"message\":{{\"role\":\"user\",\"content\":[{{\"type\":\"tool_result\",\"tool_use_id\":\"tbig\",\"content\":\"{big}\"}}]}}}}\n"
        );
        f.transcript(&live, lines.as_bytes());
        let (status, body) = f
            .get_of("/api/v1/sessions/task1/conversation/tools/tbig")
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["result"].as_str().unwrap().len(), MAX_DETAIL);
        assert!(body["input"].as_str().unwrap().len() <= MAX_DETAIL);
        assert_eq!(body["truncated"], true);
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
        let queued = f.send("later").await.1["id"].as_str().unwrap().to_string();
        f.live.set_state(LiveState::Waiting);
        let sent = f.send("now").await.1["id"].as_str().unwrap().to_string();
        let delete = |id: String| {
            Request::delete(format!("/api/v1/sessions/conv/messages/{id}"))
                .body(Body::empty())
                .unwrap()
        };
        assert_eq!(f.call(delete(sent.clone())).await.0, StatusCode::CONFLICT);
        assert_eq!(
            f.call(delete(queued.clone())).await.0,
            StatusCode::NO_CONTENT
        );
        assert_eq!(f.call(delete(queued)).await.0, StatusCode::CONFLICT);
        assert_eq!(f.entries().await.len(), 1);
    }

    #[tokio::test]
    async fn stop_writes_escape_withdraws_pending_cards_and_keeps_the_queue() {
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
        assert_eq!(f.typed(1).await, "\x1b", "only the ESC is typed");
        assert!(f.app.asks.list().is_empty());
        assert_eq!(ask.await.unwrap().1, json!({}), "the hook lets go");
        let entries = f.entries().await;
        let card = entries.iter().find(|e| e["kind"] == "card").unwrap();
        assert_eq!(card["outcome"], "withdrawn");
        let user = entries.iter().find(|e| e["kind"] == "user").unwrap();
        assert_eq!(user["state"], "queued", "kept, not sent");
        assert_eq!(f.conversation().await["activity"], Value::Null);
        assert_eq!(f.live.state(), LiveState::Waiting);
        // It goes out at the next turn's end.
        f.hook(stop("")).await;
        let expected = "\x1blater\r";
        assert_eq!(f.typed(expected.len()).await, expected);
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
        let id = f.send("later").await.1["id"].as_str().unwrap().to_string();
        f.app.store.claim_queued("conv").await.unwrap();
        let req = Request::delete(format!("/api/v1/sessions/conv/messages/{id}"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(f.call(req).await.0, StatusCode::CONFLICT);
        assert_eq!(f.entries().await[0]["state"], "sent");
    }

    #[tokio::test]
    async fn a_task_session_takes_messages_like_an_idea() {
        let f = Fixture::new().await;
        let live = f.task_session().await;
        let msg = |text: &str| json!({ "text": text });
        live.set_state(LiveState::Waiting);
        let (status, sent) = f.post("/api/v1/sessions/task1/messages", msg("hola")).await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(sent["state"], "sent");
        assert!(sent["id"].is_string());

        live.set_state(LiveState::Working);
        let (_, queued) = f
            .post("/api/v1/sessions/task1/messages", msg("luego"))
            .await;
        assert_eq!(queued["state"], "queued");
        let stored = f.app.store.entries("task1").await.unwrap();
        assert_eq!(stored.len(), 2);

        // Removing takes the string id; a transcript id or a number-less one is not found.
        let delete = |id: &str| {
            Request::delete(format!("/api/v1/sessions/task1/messages/{id}"))
                .body(Body::empty())
                .unwrap()
        };
        assert_eq!(f.call(delete("t:u1:0")).await.0, StatusCode::NOT_FOUND);
        assert_eq!(f.call(delete("abc")).await.0, StatusCode::NOT_FOUND);
        let id = queued["id"].as_str().unwrap();
        assert_eq!(f.call(delete(id)).await.0, StatusCode::NO_CONTENT);
        assert_eq!(f.app.store.entries("task1").await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_task_queue_is_typed_at_the_turn_end_and_stop_keeps_it() {
        let f = Fixture::new().await;
        // A task session whose terminal is a raw `cat`, as the idea one.
        let typed = std::env::temp_dir().join(format!("delilah-typed-{}", uuid::Uuid::new_v4()));
        let mut cmd = CommandBuilder::new("bash");
        cmd.args([
            "--noprofile",
            "--norc",
            "-c",
            &format!("stty raw -echo; exec cat > {}", typed.display()),
        ]);
        f.app
            .store
            .create_session(&record("task1", false))
            .await
            .unwrap();
        let pty = delilah_agents::pty::PtySession::spawn(cmd, 80, 24, None).unwrap();
        let live = f
            .app
            .sessions
            .insert("task1", "@web", SessionKind::Claude, false, pty);
        for _ in 0..100 {
            if typed.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        live.set_state(LiveState::Working);
        f.post(
            "/api/v1/sessions/task1/messages",
            json!({ "text": "luego" }),
        )
        .await;
        let (status, _) = f.post("/api/v1/sessions/task1/stop", json!({})).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let queued = f.app.store.queued_entries("task1").await.unwrap();
        assert_eq!(queued.len(), 1, "stop keeps the queue");

        let url = "/hooks/claude?task=@web&session=task1";
        let (status, _) = f.post(url, stop("listo")).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let expected = "\x1bluego\r";
        for _ in 0..100 {
            if std::fs::read(&typed).unwrap_or_default().len() >= expected.len() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(std::fs::read_to_string(&typed).unwrap(), expected);
        assert!(
            f.app
                .store
                .queued_entries("task1")
                .await
                .unwrap()
                .is_empty()
        );
        // A task's own Claude messages live in its transcript, not in the store.
        let kinds: Vec<_> = f
            .app
            .store
            .entries("task1")
            .await
            .unwrap()
            .into_iter()
            .map(|e| e.kind)
            .collect();
        assert_eq!(kinds, [EntryKind::User]);
    }

    #[tokio::test]
    async fn a_shell_and_a_closed_session_refuse_messages() {
        let f = Fixture::new().await;
        let mut shell = record("sh", false);
        shell.kind = SessionKind::Shell;
        f.app.store.create_session(&shell).await.unwrap();
        let mut closed = record("old", false);
        closed.closed = true;
        f.app.store.create_session(&closed).await.unwrap();
        for id in ["sh", "old"] {
            let (status, _) = f
                .post(
                    &format!("/api/v1/sessions/{id}/messages"),
                    json!({ "text": "x" }),
                )
                .await;
            assert_eq!(status, StatusCode::CONFLICT, "{id}");
            let (status, _) = f
                .post(&format!("/api/v1/sessions/{id}/stop"), json!({}))
                .await;
            assert_eq!(status, StatusCode::CONFLICT, "{id}");
        }
        let (_, body) = f
            .post("/api/v1/sessions/sh/messages", json!({ "text": "x" }))
            .await;
        assert_eq!(body["error"], "a shell session has no conversation");
    }

    #[tokio::test]
    async fn a_task_session_needs_the_terminal_on_a_permission_prompt_or_a_dialog() {
        let f = Fixture::new().await;
        let live = f.task_session().await;
        let url = "/hooks/claude?task=@web&session=task1";
        let needs = || live.needs_terminal();
        for kind in ["permission_prompt", "elicitation_dialog"] {
            let hook = json!({ "hook_event_name": "Notification", "notification_type": kind });
            assert_eq!(f.post(url, hook).await.0, StatusCode::NO_CONTENT);
            assert!(needs(), "{kind}");
            let post_tool = json!({ "hook_event_name": "PostToolUse", "tool_name": "Bash",
                "tool_input": { "command": "ls" } });
            f.post(url, post_tool).await;
            assert!(!needs());
        }
        let hook =
            json!({ "hook_event_name": "Notification", "notification_type": "permission_prompt" });
        f.post(url, hook).await;
        f.post(
            url,
            json!({ "hook_event_name": "UserPromptSubmit", "prompt": "sigue" }),
        )
        .await;
        assert!(!needs());
        assert_eq!(
            f.get_of("/api/v1/sessions/task1/conversation").await.1["needs_terminal"],
            false
        );
    }

    fn ask_transcript(tool_use_id: &str) -> String {
        format!(
            "{{\"type\":\"assistant\",\"uuid\":\"a1\",\"timestamp\":\"2025-02-01T10:00:00.000Z\",\"message\":{{\"id\":\"m1\",\"role\":\"assistant\",\"content\":[{{\"type\":\"tool_use\",\"id\":\"{tool_use_id}\",\"name\":\"AskUserQuestion\",\"input\":{{\"questions\":[]}}}}]}}}}\n"
        ) + &assistant_line(2)
    }

    async fn add_card(f: &Fixture, session: &str, ask: &str, tool_use_id: Option<&str>) {
        let mut card = Card::pending(ask, &json!([]), None);
        card.tool_use_id = tool_use_id.map(str::to_string);
        f.app
            .store
            .add_entry(
                session,
                EntryKind::Card,
                &serde_json::to_value(card).unwrap(),
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn a_card_replaces_its_ask_tool_entry_and_an_unmatched_one_stays() {
        let f = Fixture::new().await;
        let live = f.task_session().await;
        f.transcript(&live, ask_transcript("toolu_1").as_bytes());
        add_card(&f, "task1", "ask-1", Some("toolu_1")).await;
        add_card(&f, "task1", "ask-old", None).await;
        let body = f.get_of("/api/v1/sessions/task1/conversation").await.1;
        let entries = body["entries"].as_array().unwrap();
        assert!(entries.iter().all(|e| e["kind"] != "tool"), "tool dropped");
        let pos = |ask: &str| entries.iter().position(|e| e["ask"] == ask).unwrap();
        // The matched card sits where the tool was: first, before the text after it.
        assert_eq!(pos("ask-1"), 0);
        assert_eq!(entries[0]["at"], "2025-02-01T10:00:00.000Z");
        assert_eq!(entries[0]["tool_use_id"], "toolu_1");
        assert_eq!(entries.iter().filter(|e| e["ask"] == "ask-1").count(), 1);
        // The unmatched one keeps its own time (now, so after the transcript's text).
        assert_eq!(pos("ask-old"), 2);
    }

    #[tokio::test]
    async fn an_idea_with_a_transcript_hides_stored_messages_and_without_one_keeps_them() {
        let f = Fixture::new().await;
        for (kind, body) in [
            (
                EntryKind::User,
                json!({ "text": "idea", "state": "delivered", "from": "view" }),
            ),
            (
                EntryKind::User,
                json!({ "text": "sent", "state": "sent", "from": "view" }),
            ),
            (
                EntryKind::User,
                json!({ "text": "later", "state": "queued", "from": "view" }),
            ),
            (EntryKind::Claude, json!({ "text": "hola" })),
        ] {
            f.app.store.add_entry("conv", kind, &body).await.unwrap();
        }
        add_card(&f, "conv", "a", None).await;
        let without = f.entries().await;
        assert_eq!(without.len(), 5);

        f.transcript(&f.live, &fixture_bytes("basic"));
        let with = f.entries().await;
        let stored: Vec<_> = with
            .iter()
            .filter(|e| !e["id"].as_str().unwrap().starts_with("t:"))
            .map(|e| e["kind"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(stored.len(), 2, "{stored:?}");
        assert!(stored.contains(&"card".to_string()));
        assert!(
            with.iter()
                .any(|e| e["text"] == "later" && e["state"] == "queued")
        );
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
                preview_command: None,
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
