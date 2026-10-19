//! The inbox: everything that waits on the user, across tasks. A session's hooks say
//! when it needs you (a question at the end of a turn, a permission, an error); the
//! task's state adds the rest (paused by the usage limit, ready for review, failed).
//! An item goes away by itself once it is dealt with.

use std::collections::HashSet;

use axum::Json;
use axum::extract::{Path, State};
use chrono::{DateTime, Utc};
use delilah_agents::claude::Signal;
use delilah_core::{SpecPhase, Task, TaskStatus};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::actions::type_prompt;
use crate::api::ApiError;
use crate::state::AppState;

/// Long answers are cut: the inbox shows the gist, the session tab the rest.
const MAX_TEXT: usize = 4000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AttentionKind {
    /// The turn ended: the agent asks or reports something and waits.
    Question,
    /// The agent needs a permission to go on (answered in its terminal).
    Permission,
    /// The turn failed for a reason other than the usage limit.
    Error,
}

/// What a session waits on the user for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attention {
    pub kind: AttentionKind,
    pub text: String,
    pub since: DateTime<Utc>,
}

/// How a hook changes what a session waits for.
#[derive(Debug, PartialEq, Eq)]
pub enum Change {
    Keep,
    Clear,
    Set(Attention),
}

/// `limited`: the turn ended on the usage limit (the task pauses; nothing to answer).
pub fn attention_change(
    signal: Signal,
    hook: &Value,
    limited: bool,
    current: Option<&Attention>,
    now: DateTime<Utc>,
) -> Change {
    let text = |field: &str| hook[field].as_str().map(clip).filter(|t| !t.is_empty());
    let set = |kind, text: String| {
        Change::Set(Attention {
            kind,
            text,
            since: now,
        })
    };
    match signal {
        Signal::PromptSubmitted | Signal::Working | Signal::SessionEnded => Change::Clear,
        _ if limited => Change::Clear,
        Signal::TurnEnded => set(
            AttentionKind::Question,
            text("last_assistant_message").unwrap_or_else(|| "El agente terminó su turno.".into()),
        ),
        Signal::TurnFailed => set(
            AttentionKind::Error,
            text("error")
                .or_else(|| text("message"))
                .unwrap_or_else(|| "El turno falló.".into()),
        ),
        Signal::NeedsUser => match hook["notification_type"].as_str() {
            Some("permission_prompt") => set(
                AttentionKind::Permission,
                text("message").unwrap_or_else(|| "Claude necesita un permiso.".into()),
            ),
            // "Waiting for your input" after a turn: what it said at the end is the news.
            _ if current.is_some() => Change::Keep,
            _ => set(
                AttentionKind::Question,
                text("message").unwrap_or_else(|| "El agente te espera.".into()),
            ),
        },
        _ => Change::Keep,
    }
}

fn clip(s: &str) -> String {
    let s = s.trim();
    if s.len() <= MAX_TEXT {
        return s.to_string();
    }
    let mut end = MAX_TEXT;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

/// In order of urgency: what blocks an agent first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ItemKind {
    Permission,
    Question,
    /// A session's turn failed.
    Error,
    /// The task waits on you, but no session said what for (e.g. after a restart).
    Waiting,
    Review,
    Paused,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemSession {
    pub id: String,
    pub label: String,
    pub primary: bool,
    /// A reply can only be typed into a running session.
    pub live: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct InboxItem {
    pub kind: ItemKind,
    pub task: String,
    pub title: String,
    pub session: Option<ItemSession>,
    pub text: Option<String>,
    pub since: Option<DateTime<Utc>>,
    /// A question with options (AskUserQuestion) that can be answered from here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask: Option<AskView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AskView {
    pub id: String,
    pub questions: Value,
}

/// `GET /api/v1/inbox`: most urgent first (what blocks an agent), newest first within.
pub async fn list(State(app): State<AppState>) -> Result<Json<Vec<InboxItem>>, ApiError> {
    let tasks = app.store.list_tasks().await?;
    let task = |key: &str| tasks.iter().find(|t| t.key.as_str() == key);
    let mut items = Vec::new();
    let mut asked: HashSet<String> = HashSet::new();
    let mut asking: HashSet<String> = HashSet::new();

    for ask in app.asks.list() {
        let Some(t) = task(&ask.task) else { continue };
        let record = match &ask.session {
            Some(id) => app.store.session(id).await.ok().flatten(),
            None => None,
        };
        let first = ask.questions[0]["question"].as_str().map(str::to_string);
        asked.insert(ask.task.clone());
        if let Some(id) = &ask.session {
            asking.insert(id.clone());
        }
        items.push(InboxItem {
            kind: ItemKind::Question,
            task: ask.task.clone(),
            title: t.title.clone(),
            session: record.map(|r| ItemSession {
                live: app.sessions.get(&r.id).is_some(),
                id: r.id,
                label: r.label,
                primary: r.primary,
            }),
            text: first,
            since: Some(ask.since),
            ask: Some(AskView {
                id: ask.id,
                questions: ask.questions,
            }),
        });
    }

    for (s, attention) in app.store.attentions().await? {
        let Some(t) = task(&s.task).filter(|t| !t.status.is_terminal()) else {
            continue;
        };
        if asking.contains(&s.id) {
            continue;
        }
        let live = app.sessions.get(&s.id).is_some();
        // A permission prompt dies with its process.
        if attention.kind == AttentionKind::Permission && !live {
            continue;
        }
        asked.insert(s.task.clone());
        items.push(InboxItem {
            kind: match attention.kind {
                AttentionKind::Question => ItemKind::Question,
                AttentionKind::Permission => ItemKind::Permission,
                AttentionKind::Error => ItemKind::Error,
            },
            task: s.task.clone(),
            title: t.title.clone(),
            session: Some(ItemSession {
                id: s.id,
                label: s.label,
                primary: s.primary,
                live,
            }),
            text: Some(attention.text),
            since: Some(attention.since),
            ask: None,
        });
    }

    let specs = app.specs.lock().unwrap().clone();
    for t in &tasks {
        let key = t.key.as_str();
        let in_review = specs
            .get(key)
            .filter(|s| s.dir.is_some() && s.phase == SpecPhase::Review && is_open(t));
        let found = match &t.status {
            TaskStatus::WaitingForUser if !asked.contains(key) => {
                Some((ItemKind::Waiting, None, "agent-asked"))
            }
            TaskStatus::PausedByLimit { .. } => Some((ItemKind::Paused, None, "limit-reached")),
            TaskStatus::Failed { reason } => Some((ItemKind::Failed, Some(reason.clone()), "fail")),
            _ => None,
        };
        let review = in_review.map(|s| {
            let progress = format!("{}/{} subtareas hechas", s.done, s.total);
            (ItemKind::Review, Some(progress), "phase-reached")
        });
        for (kind, text, event) in found.into_iter().chain(review) {
            items.push(InboxItem {
                kind,
                task: key.to_string(),
                title: t.title.clone(),
                session: None,
                text,
                // When the event that raised it happened.
                since: last_event(&app, t, event).await,
                ask: None,
            });
        }
    }

    items.sort_by(|a, b| a.kind.cmp(&b.kind).then(b.since.cmp(&a.since)));
    Ok(Json(items))
}

async fn last_event(app: &AppState, t: &Task, name: &str) -> Option<DateTime<Utc>> {
    let events = app.store.events(&t.key).await.ok()?;
    events
        .iter()
        .rev()
        .find(|e| e.event.name() == name)
        .map(|e| e.at)
}

fn is_open(t: &Task) -> bool {
    matches!(t.status, TaskStatus::Running | TaskStatus::WaitingForUser)
}

#[derive(Debug, Deserialize)]
pub struct Reply {
    pub text: String,
}

/// `POST /api/v1/sessions/{id}/reply`: types an answer into a waiting session, as if
/// written in its terminal.
pub async fn reply(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(reply): Json<Reply>,
) -> Result<axum::http::StatusCode, ApiError> {
    let text = reply.text.trim();
    if text.is_empty() {
        return Err(ApiError::bad_request("the reply is empty"));
    }
    let live = app
        .sessions
        .get(&id)
        .ok_or_else(|| ApiError::conflict("the session is not running; reopen it to answer"))?;
    // Newlines would send it early: the TUI takes Enter as "send".
    let text = text.replace(['\r', '\n'], " ");
    type_prompt(&live, &text).await?;
    app.store.set_attention(&id, None).await?;
    app.emit(crate::events::Topic::Inbox);
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn now() -> DateTime<Utc> {
        "2026-10-19T20:00:00Z".parse().unwrap()
    }

    #[test]
    fn a_finished_turn_asks_with_its_last_message() {
        let hook =
            json!({ "hook_event_name": "Stop", "last_assistant_message": "¿Uso cursor o offset?" });
        let Change::Set(a) = attention_change(Signal::TurnEnded, &hook, false, None, now()) else {
            panic!("expected an attention");
        };
        assert_eq!(a.kind, AttentionKind::Question);
        assert_eq!(a.text, "¿Uso cursor o offset?");
    }

    #[test]
    fn the_idle_reminder_keeps_the_question_and_a_permission_replaces_it() {
        let question = Attention {
            kind: AttentionKind::Question,
            text: "¿Cursor?".into(),
            since: now(),
        };
        let idle = json!({ "notification_type": "idle_prompt", "message": "Claude is waiting for your input" });
        assert_eq!(
            attention_change(Signal::NeedsUser, &idle, false, Some(&question), now()),
            Change::Keep
        );
        let perm = json!({ "notification_type": "permission_prompt", "message": "Claude needs your permission to use Bash" });
        let Change::Set(a) =
            attention_change(Signal::NeedsUser, &perm, false, Some(&question), now())
        else {
            panic!("expected an attention");
        };
        assert_eq!(a.kind, AttentionKind::Permission);
    }

    #[test]
    fn answering_or_hitting_the_limit_clears_it() {
        let hook = json!({});
        assert_eq!(
            attention_change(Signal::PromptSubmitted, &hook, false, None, now()),
            Change::Clear
        );
        assert_eq!(
            attention_change(Signal::TurnEnded, &hook, true, None, now()),
            Change::Clear
        );
    }

    #[test]
    fn long_messages_are_cut_on_a_char_boundary() {
        let long = "ñ".repeat(MAX_TEXT);
        let cut = clip(&long);
        assert!(cut.ends_with('…'));
        assert!(cut.len() <= MAX_TEXT + '…'.len_utf8());
    }
}
