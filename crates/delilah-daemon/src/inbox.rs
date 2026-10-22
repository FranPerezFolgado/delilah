//! The inbox: everything that waits on the user, across tasks. A session's hooks say
//! when it needs you (a question at the end of a turn, a permission, an error); the
//! task's state adds the rest (paused by the usage limit, ready for review, failed).
//! An item goes away by itself once it is dealt with.

use std::collections::HashSet;

use axum::Json;
use axum::extract::{Path, State};
use chrono::{DateTime, Utc};
use delilah_agents::claude::Signal;
use delilah_core::{AutopilotState, HandoffStage, SpecPhase, Step, Task, TaskKey, TaskStatus};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::actions::type_prompt;
use crate::api::ApiError;
use crate::archive::{KIND_BLOCKED, KIND_PARTIAL};
use crate::state::AppState;
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

/// A permission prompt is something autopilot cannot answer: it pauses (a no-op when
/// the task is not on autopilot). The usage limit never gets here: it clears attention.
pub async fn pause_autopilot_on(app: &AppState, key: &TaskKey, change: &Change) {
    if matches!(change, Change::Set(a) if a.kind == AttentionKind::Permission) {
        let reason = app.lang().pick(
            "La sesión espera un permiso.",
            "The session is waiting for a permission.",
        );
        crate::autopilot::pause(app, key, reason).await;
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
    /// The CI of a task's PR failed.
    PrFailed,
    /// A reviewer asked for changes or commented on a task's PR.
    PrReview,
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
    /// The PR notice this item stands for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr: Option<InboxPr>,
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

/// `GET /api/v1/inbox`: most urgent first (what blocks an agent), newest first within.
pub async fn list(State(app): State<AppState>) -> Result<Json<Vec<InboxItem>>, ApiError> {
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
            kind: ItemKind::Question,
            task: ask.task.clone(),
            title: title.clone(),
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
            pr: None,
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
            title: title.clone(),
            session: Some(ItemSession {
                id: s.id,
                label: s.label,
                primary: s.primary,
                live,
            }),
            text: Some(attention.text),
            since: Some(attention.since),
            ask: None,
            pr: None,
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
            let progress = format!(
                "{}/{} {}",
                s.done,
                s.total,
                app.lang().pick("subtareas hechas", "subtasks done")
            );
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
                pr: None,
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
            kind: ItemKind::Handoff,
            task: t.key.to_string(),
            title: t.title.clone(),
            session,
            text: Some(text),
            since: None,
            ask: None,
            pr: None,
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
            kind: ItemKind::Autopilot,
            task: t.key.to_string(),
            title: t.title.clone(),
            session: None,
            text: Some(a.reason.clone().unwrap_or_else(|| {
                app.lang()
                    .pick("El autopiloto está en pausa.", "Autopilot is paused.")
                    .to_string()
            })),
            since: Some(a.since),
            ask: None,
            pr: None,
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
            kind: ItemKind::SuggestReview,
            task: t.key.to_string(),
            title: t.title.clone(),
            session: None,
            text: Some(
                app.lang()
                    .pick(
                        "El agente ha parado con commits en su rama.",
                        "The agent stopped with commits on its branch.",
                    )
                    .to_string(),
            ),
            since: None,
            ask: None,
            pr: None,
        });
    }

    for n in app.store.open_notices().await? {
        let kind = match n.kind.as_str() {
            "ci-failed" => ItemKind::PrFailed,
            "changes-requested" | "comments" => ItemKind::PrReview,
            KIND_BLOCKED | KIND_PARTIAL => ItemKind::PrBlocked,
            _ => continue,
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
        // An unlinked PR resolves its notices; one still open without its PR is stale.
        let prs = app.store.prs_of(&t.key).await?;
        let same_repo = prs.iter().find(|p| p.repo == n.repo);
        let Some(pr) = same_repo.or(if archive { prs.first() } else { None }) else {
            continue;
        };
        items.push(InboxItem {
            kind,
            task: n.task_key,
            title: t.title.clone(),
            session: None,
            text: Some(n.text),
            since: Some(n.created_at),
            ask: None,
            pr: Some(InboxPr {
                notice: n.id,
                url: pr.url.clone(),
                repo: n.repo,
            }),
        });
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
            agent_program: None,
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
            agent_program: None,
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
                ItemKind::PrBlocked
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
    async fn a_permission_prompt_pauses_a_running_autopilot() {
        let (app, key) = app_with_autopilot(Some(AutopilotState::Running)).await;
        pause_autopilot_on(&app, &key, &permission()).await;
        let t = app.store.get_task(&key).await.unwrap().unwrap();
        let a = t.autopilot.unwrap();
        assert_eq!(a.state, AutopilotState::Paused);
        assert_eq!(a.reason.as_deref(), Some("La sesión espera un permiso."));
    }

    #[tokio::test]
    async fn other_attentions_do_not_pause_autopilot() {
        let (app, key) = app_with_autopilot(Some(AutopilotState::Running)).await;
        let q = Change::Set(Attention {
            kind: AttentionKind::Question,
            text: "?".into(),
            since: now(),
        });
        pause_autopilot_on(&app, &key, &q).await;
        pause_autopilot_on(&app, &key, &Change::Clear).await;
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
        pause_autopilot_on(&app, &key, &change).await;
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
