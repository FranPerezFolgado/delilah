//! The inbox: everything that waits on the user, across tasks. A session's hooks say
//! when it needs you (a question at the end of a turn, a permission, an error); the
//! task's state adds the rest (paused by the usage limit, ready for review, failed).
//! An item goes away by itself once it is dealt with.

use std::collections::HashSet;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Path, State};
use chrono::{DateTime, Utc};
use delilah_agents::claude::Signal;
use delilah_core::{AutopilotState, HandoffStage, SpecPhase, Step, Task, TaskStatus};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::actions::type_prompt;
use crate::api::ApiError;
use crate::archive::{KIND_BLOCKED, KIND_PARTIAL};
use crate::state::AppState;
use crate::store_prs::{KIND_BASE_STALE, KIND_PR_OPENED, KIND_TURN_DONE, NoticeRow};
use crate::store_sessions::SessionKind;

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
    lang: crate::lang::Lang,
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
            text("last_assistant_message").unwrap_or_else(|| {
                lang.pick(
                    "El agente terminó su turno.",
                    "The agent finished its turn.",
                )
                .into()
            }),
        ),
        Signal::TurnFailed => set(
            AttentionKind::Error,
            text("error")
                .or_else(|| text("message"))
                .unwrap_or_else(|| lang.pick("El turno falló.", "The turn failed.").into()),
        ),
        Signal::NeedsUser => match hook["notification_type"].as_str() {
            Some("permission_prompt") => set(
                AttentionKind::Permission,
                text("message").unwrap_or_else(|| {
                    lang.pick("Claude necesita un permiso.", "Claude needs a permission.")
                        .into()
                }),
            ),
            // "Waiting for your input" after a turn: what it said at the end is the news.
            _ if current.is_some() => Change::Keep,
            _ => set(
                AttentionKind::Question,
                text("message").unwrap_or_else(|| {
                    lang.pick("El agente te espera.", "The agent is waiting for you.")
                        .into()
                }),
            ),
        },
        _ => Change::Keep,
    }
}

pub(crate) fn clip(s: &str) -> String {
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
    /// The CI of a task's PR failed.
    PrFailed,
    /// A reviewer asked for changes or commented on a task's PR.
    PrReview,
    /// The branch derived from the spec could not be applied to a repo; retry from here.
    /// Right after the review items: an actionable problem that blocks no agent.
    Branch,
    /// The task waits on you, but no session said what for (e.g. after a restart).
    Waiting,
    Review,
    /// A saving-mode step is over: the next session is offered, being prepared, or failed.
    Handoff,
    /// Autopilot paused: the reason, and the app offers to resume or go by hand.
    Autopilot,
    /// The agent stopped with commits on its branch: maybe it is ready for review.
    SuggestReview,
    /// Archiving the task left a PR behind (blocked or partial).
    PrBlocked,
    /// A task's branch started from a base that could not be refreshed.
    BaseStale,
    Paused,
    Failed,
    /// A non-primary session finished a turn. A notice; removable.
    TurnDone,
    /// A PR of the task was opened or linked. A notice; removable.
    PrOpened,
}

impl ItemKind {
    /// The kind as the API names it (`kind` in the JSON).
    pub fn name(self) -> &'static str {
        match self {
            Self::Permission => "permission",
            Self::Question => "question",
            Self::Error => "error",
            Self::PrFailed => "pr-failed",
            Self::PrReview => "pr-review",
            Self::Branch => "branch",
            Self::Waiting => "waiting",
            Self::Review => "review",
            Self::Handoff => "handoff",
            Self::Autopilot => "autopilot",
            Self::SuggestReview => "suggest-review",
            Self::PrBlocked => "pr-blocked",
            Self::BaseStale => "base-stale",
            Self::Paused => "paused",
            Self::Failed => "failed",
            Self::TurnDone => "turn-done",
            Self::PrOpened => "pr-opened",
        }
    }

    /// Blocking items wait on the user for the agent to go on: they are never removed
    /// by hand. Mirrors `BLOCKING` in `app/src/api.ts` (shared fixture
    /// `tests/fixtures/blocking_kinds.json`).
    pub fn is_blocking(self) -> bool {
        matches!(
            self,
            Self::Permission
                | Self::Question
                | Self::Error
                | Self::Waiting
                | Self::PrFailed
                | Self::PrReview
                | Self::PrBlocked
        )
    }
}

/// The item kind a stored notice is shown as; `None` for kinds the inbox does not list.
pub(crate) fn notice_kind(kind: &str) -> Option<ItemKind> {
    Some(match kind {
        "ci-failed" => ItemKind::PrFailed,
        "changes-requested" | "comments" => ItemKind::PrReview,
        KIND_BLOCKED | KIND_PARTIAL => ItemKind::PrBlocked,
        KIND_BASE_STALE => ItemKind::BaseStale,
        KIND_TURN_DONE => ItemKind::TurnDone,
        KIND_PR_OPENED => ItemKind::PrOpened,
        _ => return None,
    })
}

/// The id of the item a notice stands for, as `GET /inbox` lists it.
pub(crate) fn notice_item_id(n: &NoticeRow) -> Option<String> {
    let kind = notice_kind(&n.kind)?;
    Some(format!(
        "{}:{}:{}:{}",
        kind.name(),
        n.task_key,
        n.id,
        n.created_at.timestamp_millis()
    ))
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
    /// `kind:task:source:since_ms`: the same for as long as the cause lasts, new for a
    /// new occurrence of it. What dismissals refer to.
    pub id: String,
    pub kind: ItemKind,
    pub task: String,
    pub title: String,
    pub session: Option<ItemSession>,
    pub text: Option<String>,
    pub since: Option<DateTime<Utc>>,
    /// A question with options (AskUserQuestion) that can be answered from here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask: Option<AskView>,
    /// The PR notice this item stands for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr: Option<InboxPr>,
    /// A permission a chat-mode agent asks for, answered from here
    /// (`POST /api/v1/sessions/{id}/permissions/{ask}`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission: Option<PermissionView>,
    /// The notice this item stands for, when it has no PR (e.g. a stale base).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PermissionView {
    /// The ask's id: what the answer names.
    pub id: String,
    pub tool: String,
    /// One line of the input: the command, the file, the URL.
    pub summary: String,
    pub suggestion: Option<Value>,
    /// The rule "allow always" would remember; null: that button is not offered.
    pub rule: Option<String>,
}

impl InboxItem {
    /// An item with only what every item has; the rest is set with `..`.
    fn new(kind: ItemKind, task: String, title: String) -> Self {
        Self {
            id: String::new(),
            kind,
            task,
            title,
            session: None,
            text: None,
            since: None,
            ask: None,
            pr: None,
            permission: None,
            notice: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct InboxPr {
    pub notice: i64,
    pub url: String,
    pub repo: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AskView {
    pub id: String,
    pub questions: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposal: Option<crate::proposals::Proposal>,
}

/// How long a dismissal is kept (so «Deshacer» and a later restore can find it).
const DISMISSAL_KEEP_DAYS: i64 = 30;
const PURGE_EVERY: Duration = Duration::from_secs(3600);
/// Most ids one restore may name.
const MAX_RESTORE: usize = 500;

/// When the dismissals were last purged by `list` (none: not yet in this process).
static LAST_PURGE: Mutex<Option<Instant>> = Mutex::new(None);

/// Purges the old dismissals if `last` says it has been an hour (or never); whether it
/// ran. The first `list` of a process is the purge «at startup».
async fn purge_if_due(app: &AppState, last: &Mutex<Option<Instant>>, now: Instant) -> bool {
    {
        let mut last = last.lock().unwrap();
        if last.is_some_and(|t| now.duration_since(t) < PURGE_EVERY) {
            return false;
        }
        *last = Some(now);
    }
    let cutoff = Utc::now() - chrono::Duration::days(DISMISSAL_KEEP_DAYS);
    if let Err(e) = app.store.purge_dismissals(cutoff).await {
        tracing::warn!("cannot purge the old inbox dismissals: {e}");
    }
    true
}

/// The item's id: kind, task, what raised it and when. An item without a time (a branch
/// error, a handoff) takes a hash of its text, so a changed cause is a new id.
fn item_id(i: &InboxItem) -> String {
    let source = if let Some(a) = &i.ask {
        a.id.clone()
    } else if let Some(p) = &i.permission {
        p.id.clone()
    } else if let Some(pr) = &i.pr {
        pr.notice.to_string()
    } else if let Some(n) = i.notice {
        n.to_string()
    } else if let Some(s) = &i.session {
        s.id.clone()
    } else {
        "task".to_string()
    };
    let at = match i.since {
        Some(t) => t.timestamp_millis(),
        None => {
            // FNV-1a: stable across runs, unlike the std hashers.
            let hash = i.text.as_deref().unwrap_or("").bytes().fold(
                0xcbf2_9ce4_8422_2325_u64,
                |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3),
            );
            (hash % 1_000_000_000_000) as i64
        }
    };
    format!("{}:{}:{}:{}", i.kind.name(), i.task, source, at)
}

/// `POST /inbox/{id}/dismiss`: removes one item. 404 if it is not listed, 409 if it
/// blocks an agent.
pub async fn dismiss(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::http::StatusCode, ApiError> {
    let items = collect(&app).await?;
    let item = items
        .iter()
        .find(|i| i.id == id)
        .ok_or_else(|| ApiError::not_found(format!("there is no inbox item {id}")))?;
    if item.kind.is_blocking() {
        return Err(ApiError::conflict(
            "this item waits on you; answer it instead of removing it",
        ));
    }
    app.store.dismiss(&[(id, item.task.clone())]).await?;
    app.emit(crate::events::Topic::Inbox);
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// `POST /inbox/clear`: dismisses every non-blocking item shown, answers their ids.
pub async fn clear(State(app): State<AppState>) -> Result<Json<Value>, ApiError> {
    let dismissed = app.store.dismissed().await?;
    let items: Vec<_> = collect(&app)
        .await?
        .into_iter()
        .filter(|i| !i.kind.is_blocking() && !dismissed.contains(&i.id))
        .collect();
    let pairs: Vec<_> = items
        .iter()
        .map(|i| (i.id.clone(), i.task.clone()))
        .collect();
    app.store.dismiss(&pairs).await?;
    app.emit(crate::events::Topic::Inbox);
    let ids: Vec<_> = items.into_iter().map(|i| i.id).collect();
    Ok(Json(serde_json::json!({ "ids": ids })))
}

#[derive(Debug, Deserialize)]
pub struct Restore {
    pub ids: Vec<String>,
}

/// `POST /inbox/restore`: undoes dismissals by id; unknown ids are ignored.
pub async fn restore(
    State(app): State<AppState>,
    Json(body): Json<Restore>,
) -> Result<axum::http::StatusCode, ApiError> {
    if body.ids.len() > MAX_RESTORE {
        return Err(ApiError::bad_request(format!(
            "at most {MAX_RESTORE} ids at once"
        )));
    }
    app.store.undismiss(&body.ids).await?;
    app.emit(crate::events::Topic::Inbox);
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// `GET /api/v1/inbox`: most urgent first (what blocks an agent), newest first within.
/// Dismissed items are left out.
pub async fn list(State(app): State<AppState>) -> Result<Json<Vec<InboxItem>>, ApiError> {
    purge_if_due(&app, &LAST_PURGE, Instant::now()).await;
    let dismissed = app.store.dismissed().await?;
    let mut items = collect(&app).await?;
    items.retain(|i| !dismissed.contains(&i.id));
    Ok(Json(items))
}

/// Every item, dismissed or not, with its id.
async fn collect(app: &AppState) -> Result<Vec<InboxItem>, ApiError> {
    let tasks = app.store.list_tasks().await?;
    let task = |key: &str| tasks.iter().find(|t| t.key.as_str() == key);
    let mut items = Vec::new();
    let mut asked: HashSet<String> = HashSet::new();
    let mut asking: HashSet<String> = HashSet::new();

    for ask in app.asks.list() {
        let Some(title) = owner_title(&ask.task, task(&ask.task)) else {
            continue;
        };
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
                proposal: ask.proposal,
            }),
            ..InboxItem::new(ItemKind::Question, ask.task.clone(), title.clone())
        });
    }

    for ask in app.permissions.list() {
        let Some(title) = owner_title(&ask.task, task(&ask.task)) else {
            continue;
        };
        let Some(record) = app.store.session(&ask.session).await.ok().flatten() else {
            continue;
        };
        asked.insert(ask.task.clone());
        let text = if ask.summary.is_empty() {
            ask.tool.clone()
        } else {
            format!("{}: {}", ask.tool, ask.summary)
        };
        items.push(InboxItem {
            session: Some(ItemSession {
                live: app.sessions.get(&record.id).is_some(),
                id: record.id,
                label: record.label,
                primary: record.primary,
            }),
            text: Some(text),
            since: Some(ask.at),
            permission: Some(PermissionView {
                id: ask.id,
                tool: ask.tool,
                summary: ask.summary,
                suggestion: ask.suggestion,
                rule: ask.rule,
            }),
            ..InboxItem::new(ItemKind::Permission, ask.task.clone(), title)
        });
    }

    for (s, attention) in app.store.attentions().await? {
        let Some(title) = owner_title(&s.task, task(&s.task).filter(|t| !t.status.is_terminal()))
        else {
            continue;
        };
        if asking.contains(&s.id) {
            continue;
        }
        // A permission the registry holds is listed above, one item per ask.
        if attention.kind == AttentionKind::Permission && app.permissions.has_session(&s.id) {
            continue;
        }
        let live = app.sessions.get(&s.id).is_some();
        // A permission prompt dies with its process.
        if attention.kind == AttentionKind::Permission && !live {
            continue;
        }
        asked.insert(s.task.clone());
        items.push(InboxItem {
            session: Some(ItemSession {
                id: s.id,
                label: s.label,
                primary: s.primary,
                live,
            }),
            text: Some(attention.text),
            since: Some(attention.since),
            ..InboxItem::new(
                match attention.kind {
                    AttentionKind::Question => ItemKind::Question,
                    AttentionKind::Permission => ItemKind::Permission,
                    AttentionKind::Error => ItemKind::Error,
                },
                s.task.clone(),
                title.clone(),
            )
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
            TaskStatus::SetupFailed { reason } => {
                Some((ItemKind::Failed, Some(reason.clone()), "setup-failed"))
            }
            _ => None,
        };
        let review = in_review.map(|s| {
            let progress = format!(
                "{}/{} {}",
                s.done,
                s.total,
                app.lang().pick("subtareas hechas", "subtasks done")
            );
            (ItemKind::Review, Some(progress), "phase-reached")
        });
        if let Some(err) = t.branch_error.as_ref().filter(|_| !t.status.is_terminal()) {
            items.push(InboxItem {
                text: Some(err.clone()),
                ..InboxItem::new(ItemKind::Branch, key.to_string(), t.title.clone())
            });
        }
        for (kind, text, event) in found.into_iter().chain(review) {
            items.push(InboxItem {
                text,
                // When the event that raised it happened.
                since: last_event(&app, t, event).await,
                ..InboxItem::new(kind, key.to_string(), t.title.clone())
            });
        }
    }

    for t in tasks.iter().filter(|t| !t.status.is_terminal()) {
        let Some(h) = t.handoff.as_ref() else {
            continue;
        };
        let lang = app.lang();
        let text = match h.stage {
            HandoffStage::Offered => lang
                .pick(
                    "Las tareas están listas. ¿Implementar en una sesión nueva?",
                    "The tasks are ready. Implement in a new session?",
                )
                .to_string(),
            HandoffStage::Saving => lang
                .pick(
                    "Guardando las notas de la sesión…",
                    "Saving the session's notes…",
                )
                .to_string(),
            HandoffStage::Failed => h.error.clone().unwrap_or_else(|| {
                lang.pick(
                    "No se pudo empezar la sesión nueva.",
                    "The new session could not start.",
                )
                .to_string()
            }),
            HandoffStage::Starting => match h.to {
                Step::Review => lang
                    .pick(
                        "Pasando a la revisión en una sesión nueva…",
                        "Moving on to the review in a new session…",
                    )
                    .to_string(),
                _ => lang
                    .pick("Empezando la sesión nueva…", "Starting the new session…")
                    .to_string(),
            },
            HandoffStage::Skipped => continue,
        };
        let session = app
            .store
            .open_sessions(Some(t.key.as_str()))
            .await?
            .into_iter()
            .find(|s| s.primary && s.kind == SessionKind::Claude)
            .map(|s| ItemSession {
                live: app.sessions.get(&s.id).is_some(),
                id: s.id,
                label: s.label,
                primary: true,
            });
        items.push(InboxItem {
            session,
            text: Some(text),
            ..InboxItem::new(ItemKind::Handoff, t.key.to_string(), t.title.clone())
        });
    }

    for t in tasks.iter().filter(|t| !t.status.is_terminal()) {
        let Some(a) = t
            .autopilot
            .as_ref()
            .filter(|a| a.state == AutopilotState::Paused)
        else {
            continue;
        };
        items.push(InboxItem {
            text: Some(a.reason.clone().unwrap_or_else(|| {
                app.lang()
                    .pick("El autopiloto está en pausa.", "Autopilot is paused.")
                    .to_string()
            })),
            since: Some(a.since),
            ..InboxItem::new(ItemKind::Autopilot, t.key.to_string(), t.title.clone())
        });
    }

    let hints = app.review_hints.lock().unwrap().clone();
    for t in tasks
        .iter()
        .filter(|t| hints.contains(t.key.as_str()) && is_open(t))
    {
        if t.phase.as_deref().and_then(SpecPhase::parse) >= Some(SpecPhase::Review) {
            continue;
        }
        items.push(InboxItem {
            text: Some(
                app.lang()
                    .pick(
                        "El agente ha parado con commits en su rama.",
                        "The agent stopped with commits on its branch.",
                    )
                    .to_string(),
            ),
            ..InboxItem::new(ItemKind::SuggestReview, t.key.to_string(), t.title.clone())
        });
    }

    let mut notices = app.store.open_notices().await?;
    notices.extend(app.store.open_avisos().await?);
    for n in notices {
        let Some(kind) = notice_kind(&n.kind) else {
            continue;
        };
        let Some(t) = task(&n.task_key) else {
            continue;
        };
        // The archive notices are about the task as a whole, not one repo's PR. A
        // partial archive is the one that outlives the workspace: it asks to finish.
        let archive = matches!(n.kind.as_str(), KIND_BLOCKED | KIND_PARTIAL);
        // An archived task (finished, its workspace gone) has nothing to act on.
        if n.kind != KIND_PARTIAL
            && t.status.is_terminal()
            && app.store.workspace(&t.key).await?.is_none()
        {
            continue;
        }
        if n.kind == KIND_BASE_STALE {
            items.push(stale_base_item(n, t));
            continue;
        }
        if n.kind == KIND_TURN_DONE {
            if !t.status.is_terminal() {
                items.extend(turn_done_item(app, n, t).await);
            }
            continue;
        }
        // An unlinked PR resolves its notices; one still open without its PR is stale.
        let prs = app.store.prs_of(&t.key).await?;
        let same_repo = prs.iter().find(|p| p.repo == n.repo);
        let Some(pr) = same_repo.or(if archive { prs.first() } else { None }) else {
            continue;
        };
        let text = match kind {
            ItemKind::PrOpened => format!(
                "{} {}",
                app.lang().pick("Se abrió el PR", "PR opened"),
                n.text
            ),
            _ => n.text,
        };
        items.push(InboxItem {
            text: Some(text),
            since: Some(n.created_at),
            pr: Some(InboxPr {
                notice: n.id,
                url: pr.url.clone(),
                repo: n.repo,
            }),
            ..InboxItem::new(kind, n.task_key, t.title.clone())
        });
    }

    for i in &mut items {
        i.id = item_id(i);
    }
    items.sort_by(|a, b| a.kind.cmp(&b.kind).then(b.since.cmp(&a.since)));
    Ok(items)
}

/// The warning that a task's branch started from a base that could not be refreshed.
/// It is about the start, not a PR: it carries the notice to dismiss, not a PR.
fn stale_base_item(n: NoticeRow, t: &Task) -> InboxItem {
    InboxItem {
        text: Some(n.text),
        since: Some(n.created_at),
        notice: Some(n.id),
        ..InboxItem::new(ItemKind::BaseStale, n.task_key, t.title.clone())
    }
}

/// A session finished a turn: the notice names the session (`detail.session`). Gone
/// when the session is closed.
async fn turn_done_item(app: &AppState, n: NoticeRow, t: &Task) -> Option<InboxItem> {
    let id = n.detail.as_ref()?["session"].as_str()?.to_string();
    let record = app.store.session(&id).await.ok().flatten()?;
    if record.closed {
        return None;
    }
    Some(InboxItem {
        session: Some(ItemSession {
            live: app.sessions.get(&record.id).is_some(),
            id: record.id,
            label: record.label,
            primary: record.primary,
        }),
        text: Some(n.text),
        since: Some(n.created_at),
        notice: Some(n.id),
        ..InboxItem::new(ItemKind::TurnDone, n.task_key, t.title.clone())
    })
}

async fn last_event(app: &AppState, t: &Task, name: &str) -> Option<DateTime<Utc>> {
    let events = app.store.events(&t.key).await.ok()?;
    events
        .iter()
        .rev()
        .find(|e| e.event.name() == name)
        .map(|e| e.at)
}

/// What an item is titled by: its task's title, or for a project's session the
/// project itself. None when the task is gone or finished.
fn owner_title(owner: &str, task: Option<&Task>) -> Option<String> {
    match crate::store_sessions::project_of(owner) {
        Some(project) => Some(project.to_string()),
        None => task.map(|t| t.title.clone()),
    }
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
    type_prompt(&live, text).await?;
    app.store.set_attention(&id, None).await?;
    app.emit(crate::events::Topic::Inbox);
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use delilah_core::TaskKey;
    use serde_json::json;

    use super::*;

    fn now() -> DateTime<Utc> {
        "2026-10-19T20:00:00Z".parse().unwrap()
    }

    #[test]
    fn a_finished_turn_asks_with_its_last_message() {
        let hook =
            json!({ "hook_event_name": "Stop", "last_assistant_message": "¿Uso cursor o offset?" });
        let Change::Set(a) = attention_change(
            Signal::TurnEnded,
            &hook,
            false,
            None,
            now(),
            crate::lang::Lang::Es,
        ) else {
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
            attention_change(
                Signal::NeedsUser,
                &idle,
                false,
                Some(&question),
                now(),
                crate::lang::Lang::Es
            ),
            Change::Keep
        );
        let perm = json!({ "notification_type": "permission_prompt", "message": "Claude needs your permission to use Bash" });
        let Change::Set(a) = attention_change(
            Signal::NeedsUser,
            &perm,
            false,
            Some(&question),
            now(),
            crate::lang::Lang::Es,
        ) else {
            panic!("expected an attention");
        };
        assert_eq!(a.kind, AttentionKind::Permission);
    }

    #[test]
    fn answering_or_hitting_the_limit_clears_it() {
        let hook = json!({});
        assert_eq!(
            attention_change(
                Signal::PromptSubmitted,
                &hook,
                false,
                None,
                now(),
                crate::lang::Lang::Es
            ),
            Change::Clear
        );
        assert_eq!(
            attention_change(
                Signal::TurnEnded,
                &hook,
                true,
                None,
                now(),
                crate::lang::Lang::Es
            ),
            Change::Clear
        );
    }

    #[tokio::test]
    async fn a_reply_of_several_lines_is_one_paste_and_one_enter() {
        use std::time::Duration;

        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use delilah_agents::pty::PtySession;
        use portable_pty::CommandBuilder;
        use tower::ServiceExt;

        use crate::config::Config;
        use crate::store::Store;
        use crate::store_sessions::SessionKind;

        let home = std::env::temp_dir().join(format!("delilah-reply-{}", uuid::Uuid::new_v4()));
        let config = Config {
            home,
            addr: "127.0.0.1:7420".parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            claude_bin: "claude".into(),
            agent_program: None,
            ..crate::config::Config::test_defaults()
        };
        let app = AppState::new(Store::in_memory().await.unwrap(), config);
        // Prints, in hex, the exact bytes it receives: the paste plus the Enter.
        let expected = b"\x1b[200~uno\ndos\x1b[201~\r";
        let mut cmd = CommandBuilder::new("bash");
        cmd.args([
            "--noprofile",
            "--norc",
            "-c",
            &format!(
                "stty raw -echo; printf ready; v=$(dd bs=1 count={} 2>/dev/null | od -An -tx1 | tr -d ' \\n'); printf \"got:%s;\" \"$v\"; sleep 5",
                expected.len()
            ),
        ]);
        let pty = PtySession::spawn(cmd, 80, 24, None).unwrap();
        let (_, mut output) = pty.attach();
        app.sessions
            .insert("s-1", "PROJ-1", SessionKind::Claude, true, pty);

        let mut seen = String::new();
        let mut read_until = async |needle: &str| {
            tokio::time::timeout(Duration::from_secs(5), async {
                while !seen.contains(needle) {
                    seen.push_str(&String::from_utf8_lossy(&output.recv().await.unwrap()));
                }
            })
            .await
            .unwrap_or_else(|_| panic!("never saw {needle:?} in {seen:?}"));
        };
        read_until("ready").await;

        let req = Request::post("/api/v1/sessions/s-1/reply")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "text": "uno\r\ndos\n" }).to_string()))
            .unwrap();
        let res = crate::api::router(app.clone()).oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);

        let hex: String = expected.iter().map(|b| format!("{b:02x}")).collect();
        read_until(&format!("got:{hex};")).await;
    }

    #[tokio::test]
    async fn a_branch_error_is_an_item_until_cleared_and_never_for_archived_tasks() {
        let app = app_with_pr_task().await;
        let key = delilah_core::TaskKey::new("PROJ-1");
        let kinds = |app: AppState| async move {
            let Ok(Json(items)) = list(State(app)).await else {
                panic!("the inbox failed");
            };
            items
        };

        app.store
            .set_branch_error(&key, Some("no remote"))
            .await
            .unwrap();
        let items = kinds(app.clone()).await;
        let b = items.iter().find(|i| i.kind == ItemKind::Branch).unwrap();
        assert_eq!(b.task, "PROJ-1");
        assert_eq!(b.title, "Fix it");
        assert_eq!(b.text.as_deref(), Some("no remote"));

        app.store.set_branch_error(&key, None).await.unwrap();
        let items = kinds(app.clone()).await;
        assert!(items.iter().all(|i| i.kind != ItemKind::Branch));

        app.store
            .set_branch_error(&key, Some("no remote"))
            .await
            .unwrap();
        app.store
            .apply_event(&key, delilah_core::Event::MarkDone, chrono::Utc::now())
            .await
            .unwrap();
        let items = kinds(app).await;
        assert!(items.iter().all(|i| i.kind != ItemKind::Branch));
    }

    async fn items_of(app: &AppState) -> Vec<InboxItem> {
        let Ok(Json(items)) = list(State(app.clone())).await else {
            panic!("the inbox failed");
        };
        items
    }

    /// A task with a Failed status (non-blocking item) and a question (blocking).
    async fn app_with_a_question_and_a_failure() -> AppState {
        let app = app_with_pr_task().await;
        let key = delilah_core::TaskKey::new("PROJ-1");
        let mut other = Task::new(delilah_core::TaskKey::new("PROJ-2"), "Other", "feature");
        other.status = TaskStatus::Failed {
            reason: "boom".into(),
        };
        app.store.create_task(&other).await.unwrap();
        app.store
            .create_session(&crate::store_sessions::SessionRecord {
                id: "s-1".into(),
                task: key.to_string(),
                kind: SessionKind::Claude,
                label: "Principal".into(),
                primary: true,
                closed: false,
                read_only: false,
                conversation: false,
                step: None,
            })
            .await
            .unwrap();
        app.store
            .set_attention(
                "s-1",
                Some(&Attention {
                    kind: AttentionKind::Question,
                    text: "¿Cursor?".into(),
                    since: now(),
                }),
            )
            .await
            .unwrap();
        app
    }

    #[tokio::test]
    async fn every_item_has_a_stable_id() {
        let app = app_with_a_question_and_a_failure().await;
        let first = items_of(&app).await;
        let second = items_of(&app).await;
        assert!(first.len() >= 2);
        let ids = |v: &[InboxItem]| v.iter().map(|i| i.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&first), ids(&second));
        let q = first.iter().find(|i| i.kind == ItemKind::Question).unwrap();
        assert_eq!(
            q.id,
            format!("question:PROJ-1:s-1:{}", now().timestamp_millis())
        );
        let mut unique = ids(&first);
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), first.len());
        assert_eq!(serde_json::to_value(q).unwrap()["id"], q.id);
    }

    #[tokio::test]
    async fn a_new_occurrence_has_a_new_id() {
        let app = app_with_a_question_and_a_failure().await;
        let old = items_of(&app).await;
        let old = old.iter().find(|i| i.kind == ItemKind::Question).unwrap();
        app.store
            .set_attention(
                "s-1",
                Some(&Attention {
                    kind: AttentionKind::Question,
                    text: "¿Cursor?".into(),
                    since: now() + chrono::Duration::minutes(5),
                }),
            )
            .await
            .unwrap();
        let new = items_of(&app).await;
        let new = new.iter().find(|i| i.kind == ItemKind::Question).unwrap();
        assert_ne!(old.id, new.id);
    }

    async fn post_json(app: &AppState, uri: &str, body: Option<Value>) -> (u16, Value) {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;
        let mut req = Request::post(uri);
        let body = match body {
            Some(b) => {
                req = req.header("content-type", "application/json");
                Body::from(b.to_string())
            }
            None => Body::empty(),
        };
        let res = crate::api::router(app.clone())
            .oneshot(req.body(body).unwrap())
            .await
            .unwrap();
        let status = res.status().as_u16();
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, value)
    }

    #[tokio::test]
    async fn clear_dismisses_only_non_blocking_items_and_returns_their_ids() {
        let app = app_with_a_question_and_a_failure().await;
        let before = items_of(&app).await;
        let (status, body) = post_json(&app, "/api/v1/inbox/clear", None).await;
        assert_eq!(status, 200);
        let ids: Vec<String> = serde_json::from_value(body["ids"].clone()).unwrap();
        let expected: Vec<_> = before
            .iter()
            .filter(|i| !i.kind.is_blocking())
            .map(|i| i.id.clone())
            .collect();
        assert!(!expected.is_empty());
        assert_eq!(ids.len(), expected.len());
        assert!(expected.iter().all(|e| ids.contains(e)));
        let after = items_of(&app).await;
        assert!(after.iter().all(|i| i.kind.is_blocking()), "{after:?}");
        assert!(after.iter().any(|i| i.kind == ItemKind::Question));
        // Nothing new to clear the second time.
        let (_, body) = post_json(&app, "/api/v1/inbox/clear", None).await;
        assert_eq!(body["ids"], json!([]));
    }

    #[tokio::test]
    async fn restore_brings_back_exactly_those() {
        let app = app_with_a_question_and_a_failure().await;
        let before = items_of(&app).await.len();
        let (_, body) = post_json(&app, "/api/v1/inbox/clear", None).await;
        let (status, _) = post_json(
            &app,
            "/api/v1/inbox/restore",
            Some(json!({ "ids": body["ids"].clone() })),
        )
        .await;
        assert_eq!(status, 204);
        assert_eq!(items_of(&app).await.len(), before);
        let (status, _) = post_json(
            &app,
            "/api/v1/inbox/restore",
            Some(json!({ "ids": ["nope:X:y:1"] })),
        )
        .await;
        assert_eq!(status, 204);
        let many: Vec<String> = (0..501).map(|i| i.to_string()).collect();
        let (status, _) =
            post_json(&app, "/api/v1/inbox/restore", Some(json!({ "ids": many }))).await;
        assert_eq!(status, 400);
    }

    #[tokio::test]
    async fn dismissing_a_blocking_item_is_409_and_an_unlisted_one_404() {
        let app = app_with_a_question_and_a_failure().await;
        let items = items_of(&app).await;
        let q = items.iter().find(|i| i.kind == ItemKind::Question).unwrap();
        let enc = |id: &str| id.replace(':', "%3A");
        let (status, _) =
            post_json(&app, &format!("/api/v1/inbox/{}/dismiss", enc(&q.id)), None).await;
        assert_eq!(status, 409);
        let (status, _) = post_json(&app, "/api/v1/inbox/nope%3AX%3Ay%3A1/dismiss", None).await;
        assert_eq!(status, 404);

        let f = items.iter().find(|i| i.kind == ItemKind::Failed).unwrap();
        let uri = format!("/api/v1/inbox/{}/dismiss", enc(&f.id));
        let (status, _) = post_json(&app, &uri, None).await;
        assert_eq!(status, 204);
        assert!(
            items_of(&app)
                .await
                .iter()
                .all(|i| i.kind != ItemKind::Failed)
        );
    }

    #[tokio::test]
    async fn a_notice_item_is_dismissed_through_inbox_dismissals_only() {
        let app = stale_task().await;
        let item = items_of(&app).await.remove(0);
        assert_eq!(item.kind, ItemKind::BaseStale);
        let notice = item.notice.unwrap();
        let (status, _) =
            post_json(&app, &format!("/api/v1/pr-notices/{notice}/dismiss"), None).await;
        assert_eq!(status, 204);
        // The notice itself stays open; the dismissal row is the record.
        assert_eq!(app.store.open_notices().await.unwrap().len(), 1);
        assert!(app.store.dismissed().await.unwrap().contains(&item.id));
        assert!(items_of(&app).await.is_empty());
        post_json(
            &app,
            "/api/v1/inbox/restore",
            Some(json!({ "ids": [item.id] })),
        )
        .await;
        assert_eq!(items_of(&app).await.len(), 1);
    }

    #[tokio::test]
    async fn a_session_with_a_permission_ask_lists_only_the_permission_item() {
        let app = app_with_pr_task().await;
        let key = delilah_core::TaskKey::new("PROJ-1");
        app.store
            .apply_event(&key, delilah_core::Event::AgentAsked, now())
            .await
            .unwrap();
        app.store
            .create_session(&crate::store_sessions::SessionRecord {
                id: "s-1".into(),
                task: "PROJ-1".into(),
                kind: SessionKind::Claude,
                label: "Principal".into(),
                primary: true,
                closed: false,
                read_only: false,
                conversation: true,
                step: None,
            })
            .await
            .unwrap();
        app.permissions.insert(crate::permissions::PermissionAsk {
            id: "p-1".into(),
            request_id: "r-1".into(),
            task: "PROJ-1".into(),
            session: "s-1".into(),
            tool: "Bash".into(),
            input: json!({}),
            suggestion: None,
            rule: None,
            summary: "npm test".into(),
            at: now(),
        });
        let kinds: Vec<_> = items_of(&app)
            .await
            .iter()
            .map(|i| i.kind)
            .filter(|k| *k != ItemKind::PrOpened)
            .collect();
        assert_eq!(kinds, [ItemKind::Permission], "{kinds:?}");
    }

    #[tokio::test]
    async fn old_dismissals_are_purged() {
        let app = app_with_pr_task().await;
        app.store
            .dismiss(&[("old".into(), "PROJ-1".into()), ("new".into(), "PROJ-1".into())])
            .await
            .unwrap();
        sqlx::query("UPDATE inbox_dismissals SET dismissed_at = ? WHERE id = 'old'")
            .bind((Utc::now() - chrono::Duration::days(31)).to_rfc3339())
            .execute(&app.store.pool)
            .await
            .unwrap();
        let last = Mutex::new(None);
        let t0 = Instant::now();
        assert!(purge_if_due(&app, &last, t0).await);
        assert_eq!(
            app.store.dismissed().await.unwrap(),
            HashSet::from(["new".to_string()])
        );
        // At most hourly.
        assert!(!purge_if_due(&app, &last, t0 + Duration::from_secs(60)).await);
        assert!(purge_if_due(&app, &last, t0 + Duration::from_secs(3601)).await);
    }

    #[test]
    fn blocking_kinds_match_the_shared_fixture() {
        let raw = include_str!("../tests/fixtures/blocking_kinds.json");
        let mut fixture: Vec<String> = serde_json::from_str(raw).unwrap();
        let all = [
            ItemKind::Permission,
            ItemKind::Question,
            ItemKind::Error,
            ItemKind::PrFailed,
            ItemKind::PrReview,
            ItemKind::Branch,
            ItemKind::Waiting,
            ItemKind::Review,
            ItemKind::Handoff,
            ItemKind::Autopilot,
            ItemKind::SuggestReview,
            ItemKind::PrBlocked,
            ItemKind::BaseStale,
            ItemKind::Paused,
            ItemKind::Failed,
            ItemKind::TurnDone,
            ItemKind::PrOpened,
        ];
        let mut blocking: Vec<String> = all
            .iter()
            .filter(|k| k.is_blocking())
            .map(|k| k.name().to_string())
            .collect();
        blocking.sort();
        fixture.sort();
        assert_eq!(blocking, fixture);
        // `name` is the serde name.
        for k in all {
            assert_eq!(serde_json::to_value(k).unwrap(), k.name());
        }
    }

    #[tokio::test]
    async fn turn_done_and_pr_opened_are_removable_items() {
        let app = app_with_pr_task().await;
        app.store
            .create_session(&crate::store_sessions::SessionRecord {
                id: "s-2".into(),
                task: "PROJ-1".into(),
                kind: SessionKind::Claude,
                label: "Segunda".into(),
                primary: false,
                closed: false,
                read_only: false,
                conversation: false,
                step: None,
            })
            .await
            .unwrap();
        app.store
            .insert_notice(
                &delilah_core::TaskKey::new("PROJ-1"),
                &crate::store_prs::NewNotice {
                    repo: ".".into(),
                    kind: "turn-done".into(),
                    cause: "s-2:1".into(),
                    text: "listo".into(),
                    detail: Some(json!({ "session": "s-2" })),
                },
            )
            .await
            .unwrap();
        let items = items_of(&app).await;
        let kinds: Vec<_> = items.iter().map(|i| i.kind).collect();
        assert!(kinds.contains(&ItemKind::TurnDone), "{kinds:?}");
        assert!(kinds.contains(&ItemKind::PrOpened), "{kinds:?}");
        assert!(items.iter().all(|i| !i.kind.is_blocking()));
        let (_, body) = post_json(&app, "/api/v1/inbox/clear", None).await;
        assert_eq!(body["ids"].as_array().unwrap().len(), 2);
        assert!(items_of(&app).await.is_empty());
    }

    async fn app_with_pr_task() -> AppState {
        use crate::config::Config;
        use crate::store::Store;
        use crate::store_prs::NewPr;

        let config = Config {
            home: std::env::temp_dir().join(format!("delilah-inbox-{}", uuid::Uuid::new_v4())),
            addr: "127.0.0.1:7420".parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            claude_bin: "claude".into(),
            agent_program: None,
            ..crate::config::Config::test_defaults()
        };
        let app = AppState::new(Store::in_memory().await.unwrap(), config);
        let key = delilah_core::TaskKey::new("PROJ-1");
        let mut task = Task::new(key.clone(), "Fix it", "feature");
        task.status = TaskStatus::Running;
        app.store.create_task(&task).await.unwrap();
        app.store
            .link_pr(
                &key,
                &NewPr {
                    repo: ".".into(),
                    host: "github.com".into(),
                    owner_name: "me/repo".into(),
                    number: 7,
                    url: "https://github.com/me/repo/pull/7".into(),
                    linked_by: "detected".into(),
                },
            )
            .await
            .unwrap();
        app
    }

    async fn notify(app: &AppState, kind: &str, cause: &str) {
        use crate::store_prs::NewNotice;
        app.store
            .insert_notice(
                &delilah_core::TaskKey::new("PROJ-1"),
                &NewNotice {
                    repo: ".".into(),
                    kind: kind.into(),
                    cause: cause.into(),
                    text: format!("{kind} text"),
                    detail: None,
                },
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn open_pr_notices_are_items_after_their_neighbours_and_resolved_ones_are_not() {
        let app = app_with_pr_task().await;
        notify(&app, "archive-blocked", "a").await;
        notify(&app, "comments", "b").await;
        notify(&app, "ci-failed", "c").await;
        notify(&app, "changes-requested", "d").await;
        let gone = app.store.open_notices().await.unwrap();
        let resolved = gone.iter().find(|n| n.kind == "changes-requested").unwrap();
        app.store.resolve_notice(resolved.id).await.unwrap();
        app.review_hints.lock().unwrap().insert("PROJ-1".into());

        let Ok(Json(items)) = list(State(app)).await else {
            panic!("the inbox failed");
        };
        let kinds: Vec<_> = items.iter().map(|i| i.kind).collect();
        assert_eq!(
            kinds,
            [
                ItemKind::PrFailed,
                ItemKind::PrReview,
                ItemKind::SuggestReview,
                ItemKind::PrBlocked,
                // The link of the PR raised its aviso.
                ItemKind::PrOpened
            ]
        );
        let failed = &items[0];
        assert_eq!(failed.task, "PROJ-1");
        assert_eq!(failed.title, "Fix it");
        assert_eq!(failed.text.as_deref(), Some("ci-failed text"));
        assert!(failed.session.is_none());
        let pr = failed.pr.as_ref().unwrap();
        assert_eq!(
            (pr.url.as_str(), pr.repo.as_str()),
            ("https://github.com/me/repo/pull/7", ".")
        );
        let json = serde_json::to_value(failed).unwrap();
        assert_eq!(json["kind"], "pr-failed");
        assert_eq!(json["pr"]["notice"], pr.notice);
    }

    async fn stale_task() -> AppState {
        use crate::store_prs::NewNotice;
        let app = app_with_pr_task().await;
        // No PR: a stale base is about the task, not a PR.
        let key = delilah_core::TaskKey::new("PROJ-1");
        let row = app.store.prs_of(&key).await.unwrap().remove(0);
        app.store.unlink_pr(row.id).await.unwrap();
        app.store
            .insert_notice(
                &key,
                &NewNotice {
                    repo: ".".into(),
                    kind: "base-stale".into(),
                    cause: "start".into(),
                    text: "stale text".into(),
                    detail: None,
                },
            )
            .await
            .unwrap();
        app
    }

    #[tokio::test]
    async fn a_base_stale_notice_is_an_item_without_pr_until_dismissed() {
        let app = stale_task().await;
        let Ok(Json(items)) = list(State(app.clone())).await else {
            panic!("the inbox failed");
        };
        let stale: Vec<_> = items
            .iter()
            .filter(|i| i.kind == ItemKind::BaseStale)
            .collect();
        assert_eq!(stale.len(), 1, "{items:?}");
        let item = stale[0];
        let id = app.store.open_notices().await.unwrap()[0].id;
        assert_eq!(item.notice, Some(id));
        assert!(item.pr.is_none());
        assert_eq!(item.text.as_deref(), Some("stale text"));
        let json = serde_json::to_value(item).unwrap();
        assert_eq!(json["kind"], "base-stale");
        assert_eq!(json["notice"], id);
        assert!(json.get("pr").is_none());

        let status = crate::pr_api::dismiss(State(app.clone()), axum::extract::Path(id))
            .await
            .map_err(|e| e.status);
        assert_eq!(status, Ok(axum::http::StatusCode::NO_CONTENT));
        let Ok(Json(items)) = list(State(app)).await else {
            panic!("the inbox failed");
        };
        assert!(
            items.iter().all(|i| i.kind != ItemKind::BaseStale),
            "{items:?}"
        );
    }

    #[tokio::test]
    async fn archiving_resolves_the_base_stale_notice_and_other_items_have_no_notice_key() {
        let app = stale_task().await;
        let key = delilah_core::TaskKey::new("PROJ-1");
        app.store
            .apply_event(&key, delilah_core::Event::MarkDone, chrono::Utc::now())
            .await
            .unwrap();
        let Ok(Json(items)) = list(State(app.clone())).await else {
            panic!("the inbox failed");
        };
        assert!(
            items.iter().all(|i| i.kind != ItemKind::BaseStale),
            "{items:?}"
        );
        app.store.resolve_all_notices_of_task(&key).await.unwrap();
        assert!(app.store.open_notices().await.unwrap().is_empty());

        let plain = InboxItem {
            ..InboxItem::new(ItemKind::Waiting, "PROJ-1".into(), "t".into())
        };
        assert!(serde_json::to_value(plain).unwrap().get("notice").is_none());
    }

    #[tokio::test]
    async fn notices_of_an_archived_task_are_not_items() {
        let app = app_with_pr_task().await;
        notify(&app, "ci-failed", "c").await;
        let key = delilah_core::TaskKey::new("PROJ-1");
        app.store
            .apply_event(&key, delilah_core::Event::MarkDone, chrono::Utc::now())
            .await
            .unwrap();
        let Ok(Json(items)) = list(State(app)).await else {
            panic!("the inbox failed");
        };
        assert!(items.iter().all(|i| i.pr.is_none()), "{items:?}");
    }

    #[tokio::test]
    async fn an_archive_partial_of_a_done_task_without_workspace_still_reaches_the_inbox() {
        use crate::store_prs::NewNotice;
        let app = app_with_pr_task().await;
        let key = delilah_core::TaskKey::new("PROJ-1");
        // The PR is of a submodule; the notice is about the task (repo ".").
        let row = app.store.prs_of(&key).await.unwrap().remove(0);
        app.store
            .link_pr(
                &key,
                &crate::store_prs::NewPr {
                    repo: "svc-a".into(),
                    host: row.host.clone(),
                    owner_name: row.owner_name.clone(),
                    number: 8,
                    url: "https://github.com/me/svc-a/pull/8".into(),
                    linked_by: "detected".into(),
                },
            )
            .await
            .unwrap();
        app.store.unlink_pr(row.id).await.unwrap();
        for kind in ["archive-partial", "archive-blocked"] {
            app.store
                .insert_notice(
                    &key,
                    &NewNotice {
                        repo: ".".into(),
                        kind: kind.into(),
                        cause: "merge".into(),
                        text: format!("{kind} text"),
                        detail: None,
                    },
                )
                .await
                .unwrap();
        }
        app.store
            .apply_event(&key, delilah_core::Event::MarkDone, chrono::Utc::now())
            .await
            .unwrap();

        let Ok(Json(items)) = list(State(app)).await else {
            panic!("the inbox failed");
        };
        // The blocked one is for a task still holding its workspace: archived, it is gone.
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(items[0].kind, ItemKind::PrBlocked);
        assert_eq!(items[0].text.as_deref(), Some("archive-partial text"));
        let pr = items[0].pr.as_ref().unwrap();
        assert_eq!(pr.url, "https://github.com/me/svc-a/pull/8");
        assert_eq!(pr.repo, ".");
    }

    #[test]
    fn long_messages_are_cut_on_a_char_boundary() {
        let long = "ñ".repeat(MAX_TEXT);
        let cut = clip(&long);
        assert!(cut.ends_with('…'));
        assert!(cut.len() <= MAX_TEXT + '…'.len_utf8());
    }

    async fn app_with_handoff(h: Option<delilah_core::Handoff>) -> AppState {
        let config = crate::config::Config {
            home: std::env::temp_dir().join(format!("delilah-inbox-{}", uuid::Uuid::new_v4())),
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            agent_program: None,
            ..crate::config::Config::test_defaults()
        };
        let app = AppState::new(crate::store::Store::in_memory().await.unwrap(), config);
        let key = delilah_core::TaskKey::new("PROJ-1");
        let mut t = Task::new(key.clone(), "t", "feature");
        t.saving_mode = true;
        app.store.create_task(&t).await.unwrap();
        app.store
            .create_session(&crate::store_sessions::SessionRecord {
                id: "plan-1".into(),
                task: "PROJ-1".into(),
                kind: SessionKind::Claude,
                label: "Spec y plan".into(),
                primary: true,
                closed: false,
                read_only: false,
                conversation: false,
                step: Some(delilah_core::Step::Plan),
            })
            .await
            .unwrap();
        app.store.set_handoff(&key, h.as_ref()).await.unwrap();
        app
    }

    fn handoff(stage: HandoffStage, error: Option<&str>) -> Option<delilah_core::Handoff> {
        Some(delilah_core::Handoff {
            to: delilah_core::Step::Implement,
            stage,
            error: error.map(str::to_string),
        })
    }

    #[tokio::test]
    async fn an_offered_handoff_is_one_item_with_the_outgoing_session() {
        let app = app_with_handoff(handoff(HandoffStage::Offered, None)).await;
        let Json(items) = list(State(app)).await.ok().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, ItemKind::Handoff);
        assert_eq!(
            items[0].text.as_deref(),
            Some("Las tareas están listas. ¿Implementar en una sesión nueva?")
        );
        assert_eq!(items[0].session.as_ref().unwrap().id, "plan-1");
    }

    #[tokio::test]
    async fn a_failed_handoff_shows_its_error_and_others_show_nothing() {
        let app = app_with_handoff(handoff(HandoffStage::Failed, Some("no claude"))).await;
        let Json(items) = list(State(app)).await.ok().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text.as_deref(), Some("no claude"));
        for h in [handoff(HandoffStage::Skipped, None), None] {
            let app = app_with_handoff(h).await;
            let Json(items) = list(State(app)).await.ok().unwrap();
            assert!(items.is_empty());
        }
    }

    #[tokio::test]
    async fn a_starting_handoff_shows_text_for_the_review_and_the_implementation() {
        let app = app_with_handoff(Some(delilah_core::Handoff {
            to: Step::Review,
            stage: HandoffStage::Starting,
            error: None,
        }))
        .await;
        let Json(items) = list(State(app)).await.ok().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, ItemKind::Handoff);
        assert_eq!(
            items[0].text.as_deref(),
            Some("Pasando a la revisión en una sesión nueva…")
        );
        let app = app_with_handoff(handoff(HandoffStage::Starting, None)).await;
        let Json(items) = list(State(app)).await.ok().unwrap();
        assert_eq!(items[0].text.as_deref(), Some("Empezando la sesión nueva…"));
    }

    #[test]
    fn the_handoff_sorts_after_review_and_before_the_review_hint() {
        assert!(
            ItemKind::Review < ItemKind::Handoff && ItemKind::Handoff < ItemKind::SuggestReview
        );
    }

    async fn app_with_autopilot(state: Option<AutopilotState>) -> (AppState, TaskKey) {
        let app = app_with_handoff(None).await;
        let key = TaskKey::new("PROJ-1");
        let a = state.map(|state| delilah_core::Autopilot {
            state,
            stage: None,
            reason: (state == AutopilotState::Paused).then(|| "motivo".to_string()),
            since: now(),
            nudged: None,
            review_session: None,
        });
        app.store.set_autopilot(&key, a.as_ref()).await.unwrap();
        (app, key)
    }

    fn permission() -> Change {
        Change::Set(Attention {
            kind: AttentionKind::Permission,
            text: "ok?".into(),
            since: now(),
        })
    }

    #[tokio::test]
    async fn a_waiting_permission_ask_is_one_permission_item_with_its_details() {
        let (app, _) = app_with_autopilot(None).await;
        let attention = Attention {
            kind: AttentionKind::Permission,
            text: "Bash: npm test".into(),
            since: now(),
        };
        app.store
            .set_attention("plan-1", Some(&attention))
            .await
            .unwrap();
        app.sessions.insert(
            "plan-1",
            "PROJ-1",
            SessionKind::Claude,
            true,
            std::sync::Arc::new(
                delilah_agents::stream::StreamSession::spawn(tokio::process::Command::new("cat"))
                    .unwrap()
                    .0,
            ),
        );
        assert!(app.permissions.insert(crate::permissions::PermissionAsk {
            id: "ask-1".into(),
            request_id: "r-1".into(),
            task: "PROJ-1".into(),
            session: "plan-1".into(),
            tool: "Bash".into(),
            input: json!({ "command": "npm test" }),
            suggestion: Some(json!([{ "type": "setMode" }])),
            rule: Some("Bash(npm test)".into()),
            summary: "npm test".into(),
            at: now(),
        }));
        let Json(items) = list(State(app)).await.ok().unwrap();
        assert_eq!(items.len(), 1, "the attention does not repeat the ask");
        let value = serde_json::to_value(&items[0]).unwrap();
        assert_eq!(value["kind"], "permission");
        assert_eq!(value["session"]["id"], "plan-1");
        assert_eq!(value["permission"]["id"], "ask-1");
        assert_eq!(value["permission"]["tool"], "Bash");
        assert_eq!(value["permission"]["summary"], "npm test");
        assert_eq!(value["permission"]["rule"], "Bash(npm test)");
        assert_eq!(
            value["permission"]["suggestion"],
            json!([{ "type": "setMode" }])
        );
    }

    #[tokio::test]
    async fn a_permission_prompt_pauses_a_running_autopilot() {
        let (app, key) = app_with_autopilot(Some(AutopilotState::Running)).await;
        crate::autopilot::on_attention(&app, &key, "plan-1", &permission()).await;
        let t = app.store.get_task(&key).await.unwrap().unwrap();
        let a = t.autopilot.unwrap();
        assert_eq!(a.state, AutopilotState::Paused);
        assert_eq!(a.reason.as_deref(), Some("La sesión espera un permiso."));
    }

    #[tokio::test]
    async fn a_permission_prompt_of_another_session_leaves_autopilot_running() {
        let (app, key) = app_with_autopilot(Some(AutopilotState::Running)).await;
        crate::autopilot::on_attention(&app, &key, "side-terminal", &permission()).await;
        let t = app.store.get_task(&key).await.unwrap().unwrap();
        assert!(crate::autopilot::is_running(&t));
    }

    #[tokio::test]
    async fn other_attentions_do_not_pause_autopilot() {
        let (app, key) = app_with_autopilot(Some(AutopilotState::Running)).await;
        let q = Change::Set(Attention {
            kind: AttentionKind::Question,
            text: "?".into(),
            since: now(),
        });
        crate::autopilot::on_attention(&app, &key, "plan-1", &q).await;
        crate::autopilot::on_attention(&app, &key, "plan-1", &Change::Clear).await;
        let t = app.store.get_task(&key).await.unwrap().unwrap();
        assert!(crate::autopilot::is_running(&t));
    }

    #[tokio::test]
    async fn a_paused_autopilot_lists_one_item_with_the_reason() {
        let (app, _) = app_with_autopilot(Some(AutopilotState::Paused)).await;
        let Json(items) = list(State(app)).await.ok().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, ItemKind::Autopilot);
        assert_eq!(items[0].text.as_deref(), Some("motivo"));
        assert_eq!(
            serde_json::to_value(items[0].kind).unwrap(),
            json!("autopilot")
        );
    }

    #[tokio::test]
    async fn a_running_or_done_autopilot_lists_nothing() {
        for state in [AutopilotState::Running, AutopilotState::Done] {
            let (app, _) = app_with_autopilot(Some(state)).await;
            let Json(items) = list(State(app)).await.ok().unwrap();
            assert!(items.is_empty());
        }
    }

    #[tokio::test]
    async fn the_usage_limit_leaves_autopilot_running() {
        use delilah_core::Event;
        let (app, key) = app_with_autopilot(Some(AutopilotState::Running)).await;
        for e in [Event::Start, Event::WorkspaceReady] {
            app.transition(&key, e).await.unwrap();
        }
        let resume_at = now() + chrono::Duration::hours(1);
        app.transition(&key, Event::LimitReached { resume_at })
            .await
            .unwrap();
        // The limit itself clears attention: nothing to pause on.
        let hook = json!({ "hook_event_name": "Stop" });
        let change = attention_change(Signal::TurnEnded, &hook, true, None, now(), app.lang());
        assert_eq!(change, Change::Clear);
        crate::autopilot::on_attention(&app, &key, "plan-1", &change).await;
        app.transition(&key, Event::ResetDue { now: resume_at })
            .await
            .unwrap();
        let t = app.store.get_task(&key).await.unwrap().unwrap();
        assert!(crate::autopilot::is_running(&t));
        let found = Some(crate::handoff::Boundary::Offer(Step::Implement));
        assert!(matches!(
            crate::autopilot::shape(&t, found),
            Some(crate::handoff::Boundary::Start(Step::Implement))
        ));
        let Json(items) = list(State(app)).await.ok().unwrap();
        assert!(items.iter().all(|i| i.kind != ItemKind::Autopilot));
    }
}
