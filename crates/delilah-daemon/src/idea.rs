//! What only an idea conversation has: the cards Claude's asks put in its history
//! (Nueva tarea, proposals) and the check that a session is an idea.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::events::Topic;
use crate::proposals::Proposal;
use crate::state::AppState;
use crate::store_sessions::{EntryKind, SessionKind};

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
    /// The AskUserQuestion call's id, which ties the card to the transcript's tool entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    /// When that call was made, found once in the transcript and kept so later reads
    /// do not scan for it again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_at: Option<String>,
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
            tool_use_id: None,
            tool_at: None,
        }
    }
}

/// Whether the session is an idea conversation: its live state knows; a session that
/// is not running is read from its record.
pub async fn is_idea(app: &AppState, session: &str) -> bool {
    match app.sessions.get(session) {
        Some(live) => live.is_idea(),
        None => matches!(app.store.session(session).await, Ok(Some(r)) if r.conversation),
    }
}

/// A card of a conversation's session enters the history as pending.
pub async fn card_added(app: &AppState, session: &str, card: &Card) {
    // Every Claude session shows its cards inline, not only ideas.
    let claude = match app.sessions.get(session) {
        Some(live) => live.kind != SessionKind::Shell,
        None => {
            matches!(app.store.session(session).await, Ok(Some(r)) if r.kind != SessionKind::Shell)
        }
    };
    if !claude {
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
