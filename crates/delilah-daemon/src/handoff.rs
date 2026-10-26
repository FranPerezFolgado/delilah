//! Saving mode: each phase of a task runs in a fresh Claude session. This module
//! decides when a step is done (`boundary`) and carries out the handoff to the next
//! one: offer it, wait for the user, start the new session, or undo it if it cannot
//! start. The state lives in `tasks.handoff`, written before each side effect, so a
//! restart can finish or re-offer a handoff (see `specs/004-saving-mode`).

use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use axum::Json;
use axum::extract::{Path as AxPath, State};
use delilah_core::workflow::{ProjectWorkflow, Vars, render};
use delilah_core::{Handoff, HandoffStage, Step, Task, TaskKey};
use delilah_git::{GitOptions, Workspace};

use crate::api::{ActionReq, ApiError};
use crate::autopilot;
use crate::events::Topic;
use crate::lang::Lang;
use crate::specs::SpecSummary;
use crate::state::AppState;
use crate::store_sessions::{SessionKind, SessionRecord};

/// What the debug log says when a turn ends with background work running; the hooks tests
/// count lines with it.
pub(crate) const BACKGROUND_WAIT_LOG: &str = "turn ended with background work running: no stall";

/// How long the stopped session gets to exit before the new one is launched.
const KILL_WAIT: Duration = Duration::from_secs(3);

/// What a handoff's session launch needs: the shared bound of the launcher, which
/// tests replace with a stub.
pub(crate) trait Launcher:
    for<'a> Fn(&'a AppState, &'a SessionRecord, &'a Path, Option<&'a str>, bool) -> LaunchFuture<'a>
{
}

impl<F> Launcher for F where
    F: for<'a> Fn(
        &'a AppState,
        &'a SessionRecord,
        &'a Path,
        Option<&'a str>,
        bool,
    ) -> LaunchFuture<'a>
{
}

/// What a launch gives back: a stream session is up only once the agent said hello.
pub(crate) type LaunchFuture<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ApiError>> + Send + 'a>>;

/// The handoffs' shared state, one per daemon (in `AppState`).
#[derive(Default)]
pub struct HandoffCoordinator {
    /// Serialises the check-and-set of a task's handoff, so two confirmations (or a
    /// confirmation and a turn end) can never both start the same step.
    guard: tokio::sync::Mutex<()>,
    /// Tasks whose new session is being started right now, so a recovery tick never
    /// starts the same step a second time. Only changed while holding `guard`.
    starting: std::sync::Mutex<HashSet<String>>,
}

impl HandoffCoordinator {
    fn is_starting(&self, key: &TaskKey) -> bool {
        let starting = self.starting.lock().unwrap_or_else(|e| e.into_inner());
        starting.contains(key.as_str())
    }

    /// Marks the task as starting; false if it already was.
    fn mark_starting(&self, key: &TaskKey) -> bool {
        let mut starting = self.starting.lock().unwrap_or_else(|e| e.into_inner());
        starting.insert(key.to_string())
    }

    fn unmark_starting(&self, key: &TaskKey) {
        let mut starting = self.starting.lock().unwrap_or_else(|e| e.into_inner());
        starting.remove(key.as_str());
    }
}

/// What the end of a step asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Boundary {
    /// Ask the user before moving to the step.
    Offer(Step),
    /// Move to the step on its own.
    Start(Step),
    /// The review is over: autopilot opens the PRs.
    Finish,
}

/// Whether the step the primary session runs has reached a boundary: the spec is
/// ready to implement (offer) or fully checked (review starts on its own). When both
/// hold, the furthest step wins.
pub(crate) fn boundary(
    task: &Task,
    primary_step: Option<Step>,
    spec: &SpecSummary,
) -> Option<Boundary> {
    if !task.saving_mode || spec.dir.is_none() || spec.total == 0 {
        return None;
    }
    let skipped = match &task.handoff {
        None => false,
        Some(h) if h.stage == HandoffStage::Skipped => true,
        // A pending offer for the implementation is overtaken by the review.
        Some(h) if h.stage == HandoffStage::Offered && h.to == Step::Implement => false,
        Some(_) => return None,
    };
    let all_done = spec.done == spec.total;
    match primary_step? {
        Step::Plan if all_done => Some(Boundary::Start(Step::Review)),
        Step::Plan if spec.done == 0 && !skipped && task.handoff.is_none() => {
            Some(Boundary::Offer(Step::Implement))
        }
        Step::Implement if all_done => Some(Boundary::Start(Step::Review)),
        Step::Review if autopilot::is_running(task) => Some(Boundary::Finish),
        _ => None,
    }
}

/// The label of a step's session.
pub(crate) fn step_label(lang: Lang, step: Step) -> &'static str {
    match step {
        Step::Plan => lang.pick("Spec y plan", "Spec and plan"),
        Step::Implement => lang.pick("Implementación", "Implementation"),
        Step::Review => lang.pick("Revisión", "Review"),
    }
}

/// What the first session is told on top of its prompt: stop once `tasks.md` is
/// written, the next session takes over.
pub(crate) fn plan_note(lang: Lang) -> &'static str {
    lang.pick(
        "Modo ahorro: cuando `tasks.md` esté escrito, para y no implementes; Delilah seguirá en una sesión nueva.",
        "Saving mode: once `tasks.md` is written, stop and do not implement; Delilah will carry on in a new session.",
    )
}

/// What a step's session is told to do: the project's own command for it, with
/// `{spec_dir}` and the other placeholders filled in (`vars`). Nothing for the plan
/// step (the start sends it) or for a workflow that is not phased.
fn step_prompt(step: Step, workflow: &ProjectWorkflow, vars: &Vars) -> Option<String> {
    let ProjectWorkflow::Phased {
        implement, review, ..
    } = workflow
    else {
        return None;
    };
    match step {
        Step::Plan => None,
        Step::Implement => Some(render(implement, vars)),
        Step::Review => Some(render(review, vars)),
    }
}

/// Reads the task's spec now (blocking work off the async threads).
async fn read_spec(app: &AppState, key: &TaskKey) -> Option<SpecSummary> {
    let (_, ws) = app.store.workspace(key).await.ok()??;
    if !ws.path.is_dir() {
        return None;
    }
    let opts = GitOptions {
        allow_file_protocol: app.config.allow_file_protocol,
    };
    let name = key.to_string();
    let spec = tokio::task::spawn_blocking(move || crate::specs::read_spec(&ws, &name, &opts))
        .await
        .ok()?;
    Some(spec.summary)
}

/// The task's main session, if it is a Claude one.
pub(crate) async fn primary_session(app: &AppState, key: &TaskKey) -> Option<SessionRecord> {
    let sessions = app.store.open_sessions(Some(key.as_str())).await.ok()?;
    sessions
        .into_iter()
        .find(|s| s.primary && s.kind == SessionKind::Claude)
}

async fn task_of(app: &AppState, key: &TaskKey) -> Result<Task, ApiError> {
    app.store
        .get_task(key)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("no task {key}")))
}

async fn set(app: &AppState, key: &TaskKey, to: Step, stage: HandoffStage, error: Option<String>) {
    let handoff = Handoff { to, stage, error };
    if let Err(e) = app.store.set_handoff(key, Some(&handoff)).await {
        tracing::warn!(task = %key, "cannot record the handoff: {e}");
    }
    app.emit(Topic::Inbox);
    app.emit(Topic::Tasks);
}

/// The `save-notes` preference: on unless the user turned it off.
async fn save_notes_preference(app: &AppState) -> bool {
    let value: Option<serde_json::Value> =
        app.store.setting("pref:save-notes").await.ok().flatten();
    value
        .as_ref()
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true)
}

/// Where a confirmed handoff goes next: the one place that decides between saving
/// the outgoing session's notes and starting the new session right away. Saving
/// writes the `saving` stage first and then types `/save-session` into the outgoing
/// primary; with the preference off, no live session to ask, or the typing failing,
/// the new session starts at once.
///
/// Takes the guard of the caller (which has checked the handoff) and releases it.
/// `entered` runs once the handoff is under way (saving written, or start marked),
/// before the launch.
async fn enter<L: Launcher>(
    app: &AppState,
    key: &TaskKey,
    to: Step,
    guard: tokio::sync::MutexGuard<'_, ()>,
    launch: &L,
    entered: impl FnOnce(),
) {
    if save_notes_preference(app).await
        && let Some(live) = app.sessions.primary(key.as_str())
    {
        set(app, key, to, HandoffStage::Saving, None).await;
        match crate::actions::type_prompt(&live, "/save-session").await {
            Ok(()) => return entered(),
            Err(e) => {
                tracing::warn!(task = %key, "cannot ask for the notes: {}", e.message());
            }
        }
    }
    if !app.handoffs.mark_starting(key) {
        return;
    }
    set(app, key, to, HandoffStage::Starting, None).await;
    entered();
    drop(guard);
    start_with(app, key, to, launch).await;
}

/// The primary session finished a turn (not on the usage limit): see whether its step
/// is over. Offers the next step or starts it, as `boundary` says.
#[cfg(test)]
pub(crate) async fn on_turn_ended(app: &AppState, key: &TaskKey) {
    on_turn_end(app, key, TurnEnd::default()).await;
}

pub(crate) async fn on_turn_ended_with<L: Launcher>(app: &AppState, key: &TaskKey, launch: &L) {
    on_turn_end_with(app, key, TurnEnd::default(), launch).await;
}

/// What the hook said about the turn that ended.
#[derive(Default, Clone, Copy)]
pub(crate) struct TurnEnd {
    /// Background work (subagents, shells) was still running when the turn ended.
    pub background: bool,
}

pub(crate) async fn on_turn_end(app: &AppState, key: &TaskKey, end: TurnEnd) {
    on_turn_end_with(app, key, end, &crate::run::launch_session).await;
}

pub(crate) async fn on_turn_end_with<L: Launcher>(
    app: &AppState,
    key: &TaskKey,
    end: TurnEnd,
    launch: &L,
) {
    // The stage is read under the guard: a save turn that ends right after the
    // prompt was typed finds `saving` already written.
    let guard = app.handoffs.guard.lock().await;
    let Ok(task) = task_of(app, key).await else {
        return;
    };
    if !task.saving_mode {
        return;
    }
    if matches!(&task.handoff, Some(h) if h.stage == HandoffStage::Saving) {
        return finish_save(app, key, guard, launch).await;
    }
    let Some(primary) = primary_session(app, key).await else {
        return;
    };
    let Some(spec) = read_spec(app, key).await else {
        return;
    };
    let Some(found) = autopilot::shape(&task, boundary(&task, primary.step, &spec)) else {
        drop(guard);
        // Waiting only skips the stall; it never blocks `finish_save`, the boundary or
        // a handoff, which all ran above or below on every turn end.
        if end.background {
            tracing::debug!(task = %key, "{BACKGROUND_WAIT_LOG}");
            return;
        }
        // No boundary on autopilot is a stall (which does nothing off autopilot or
        // while a handoff is under way).
        autopilot::stall(app, key, &task, primary.step, &spec).await;
        return;
    };
    match found {
        Boundary::Offer(to) => set(app, key, to, HandoffStage::Offered, None).await,
        Boundary::Start(to) => {
            enter(app, key, to, guard, launch, || {
                announce(app, key, &task, to)
            })
            .await;
        }
        Boundary::Finish => {
            drop(guard);
            // Waits for a preview and opens PRs: not in the turn-end path, which goes
            // on to the hint and must not be held up.
            let (app, key, session) = (app.clone(), key.clone(), primary.id.clone());
            tokio::spawn(async move { autopilot::finish(&app, &key, &session).await });
        }
    }
}

/// Tells the user, like a finished turn does, that a step started on its own.
/// Quiet when the app is listening: it notifies itself, as for a finished turn.
fn announce(app: &AppState, key: &TaskKey, task: &Task, to: Step) {
    if app.events.listening() {
        return;
    }
    let lang = app.lang();
    app.notifier.send(
        format!(
            "{key} {}",
            match to {
                Step::Implement =>
                    lang.pick("pasa a la implementación", "moves on to the implementation"),
                _ => lang.pick("pasa a la revisión", "moves on to the review"),
            }
        ),
        format!("{}: {}", step_label(lang, to), task.title),
    );
}

/// The outgoing session's save turn ended: the new session can start.
async fn finish_save<L: Launcher>(
    app: &AppState,
    key: &TaskKey,
    guard: tokio::sync::MutexGuard<'_, ()>,
    launch: &L,
) {
    let Ok(fresh) = task_of(app, key).await else {
        return;
    };
    let to = match &fresh.handoff {
        Some(h) if h.stage == HandoffStage::Saving => h.to,
        _ => return,
    };
    if !app.handoffs.mark_starting(key) {
        return;
    }
    set(app, key, to, HandoffStage::Starting, None).await;
    drop(guard);
    start_with(app, key, to, launch).await;
}

/// The primary session's turn failed (not on the usage limit): a handoff waiting for
/// its notes cannot go on by itself, so it stops as failed, to retry.
pub(crate) async fn on_turn_failed(app: &AppState, key: &TaskKey) {
    let _guard = app.handoffs.guard.lock().await;
    let Ok(task) = task_of(app, key).await else {
        return;
    };
    let lang = app.lang();
    let mut reason = lang
        .pick("la sesión falló", "the session failed")
        .to_string();
    if let Some(h) = task
        .handoff
        .as_ref()
        .filter(|h| task.saving_mode && h.stage == HandoffStage::Saving)
    {
        let message = lang
            .pick(
                "la sesión no pudo guardar las notas",
                "the session could not save its notes",
            )
            .to_string();
        set(app, key, h.to, HandoffStage::Failed, Some(message.clone())).await;
        reason = message;
    }
    // A step's session that fails pauses the autopilot (the task keeps its phase).
    if autopilot::is_running(&task) {
        autopilot::pause(app, key, reason).await;
    }
}

/// The user accepts the offered step (which also approves the tasks).
pub(crate) async fn confirm(app: &AppState, key: &TaskKey) -> Result<(), ApiError> {
    confirm_with(app, key, &crate::run::launch_session).await
}

pub(crate) async fn confirm_with<L: Launcher>(
    app: &AppState,
    key: &TaskKey,
    launch: &L,
) -> Result<(), ApiError> {
    let guard = app.handoffs.guard.lock().await;
    let task = task_of(app, key).await?;
    let to = match &task.handoff {
        Some(h) if h.stage == HandoffStage::Offered => h.to,
        _ => {
            return Err(ApiError::conflict(format!(
                "{key} has no handoff to confirm"
            )));
        }
    };
    enter(app, key, to, guard, launch, || {}).await;
    Ok(())
}

/// The user keeps working in the same session: no handoff, and no saving mode.
pub(crate) async fn decline(app: &AppState, key: &TaskKey) -> Result<(), ApiError> {
    let _guard = app.handoffs.guard.lock().await;
    let task = task_of(app, key).await?;
    if !matches!(&task.handoff, Some(h) if h.stage == HandoffStage::Offered) {
        return Err(ApiError::conflict(format!(
            "{key} has no handoff to decline"
        )));
    }
    app.store.set_handoff(key, None).await?;
    app.store.set_saving_mode(key, false).await?;
    app.emit(Topic::Inbox);
    app.emit(Topic::Tasks);
    Ok(())
}

/// Tries again a handoff whose new session could not start.
pub(crate) async fn retry(app: &AppState, key: &TaskKey) -> Result<(), ApiError> {
    retry_with(app, key, &crate::run::launch_session).await
}

pub(crate) async fn retry_with<L: Launcher>(
    app: &AppState,
    key: &TaskKey,
    launch: &L,
) -> Result<(), ApiError> {
    let guard = app.handoffs.guard.lock().await;
    let task = task_of(app, key).await?;
    let to = match &task.handoff {
        Some(h) if h.stage == HandoffStage::Failed => h.to,
        _ => return Err(ApiError::conflict(format!("{key} has no failed handoff"))),
    };
    if !app.handoffs.mark_starting(key) {
        return Ok(());
    }
    set(app, key, to, HandoffStage::Starting, None).await;
    drop(guard);
    start_with(app, key, to, launch).await;
    Ok(())
}

/// The `saving-mode` preference: off unless the user turned it on.
pub(crate) async fn saving_mode_preference(app: &AppState) -> bool {
    let value: Option<serde_json::Value> =
        app.store.setting("pref:saving-mode").await.ok().flatten();
    value
        .as_ref()
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

/// The plan session edited code on its own: the user is implementing there, so no
/// new session is offered for it.
pub(crate) async fn on_code_edit(app: &AppState, key: &TaskKey) {
    let _guard = app.handoffs.guard.lock().await;
    let Ok(task) = task_of(app, key).await else {
        return;
    };
    let open = match &task.handoff {
        None => true,
        Some(h) => h.stage == HandoffStage::Offered,
    };
    if !task.saving_mode || !open {
        return;
    }
    let Some(primary) = primary_session(app, key).await else {
        return;
    };
    if primary.step == Some(Step::Plan) {
        set(app, key, Step::Implement, HandoffStage::Skipped, None).await;
    }
}

/// The user moved the task forward to implement: offer the new session if it is still
/// in its plan one.
pub(crate) async fn on_implement_chosen(app: &AppState, key: &TaskKey) {
    let _guard = app.handoffs.guard.lock().await;
    let Ok(task) = task_of(app, key).await else {
        return;
    };
    if !task.saving_mode || task.handoff.is_some() {
        return;
    }
    let Some(primary) = primary_session(app, key).await else {
        return;
    };
    if primary.step == Some(Step::Plan) {
        set(app, key, Step::Implement, HandoffStage::Offered, None).await;
    }
}

/// Finishes or re-offers a handoff a restart (or a failed start) left half done.
pub(crate) async fn recover(app: &AppState, task: &Task) {
    recover_with(app, task, &crate::run::launch_session).await;
}

async fn recover_with<L: Launcher>(app: &AppState, task: &Task, launch: &L) {
    let Some(h) = task.handoff.as_ref().filter(|_| task.saving_mode) else {
        return;
    };
    let key = &task.key;
    let guard = app.handoffs.guard.lock().await;
    // A start in flight owns the handoff: it will write the result itself.
    if app.handoffs.is_starting(key) {
        return;
    }
    // Decide on what is stored now, not on what the tick read a moment ago.
    let Ok(fresh) = task_of(app, key).await else {
        return;
    };
    if fresh.handoff.as_ref() != Some(h) {
        return;
    }
    match h.stage {
        HandoffStage::Starting => {
            let session = app.store.session_of_step(key.as_str(), h.to).await;
            // A session with no live process was never launched (the daemon died
            // between recording it and launching it): launch it now.
            let running = match &session {
                Ok(Some(s)) => app.sessions.get(&s.id).is_some(),
                _ => false,
            };
            if running {
                if let Err(e) = app.store.set_handoff(key, None).await {
                    tracing::warn!(task = %key, "cannot clear the handoff: {e}");
                }
                app.emit(Topic::Inbox);
                app.emit(Topic::Tasks);
            } else {
                app.handoffs.mark_starting(key);
                drop(guard);
                start_with(app, key, h.to, launch).await;
            }
        }
        HandoffStage::Saving => {
            let gone = match primary_session(app, key).await {
                Some(p) => app.sessions.get(&p.id).is_none(),
                None => true,
            };
            if !gone {
                return;
            }
            // The notes cannot be saved any more: carry on without them.
            if h.to == Step::Implement {
                set(app, key, h.to, HandoffStage::Offered, None).await;
            } else {
                app.handoffs.mark_starting(key);
                set(app, key, h.to, HandoffStage::Starting, None).await;
                drop(guard);
                start_with(app, key, h.to, launch).await;
            }
        }
        _ => {}
    }
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct SavingModeReq {
    pub on: bool,
}

/// `POST /api/v1/tasks/{key}/saving-mode`: turns it on or off on a task; off drops a
/// pending handoff (the sessions stay).
pub(crate) async fn set_saving_mode(
    State(app): State<AppState>,
    AxPath(key): AxPath<String>,
    Json(req): Json<SavingModeReq>,
) -> Result<Json<Task>, ApiError> {
    let key = TaskKey::new(key);
    Ok(Json(apply_saving_mode(&app, &key, req.on).await?))
}

/// Turns saving mode on or off on a task, under the handoff guard; off drops a
/// pending handoff (the sessions stay). Returns the task as stored.
pub(crate) async fn apply_saving_mode(
    app: &AppState,
    key: &TaskKey,
    on: bool,
) -> Result<Task, ApiError> {
    let _guard = app.handoffs.guard.lock().await;
    task_of(app, key).await?;
    app.store.set_saving_mode(key, on).await?;
    if !on {
        app.store.set_handoff(key, None).await?;
    }
    app.emit(Topic::Inbox);
    app.emit(Topic::Tasks);
    task_of(app, key).await
}

/// `POST /api/v1/tasks/{key}/handoff`: confirm, decline or retry the handoff.
pub(crate) async fn act(
    State(app): State<AppState>,
    AxPath(key): AxPath<String>,
    Json(req): Json<ActionReq>,
) -> Result<Json<Task>, ApiError> {
    let key = TaskKey::new(key);
    match req.action.as_str() {
        "confirm" => confirm(&app, &key).await?,
        "decline" => decline(&app, &key).await?,
        "retry" => retry(&app, &key).await?,
        other => {
            return Err(ApiError::bad_request(format!("unknown action `{other}`")));
        }
    }
    Ok(Json(task_of(&app, &key).await?))
}

/// Starts `step`'s session: it becomes the main one, the previous is stopped (and
/// stays listed), and the handoff is cleared. If the new session cannot start the
/// previous one is main again and the handoff is `failed`, to retry.
///
/// The caller has marked the task as starting (under the guard) and dropped the guard.
async fn start_with<L: Launcher>(app: &AppState, key: &TaskKey, step: Step, launch: &L) {
    let result = try_start(app, key, step, launch).await;
    // The final write is made under the guard, and only if the handoff is still the
    // one being started: a decline or saving mode turned off meanwhile stands.
    let _guard = app.handoffs.guard.lock().await;
    app.handoffs.unmark_starting(key);
    let current = app.store.get_task(key).await.ok().flatten();
    let still_starting = matches!(
        current.as_ref().and_then(|t| t.handoff.as_ref()),
        Some(h) if h.stage == HandoffStage::Starting && h.to == step
    );
    match result {
        Ok(()) => {
            if still_starting && let Err(e) = app.store.set_handoff(key, None).await {
                tracing::warn!(task = %key, "cannot clear the handoff: {e}");
            }
            tracing::info!(task = %key, step = step.name(), "handoff done");
        }
        Err(message) => {
            tracing::warn!(task = %key, step = step.name(), "handoff failed: {message}");
            if still_starting {
                set(app, key, step, HandoffStage::Failed, Some(message.clone())).await;
                if current.as_ref().is_some_and(autopilot::is_running) {
                    autopilot::pause(app, key, message).await;
                }
            }
        }
    }
    app.emit(Topic::Sessions);
    app.emit(Topic::Inbox);
    app.emit(Topic::Tasks);
}

/// Whether a spec folder can go into the `/orchestrate` prompt as it is.
fn safe_dir(dir: &str) -> bool {
    !dir.is_empty()
        && !dir.contains("..")
        && dir
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-'))
}

async fn try_start<L: Launcher>(
    app: &AppState,
    key: &TaskKey,
    step: Step,
    launch: &L,
) -> Result<(), String> {
    let ws: Workspace = match app.store.workspace(key).await {
        Ok(Some((_, ws))) if ws.path.is_dir() => ws,
        Ok(_) => return Err("the task has no workspace".into()),
        Err(e) => return Err(e.to_string()),
    };
    let dir = read_spec(app, key).await.and_then(|s| s.dir);
    let lang = app.lang();
    if let Some(d) = dir
        .as_deref()
        .filter(|d| step == Step::Implement && !safe_dir(d))
    {
        return Err(lang
            .pick(
                "la carpeta de la spec tiene caracteres no permitidos",
                "the spec folder has characters that are not allowed",
            )
            .to_string()
            + &format!(": {d}"));
    }
    let task = match app.store.get_task(key).await {
        Ok(Some(t)) => t,
        Ok(None) => return Err("the task does not exist".into()),
        Err(e) => return Err(e.to_string()),
    };
    let not_phased = || {
        lang.pick(
            "el proyecto de la tarea ya no tiene un flujo por fases",
            "the task's project no longer has a phased workflow",
        )
        .to_string()
    };
    let Some(project) = task.project.as_deref() else {
        return Err(not_phased());
    };
    let workflow = match app.store.project(project).await {
        Ok(p) => p.settings.workflow(),
        Err(_) => return Err(not_phased()),
    };
    if !matches!(workflow, ProjectWorkflow::Phased { .. }) {
        return Err(not_phased());
    }
    let title = task.title.trim();
    let vars = Vars {
        ticket: key.as_str(),
        title,
        task: if crate::branching::is_free(key) {
            title
        } else {
            key.as_str()
        },
        spec_dir: Some(dir.as_deref().unwrap_or("")),
    };
    let prompt = match (step_prompt(step, &workflow, &vars), &dir) {
        (Some(prompt), Some(_)) => prompt,
        (Some(prompt), None) if step == Step::Review => prompt,
        _ => {
            return Err(lang
                .pick(
                    "no encuentro la carpeta de la spec",
                    "cannot find the spec folder",
                )
                .into());
        }
    };
    let old = primary_session(app, key).await;
    let new = SessionRecord {
        id: uuid::Uuid::new_v4().to_string(),
        task: key.to_string(),
        kind: SessionKind::Claude,
        label: step_label(lang, step).into(),
        primary: true,
        closed: false,
        read_only: false,
        conversation: false,
        step: Some(step),
    };
    let from = old.as_ref().map(|s| s.id.as_str()).unwrap_or("");
    let new = app
        .store
        .hand_over(key.as_str(), from, &new)
        .await
        .map_err(|e| e.to_string())?;
    // A retry that finds its session already running has nothing more to do.
    if app.sessions.get(&new.id).is_some() {
        return Ok(());
    }
    // On autopilot the session is also told how to work with nobody to answer, and
    // (the review) where to write its outcome: `new` is the session that runs.
    let prompt = match autopilot::prepare_step(app, key, step, &new.id).await {
        Some(note) => format!("{prompt} {note}"),
        None => prompt,
    };
    // The step's session was already the main one (a restart between recording it and
    // launching it): there is no other session to stop, or to go back to.
    let relaunch = old.as_ref().is_some_and(|s| s.id == new.id);
    if let Some(live) = old
        .as_ref()
        .filter(|_| !relaunch)
        .and_then(|s| app.sessions.get(&s.id))
    {
        let _ = live.runtime.kill();
        let _ = tokio::time::timeout(KILL_WAIT, live.runtime.wait()).await;
    }
    if let Err(e) = launch(app, &new, &ws.path, Some(&prompt), false).await {
        if !relaunch && let Err(undo) = app.store.undo_hand_over(key.as_str(), from, &new.id).await
        {
            tracing::warn!(task = %key, "cannot undo the handoff: {undo}");
        }
        return Err(e.message().to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::config::Config;
    use crate::store::Store;
    use delilah_core::ProjectKind;

    fn task(saving: bool, handoff: Option<(Step, HandoffStage)>) -> Task {
        let mut t = Task::new(TaskKey::new("PROJ-1"), "t", "project");
        t.saving_mode = saving;
        t.handoff = handoff.map(|(to, stage)| Handoff {
            to,
            stage,
            error: None,
        });
        t
    }

    fn spec(done: usize, total: usize) -> SpecSummary {
        SpecSummary {
            phase: delilah_core::SpecPhase::Specify,
            dir: Some("specs/proj-1-x".into()),
            done,
            total,
            current: None,
        }
    }

    #[test]
    fn plan_with_unstarted_tasks_offers_the_implementation() {
        let t = task(true, None);
        assert_eq!(
            boundary(&t, Some(Step::Plan), &spec(0, 5)),
            Some(Boundary::Offer(Step::Implement))
        );
        assert_eq!(boundary(&t, Some(Step::Plan), &spec(0, 0)), None);
        assert_eq!(boundary(&t, Some(Step::Plan), &spec(2, 5)), None);
    }

    #[test]
    fn nothing_without_saving_mode_a_spec_dir_or_a_free_slot() {
        let plan = Some(Step::Plan);
        assert_eq!(boundary(&task(false, None), plan, &spec(0, 5)), None);
        let mut no_dir = spec(0, 5);
        no_dir.dir = None;
        assert_eq!(boundary(&task(true, None), plan, &no_dir), None);
        for stage in [
            HandoffStage::Saving,
            HandoffStage::Starting,
            HandoffStage::Failed,
            HandoffStage::Offered,
        ] {
            let t = task(true, Some((Step::Implement, stage)));
            assert_eq!(boundary(&t, plan, &spec(0, 5)), None, "{stage:?}");
        }
        // The user was seen implementing in the plan session: no offer.
        let skipped = task(true, Some((Step::Implement, HandoffStage::Skipped)));
        assert_eq!(boundary(&skipped, plan, &spec(0, 5)), None);
        // A session of no step, or the review one, has no boundary.
        assert_eq!(boundary(&task(true, None), None, &spec(0, 5)), None);
        assert_eq!(
            boundary(&task(true, None), Some(Step::Review), &spec(5, 5)),
            None
        );
    }

    #[test]
    fn a_fully_checked_spec_starts_the_review_from_the_furthest_step() {
        let t = task(true, None);
        for step in [Step::Plan, Step::Implement] {
            assert_eq!(
                boundary(&t, Some(step), &spec(5, 5)),
                Some(Boundary::Start(Step::Review))
            );
        }
        assert_eq!(boundary(&t, Some(Step::Implement), &spec(3, 5)), None);
        // An offer still pending, and then everything checked.
        let offered = task(true, Some((Step::Implement, HandoffStage::Offered)));
        assert_eq!(
            boundary(&offered, Some(Step::Plan), &spec(5, 5)),
            Some(Boundary::Start(Step::Review))
        );
        // Skipping the implementation does not skip the review.
        let skipped = task(true, Some((Step::Implement, HandoffStage::Skipped)));
        assert_eq!(
            boundary(&skipped, Some(Step::Plan), &spec(5, 5)),
            Some(Boundary::Start(Step::Review))
        );
    }

    /// A task in saving mode with its plan session and a workspace whose spec is
    /// written (nothing checked yet).
    async fn setup() -> (AppState, TaskKey) {
        let config = Config {
            home: std::env::temp_dir().join(format!("delilah-handoff-{}", uuid::Uuid::new_v4())),
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            agent_program: None,
            ..crate::config::Config::test_defaults()
        };
        let app = AppState::new(Store::in_memory().await.unwrap(), config);
        app.lang.set(crate::lang::Lang::Es); // Spanish fixtures
        let key = TaskKey::new("PROJ-1");
        app.store
            .create_project(&delilah_core::Project {
                name: "project".into(),
                path: app.config.home.join("ws"),
                kind: ProjectKind::Repo,
                workflow: "project".into(),
                settings: delilah_core::ProjectSettings {
                    workflow: Some(crate::lang::feature_preset(Lang::Es)),
                    ..Default::default()
                },
                preview_command: None,
            })
            .await
            .unwrap();
        let mut t = Task::new(key.clone(), "t", "project");
        t.project = Some("project".into());
        t.saving_mode = true;
        app.store.create_task(&t).await.unwrap();
        let path = app.config.home.join("ws");
        std::fs::create_dir_all(path.join("specs/proj-1-x")).unwrap();
        std::fs::write(
            path.join("specs/proj-1-x/tasks.md"),
            "- [ ] T001 One\n- [ ] T002 Two\n",
        )
        .unwrap();
        let ws = Workspace {
            path: path.clone(),
            branch: "feature/PROJ-1".into(),
            kind: ProjectKind::Repo,
            submodules: vec![],
        };
        app.store.save_workspace(&key, &path, &ws).await.unwrap();
        app.store
            .create_session(&SessionRecord {
                id: "plan-1".into(),
                task: "PROJ-1".into(),
                kind: SessionKind::Claude,
                label: "Spec y plan".into(),
                primary: true,
                closed: false,
                read_only: false,
                conversation: false,
                step: Some(Step::Plan),
            })
            .await
            .unwrap();
        (app, key)
    }

    /// `step_prompt` for the `/feature` preset.
    fn preset_prompt(lang: Lang, step: Step, dir: &str) -> Option<String> {
        step_prompt(
            step,
            &crate::lang::feature_preset(lang),
            &Vars {
                ticket: "PROJ-1",
                title: "t",
                task: "PROJ-1",
                spec_dir: Some(dir),
            },
        )
    }

    async fn set_workflow(app: &AppState, workflow: Option<ProjectWorkflow>) {
        let mut settings = app.store.project("project").await.unwrap().settings;
        settings.workflow = workflow;
        app.store
            .set_project_settings("project", &settings)
            .await
            .unwrap();
    }

    #[test]
    fn step_prompt_renders_the_projects_own_commands() {
        let w = ProjectWorkflow::Phased {
            plan: "/plan {task}".into(),
            implement: "/build {spec_dir} for {ticket}".into(),
            review: "/check {title}".into(),
        };
        let vars = Vars {
            ticket: "PROJ-1",
            title: "Fix it",
            task: "Fix it",
            spec_dir: Some("specs/x"),
        };
        assert_eq!(
            step_prompt(Step::Implement, &w, &vars).unwrap(),
            "/build specs/x for PROJ-1"
        );
        assert_eq!(
            step_prompt(Step::Review, &w, &vars).unwrap(),
            "/check Fix it"
        );
        assert!(step_prompt(Step::Plan, &w, &vars).is_none());
        assert!(step_prompt(Step::Review, &ProjectWorkflow::default(), &vars).is_none());
    }

    #[tokio::test]
    async fn a_custom_workflow_hands_over_with_its_own_implement_command() {
        let (app, key) = setup().await;
        set_workflow(
            &app,
            Some(ProjectWorkflow::Phased {
                plan: "/plan".into(),
                implement: "/build {spec_dir}".into(),
                review: "/check".into(),
            }),
        )
        .await;
        let launched = Launched::default();
        try_start(&app, &key, Step::Implement, &ok_launcher(&launched))
            .await
            .unwrap();
        let sent = launched.lock().unwrap()[0].1.clone().unwrap();
        assert_eq!(sent, "/build specs/proj-1-x");
    }

    #[tokio::test]
    async fn try_start_fails_when_the_project_is_no_longer_phased() {
        let (app, key) = setup().await;
        set_workflow(&app, None).await;
        let launched = Launched::default();
        let err = try_start(&app, &key, Step::Implement, &ok_launcher(&launched))
            .await
            .unwrap_err();
        assert_eq!(
            err,
            "el proyecto de la tarea ya no tiene un flujo por fases"
        );
        assert!(launched.lock().unwrap().is_empty());
        app.lang.set(Lang::En);
        let err = try_start(&app, &key, Step::Review, &ok_launcher(&launched))
            .await
            .unwrap_err();
        assert!(err.contains("no longer has a phased workflow"), "{err}");
    }

    type Launched = Mutex<Vec<(SessionRecord, Option<String>)>>;

    fn ok_launcher(launched: &Launched) -> impl Launcher + '_ {
        move |_: &AppState,
              s: &SessionRecord,
              _: &Path,
              prompt: Option<&str>,
              _|
              -> LaunchFuture<'_> {
            launched
                .lock()
                .unwrap()
                .push((s.clone(), prompt.map(str::to_string)));
            Box::pin(std::future::ready(Ok(())))
        }
    }

    fn failing_launcher() -> impl Launcher {
        |_: &AppState, _: &SessionRecord, _: &Path, _: Option<&str>, _| -> LaunchFuture<'_> {
            Box::pin(std::future::ready(Err(ApiError::internal("no claude"))))
        }
    }

    async fn handoff_of(app: &AppState, key: &TaskKey) -> Option<Handoff> {
        app.store.get_task(key).await.unwrap().unwrap().handoff
    }

    #[tokio::test]
    async fn the_end_of_the_plan_turn_offers_the_implementation() {
        let (app, key) = setup().await;
        let mut rx = app.events.subscribe();
        on_turn_ended(&app, &key).await;
        let h = handoff_of(&app, &key).await.unwrap();
        assert_eq!((h.to, h.stage), (Step::Implement, HandoffStage::Offered));
        let mut topics = vec![];
        while let Ok(t) = rx.try_recv() {
            topics.push(t);
        }
        assert!(topics.contains(&Topic::Inbox), "{topics:?}");
        // Once offered, a later turn end changes nothing.
        on_turn_ended(&app, &key).await;
        assert_eq!(handoff_of(&app, &key).await.unwrap(), h);
    }

    async fn no_save_notes(app: &AppState) {
        app.store
            .set_setting("pref:save-notes", &serde_json::json!(false))
            .await
            .unwrap();
    }

    /// A live outgoing primary (a process that does nothing) and an offered handoff.
    async fn offered_with_live_primary(app: &AppState, key: &TaskKey) {
        let mut cmd = portable_pty::CommandBuilder::new("sleep");
        cmd.arg("30");
        let pty = delilah_agents::pty::PtySession::spawn(cmd, 80, 24, None).unwrap();
        app.sessions
            .insert("plan-1", "PROJ-1", SessionKind::Claude, true, pty);
        on_turn_ended(app, key).await;
    }

    #[tokio::test]
    async fn confirm_saves_the_notes_first_and_the_save_turn_end_starts_the_session() {
        let (app, key) = setup().await;
        offered_with_live_primary(&app, &key).await;
        let launched = Launched::default();
        confirm_with(&app, &key, &ok_launcher(&launched))
            .await
            .ok()
            .unwrap();
        let h = handoff_of(&app, &key).await.unwrap();
        assert_eq!((h.to, h.stage), (Step::Implement, HandoffStage::Saving));
        assert!(launched.lock().unwrap().is_empty());
        assert_eq!(
            app.sessions.get("plan-1").unwrap().state(),
            crate::sessions::LiveState::Working
        );

        on_turn_ended_with(&app, &key, &ok_launcher(&launched)).await;
        let launched = launched.lock().unwrap().clone();
        assert_eq!(launched.len(), 1);
        assert_eq!(launched[0].0.step, Some(Step::Implement));
        assert!(handoff_of(&app, &key).await.is_none());
    }

    #[tokio::test]
    async fn a_failed_save_turn_fails_the_handoff_and_a_retry_starts_it() {
        let (app, key) = setup().await;
        offered_with_live_primary(&app, &key).await;
        let launched = Launched::default();
        confirm_with(&app, &key, &ok_launcher(&launched))
            .await
            .ok()
            .unwrap();
        on_turn_failed(&app, &key).await;
        let h = handoff_of(&app, &key).await.unwrap();
        assert_eq!((h.to, h.stage), (Step::Implement, HandoffStage::Failed));
        assert!(h.error.is_some());
        assert!(launched.lock().unwrap().is_empty());

        retry_with(&app, &key, &ok_launcher(&launched))
            .await
            .ok()
            .unwrap();
        assert_eq!(launched.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_turn_failure_outside_saving_changes_nothing() {
        let (app, key) = setup().await;
        on_turn_ended(&app, &key).await;
        let before = handoff_of(&app, &key).await;
        on_turn_failed(&app, &key).await;
        assert_eq!(handoff_of(&app, &key).await, before);
    }

    #[tokio::test]
    async fn without_save_notes_the_session_starts_straight_away() {
        let (app, key) = setup().await;
        no_save_notes(&app).await;
        offered_with_live_primary(&app, &key).await;
        let launched = Launched::default();
        confirm_with(&app, &key, &ok_launcher(&launched))
            .await
            .ok()
            .unwrap();
        assert_eq!(launched.lock().unwrap().len(), 1);
        assert!(handoff_of(&app, &key).await.is_none());
    }

    #[tokio::test]
    async fn the_automatic_review_handoff_saves_the_notes_too() {
        let (app, key) = setup().await;
        let mut cmd = portable_pty::CommandBuilder::new("sleep");
        cmd.arg("30");
        let pty = delilah_agents::pty::PtySession::spawn(cmd, 80, 24, None).unwrap();
        app.sessions
            .insert("plan-1", "PROJ-1", SessionKind::Claude, true, pty);
        let ws = app.config.home.join("ws");
        std::fs::write(
            ws.join("specs/proj-1-x/tasks.md"),
            "- [x] T001 One\n- [x] T002 Two\n",
        )
        .unwrap();
        let launched = Launched::default();
        on_turn_ended_with(&app, &key, &ok_launcher(&launched)).await;
        let h = handoff_of(&app, &key).await.unwrap();
        assert_eq!((h.to, h.stage), (Step::Review, HandoffStage::Saving));
        assert!(launched.lock().unwrap().is_empty());
        on_turn_ended_with(&app, &key, &ok_launcher(&launched)).await;
        assert_eq!(launched.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn confirm_starts_the_implementation_in_a_new_primary_session() {
        let (app, key) = setup().await;
        // The outgoing session is really running, so killing it can be seen.
        let mut cmd = portable_pty::CommandBuilder::new("sleep");
        cmd.arg("30");
        let pty = delilah_agents::pty::PtySession::spawn(cmd, 80, 24, None).unwrap();
        app.sessions
            .insert("plan-1", "PROJ-1", SessionKind::Claude, true, pty.clone());
        no_save_notes(&app).await;
        on_turn_ended(&app, &key).await;

        let launched = Launched::default();
        confirm_with(&app, &key, &ok_launcher(&launched))
            .await
            .ok()
            .unwrap();

        let launched = launched.lock().unwrap().clone();
        assert_eq!(launched.len(), 1);
        let (session, prompt) = &launched[0];
        assert_eq!(session.step, Some(Step::Implement));
        assert_eq!(session.label, "Implementación");
        assert_eq!(prompt.as_deref(), Some("/orchestrate specs/proj-1-x"));
        tokio::time::timeout(Duration::from_secs(5), pty.wait())
            .await
            .expect("the old session was killed");

        assert!(handoff_of(&app, &key).await.is_none());
        let open = app.store.open_sessions(Some("PROJ-1")).await.unwrap();
        assert_eq!(open.len(), 2, "the old one stays listed");
        let primaries: Vec<_> = open.iter().filter(|s| s.primary).collect();
        assert_eq!(primaries.len(), 1);
        assert_eq!(primaries[0].id, session.id);
    }

    #[tokio::test]
    async fn the_prompts_follow_the_language() {
        assert_eq!(
            preset_prompt(Lang::Es, Step::Implement, "specs/x").unwrap(),
            "/orchestrate specs/x"
        );
        let es = preset_prompt(Lang::Es, Step::Review, "").unwrap();
        let en = preset_prompt(Lang::En, Step::Review, "").unwrap();
        assert!(es.starts_with("/review") && en.starts_with("/review"));
        assert!(es.contains("No hagas push") && en.contains("Do not push"));
        assert!(plan_note(Lang::Es).contains("Delilah seguirá en una sesión nueva."));
        assert!(plan_note(Lang::En).contains("tasks.md"));
    }

    #[tokio::test]
    async fn confirming_twice_starts_one_session() {
        let (app, key) = setup().await;
        on_turn_ended(&app, &key).await;
        let launched = Launched::default();
        let launcher = ok_launcher(&launched);
        confirm_with(&app, &key, &launcher).await.ok().unwrap();
        let err = confirm_with(&app, &key, &launcher).await.unwrap_err();
        assert_eq!(err.status, axum::http::StatusCode::CONFLICT);
        assert_eq!(launched.lock().unwrap().len(), 1);
        // Nor does a task with no offer confirm, decline or retry.
        assert!(decline(&app, &key).await.is_err());
        assert!(retry(&app, &key).await.is_err());
        let missing = TaskKey::new("NOPE-1");
        assert_eq!(
            confirm(&app, &missing).await.unwrap_err().status,
            axum::http::StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn declining_keeps_the_session_and_ends_saving_mode() {
        let (app, key) = setup().await;
        on_turn_ended(&app, &key).await;
        decline(&app, &key).await.ok().unwrap();
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        assert!(task.handoff.is_none() && !task.saving_mode);
        let open = app.store.open_sessions(Some("PROJ-1")).await.unwrap();
        assert_eq!(open.len(), 1);
        assert!(open[0].primary);
    }

    #[tokio::test]
    async fn a_launch_failure_restores_the_old_session_and_can_be_retried() {
        let (app, key) = setup().await;
        on_turn_ended(&app, &key).await;
        let failing = failing_launcher();
        confirm_with(&app, &key, &failing).await.ok().unwrap();

        let h = handoff_of(&app, &key).await.unwrap();
        assert_eq!((h.to, h.stage), (Step::Implement, HandoffStage::Failed));
        assert_eq!(h.error.as_deref(), Some("no claude"));
        let open = app.store.open_sessions(Some("PROJ-1")).await.unwrap();
        assert_eq!(open.len(), 1, "the new one is closed");
        assert!(open[0].primary && open[0].id == "plan-1");
        let events = app.store.events(&key).await.unwrap();
        assert!(!events.iter().any(|e| e.event.name() == "step-started"));

        let launched = Launched::default();
        retry_with(&app, &key, &ok_launcher(&launched))
            .await
            .ok()
            .unwrap();
        assert_eq!(launched.lock().unwrap().len(), 1);
        assert!(handoff_of(&app, &key).await.is_none());
        let open = app.store.open_sessions(Some("PROJ-1")).await.unwrap();
        assert_eq!(open.iter().filter(|s| s.primary).count(), 1);
        assert!(
            open.iter()
                .any(|s| s.step == Some(Step::Implement) && s.primary)
        );
    }

    #[tokio::test]
    async fn a_fully_checked_spec_starts_the_review_by_itself() {
        let (app, key) = setup().await;
        let ws = app.config.home.join("ws");
        std::fs::write(
            ws.join("specs/proj-1-x/tasks.md"),
            "- [x] T001 One\n- [x] T002 Two\n",
        )
        .unwrap();
        // The real launcher cannot be used here: check the decision and the stage.
        let launched = Launched::default();
        let found = {
            let task = app.store.get_task(&key).await.unwrap().unwrap();
            let spec = read_spec(&app, &key).await.unwrap();
            boundary(&task, Some(Step::Plan), &spec)
        };
        assert_eq!(found, Some(Boundary::Start(Step::Review)));
        set(&app, &key, Step::Review, HandoffStage::Starting, None).await;
        start_with(&app, &key, Step::Review, &ok_launcher(&launched)).await;
        let launched = launched.lock().unwrap();
        assert_eq!(launched[0].0.step, Some(Step::Review));
        assert!(launched[0].1.as_deref().unwrap().starts_with("/review"));
    }

    async fn put_handoff(app: &AppState, key: &TaskKey, to: Step, stage: HandoffStage) {
        let h = Handoff {
            to,
            stage,
            error: None,
        };
        app.store.set_handoff(key, Some(&h)).await.unwrap();
    }

    #[tokio::test]
    async fn recovery_starts_a_starting_handoff_once() {
        let (app, key) = setup().await;
        put_handoff(&app, &key, Step::Implement, HandoffStage::Starting).await;
        let launched = Launched::default();
        let launcher = ok_launcher(&launched);
        let stale = app.store.get_task(&key).await.unwrap().unwrap();
        recover_with(&app, &stale, &launcher).await;
        recover_with(&app, &stale, &launcher).await;
        assert_eq!(launched.lock().unwrap().len(), 1);
        assert!(handoff_of(&app, &key).await.is_none());
    }

    /// The implementation session as `hand_over` leaves it: recorded and primary.
    async fn hand_over_implement(app: &AppState, key: &TaskKey) -> SessionRecord {
        let new = SessionRecord {
            id: "impl-1".into(),
            task: "PROJ-1".into(),
            kind: SessionKind::Claude,
            label: "Implementación".into(),
            primary: true,
            closed: false,
            read_only: false,
            conversation: false,
            step: Some(Step::Implement),
        };
        app.store
            .hand_over(key.as_str(), "plan-1", &new)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn recovery_only_clears_a_handoff_whose_session_is_running() {
        let (app, key) = setup().await;
        hand_over_implement(&app, &key).await;
        let mut cmd = portable_pty::CommandBuilder::new("sleep");
        cmd.arg("30");
        let pty = delilah_agents::pty::PtySession::spawn(cmd, 80, 24, None).unwrap();
        app.sessions
            .insert("impl-1", "PROJ-1", SessionKind::Claude, true, pty.clone());
        put_handoff(&app, &key, Step::Implement, HandoffStage::Starting).await;
        let launched = Launched::default();
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        recover_with(&app, &task, &ok_launcher(&launched)).await;
        assert!(launched.lock().unwrap().is_empty());
        assert!(handoff_of(&app, &key).await.is_none());
        let _ = pty.kill();
    }

    #[tokio::test]
    async fn recovery_launches_a_session_that_was_never_launched() {
        let (app, key) = setup().await;
        hand_over_implement(&app, &key).await;
        put_handoff(&app, &key, Step::Implement, HandoffStage::Starting).await;
        let launched = Launched::default();
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        recover_with(&app, &task, &ok_launcher(&launched)).await;
        {
            let launched = launched.lock().unwrap();
            assert_eq!(launched.len(), 1);
            assert_eq!(launched[0].0.id, "impl-1");
            assert_eq!(
                launched[0].1.as_deref(),
                Some("/orchestrate specs/proj-1-x")
            );
        }
        assert!(handoff_of(&app, &key).await.is_none());
        let open = app.store.open_sessions(Some("PROJ-1")).await.unwrap();
        assert_eq!(open.iter().filter(|s| s.primary).count(), 1);

        // If that launch fails the handoff is failed, and the session stays main.
        put_handoff(&app, &key, Step::Implement, HandoffStage::Starting).await;
        let failing = failing_launcher();
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        recover_with(&app, &task, &failing).await;
        let h = handoff_of(&app, &key).await.unwrap();
        assert_eq!(h.stage, HandoffStage::Failed);
        let open = app.store.open_sessions(Some("PROJ-1")).await.unwrap();
        assert_eq!(open.iter().filter(|s| s.primary).count(), 1);
    }

    #[tokio::test]
    async fn recovery_leaves_a_start_in_flight_alone() {
        let (app, key) = setup().await;
        put_handoff(&app, &key, Step::Implement, HandoffStage::Starting).await;
        assert!(app.handoffs.mark_starting(&key));
        let launched = Launched::default();
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        recover_with(&app, &task, &ok_launcher(&launched)).await;
        assert!(launched.lock().unwrap().is_empty());
        assert_eq!(
            handoff_of(&app, &key).await.unwrap().stage,
            HandoffStage::Starting
        );
    }

    #[tokio::test]
    async fn turning_saving_mode_off_during_a_start_is_not_overwritten() {
        let (app, key) = setup().await;
        on_turn_ended(&app, &key).await;
        // A start is in flight (the stage is recorded, the launch is under way)...
        let h = app
            .store
            .get_task(&key)
            .await
            .unwrap()
            .unwrap()
            .handoff
            .unwrap();
        assert_eq!(h.stage, HandoffStage::Offered);
        assert!(app.handoffs.mark_starting(&key));
        set(&app, &key, h.to, HandoffStage::Starting, None).await;
        // ...and saving mode is turned off before it ends, then the launch fails.
        apply_saving_mode(&app, &key, false).await.ok().unwrap();
        let failing = failing_launcher();
        start_with(&app, &key, h.to, &failing).await;
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        assert!(!task.saving_mode);
        assert!(task.handoff.is_none(), "{:?}", task.handoff);
    }

    #[tokio::test]
    async fn a_spec_folder_with_odd_characters_fails_the_handoff() {
        let (app, key) = setup().await;
        let ws = app.config.home.join("ws");
        std::fs::rename(ws.join("specs/proj-1-x"), ws.join("specs/proj-1-x;y")).unwrap();
        on_turn_ended(&app, &key).await;
        let launched = Launched::default();
        confirm_with(&app, &key, &ok_launcher(&launched))
            .await
            .ok()
            .unwrap();
        assert!(launched.lock().unwrap().is_empty());
        let h = handoff_of(&app, &key).await.unwrap();
        assert_eq!(h.stage, HandoffStage::Failed);
        assert!(h.error.unwrap().contains("proj-1-x;y"));
        assert!(safe_dir("specs/004-saving-mode") && !safe_dir("../x") && !safe_dir(""));
    }

    #[tokio::test]
    async fn recovery_skips_a_save_whose_session_is_gone() {
        let (app, key) = setup().await;
        let launched = Launched::default();
        put_handoff(&app, &key, Step::Implement, HandoffStage::Saving).await;
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        recover_with(&app, &task, &ok_launcher(&launched)).await;
        let h = handoff_of(&app, &key).await.unwrap();
        assert_eq!((h.to, h.stage), (Step::Implement, HandoffStage::Offered));
        assert!(launched.lock().unwrap().is_empty());

        put_handoff(&app, &key, Step::Review, HandoffStage::Saving).await;
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        recover_with(&app, &task, &ok_launcher(&launched)).await;
        assert_eq!(launched.lock().unwrap().len(), 1);
        assert_eq!(launched.lock().unwrap()[0].0.step, Some(Step::Review));
        assert!(handoff_of(&app, &key).await.is_none());
    }

    #[tokio::test]
    async fn a_code_edit_in_the_plan_session_skips_the_offer() {
        let (app, key) = setup().await;
        on_code_edit(&app, &key).await;
        let h = handoff_of(&app, &key).await.unwrap();
        assert_eq!((h.to, h.stage), (Step::Implement, HandoffStage::Skipped));
    }

    #[tokio::test]
    async fn choosing_implement_offers_only_from_the_plan_session() {
        let (app, key) = setup().await;
        on_implement_chosen(&app, &key).await;
        let h = handoff_of(&app, &key).await.unwrap();
        assert_eq!((h.to, h.stage), (Step::Implement, HandoffStage::Offered));
        // A task without saving mode is left alone.
        app.store.set_saving_mode(&key, false).await.unwrap();
        app.store.set_handoff(&key, None).await.unwrap();
        on_implement_chosen(&app, &key).await;
        assert!(handoff_of(&app, &key).await.is_none());
    }

    fn check_all(app: &AppState) {
        let ws = app.config.home.join("ws");
        std::fs::write(
            ws.join("specs/proj-1-x/tasks.md"),
            "- [x] T001 One\n- [x] T002 Two\n",
        )
        .unwrap();
    }

    #[tokio::test]
    async fn the_last_checked_task_starts_the_review_without_an_offer_and_announces_it() {
        let (app, key) = setup().await;
        no_save_notes(&app).await;
        check_all(&app);
        let launched = Launched::default();
        let mut rx = app.events.subscribe();
        on_turn_ended_with(&app, &key, &ok_launcher(&launched)).await;
        {
            let launched = launched.lock().unwrap();
            assert_eq!(launched.len(), 1);
            assert_eq!(launched[0].0.step, Some(Step::Review));
            assert_eq!(launched[0].0.label, "Revisión");
            assert!(launched[0].0.primary);
            let prompt = launched[0].1.as_deref().unwrap();
            assert!(prompt.starts_with("/review"));
            assert!(prompt.contains("hallazgos confirmados"));
            assert!(prompt.contains("documentación") && prompt.contains("commit"));
            assert!(prompt.contains("No hagas push ni abras una PR"));
        }
        assert!(handoff_of(&app, &key).await.is_none());
        // The app is listening (`rx`) and notifies itself: the daemon stays quiet.
        assert!(app.notifier.recorded().is_empty());
        let mut topics = vec![];
        while let Ok(t) = rx.try_recv() {
            topics.push(t);
        }
        assert!(topics.contains(&Topic::Inbox), "{topics:?}");
    }

    #[tokio::test]
    async fn the_automatic_review_is_announced_when_nobody_listens() {
        let (app, key) = setup().await;
        no_save_notes(&app).await;
        check_all(&app);
        let launched = Launched::default();
        on_turn_ended_with(&app, &key, &ok_launcher(&launched)).await;
        assert_eq!(launched.lock().unwrap().len(), 1);
        let sent = app.notifier.recorded();
        assert_eq!(sent.len(), 1);
        assert!(sent[0].title.starts_with("PROJ-1"), "{:?}", sent[0]);
        assert!(sent[0].body.contains("Revisión"));
    }

    #[tokio::test]
    async fn the_automatic_review_waits_for_the_notes_then_launches_once() {
        let (app, key) = setup().await;
        let cmd = portable_pty::CommandBuilder::new("cat");
        let pty = delilah_agents::pty::PtySession::spawn(cmd, 80, 24, None).unwrap();
        app.sessions
            .insert("plan-1", "PROJ-1", SessionKind::Claude, true, pty.clone());
        check_all(&app);
        let launched = Launched::default();
        let launcher = ok_launcher(&launched);
        on_turn_ended_with(&app, &key, &launcher).await;
        let h = handoff_of(&app, &key).await.unwrap();
        assert_eq!((h.to, h.stage), (Step::Review, HandoffStage::Saving));
        assert!(launched.lock().unwrap().is_empty());
        // The save turn ends: the review starts, once.
        on_turn_ended_with(&app, &key, &launcher).await;
        {
            let launched = launched.lock().unwrap();
            assert_eq!(launched.len(), 1);
            assert_eq!(launched[0].0.step, Some(Step::Review));
        }
        let _ = pty.kill();
    }

    #[tokio::test]
    async fn a_failed_save_prompt_falls_back_to_starting() {
        let (app, key) = setup().await;
        let cmd = portable_pty::CommandBuilder::new("true");
        let pty = delilah_agents::pty::PtySession::spawn(cmd, 80, 24, None).unwrap();
        pty.wait().await;
        app.sessions
            .insert("plan-1", "PROJ-1", SessionKind::Claude, true, pty.clone());
        check_all(&app);
        let launched = Launched::default();
        on_turn_ended_with(&app, &key, &ok_launcher(&launched)).await;
        let launched = launched.lock().unwrap();
        assert_eq!(launched.len(), 1);
        assert_eq!(launched[0].0.step, Some(Step::Review));
    }

    #[test]
    fn the_review_prompt_forbids_push_and_pr_in_both_languages() {
        let en = preset_prompt(Lang::En, Step::Review, "").unwrap();
        for word in [
            "review",
            "confirmed findings",
            "docs",
            "commit",
            "Do not push or open a PR",
        ] {
            assert!(en.contains(word), "{word}");
        }
        let es = preset_prompt(Lang::Es, Step::Review, "").unwrap();
        assert!(
            es.contains("hallazgos confirmados") && es.contains("No hagas push ni abras una PR")
        );
    }

    #[tokio::test]
    async fn the_automatic_review_shows_in_the_inbox_while_saving_and_starting() {
        let (app, key) = setup().await;
        let mut cmd = portable_pty::CommandBuilder::new("sleep");
        cmd.arg("30");
        let pty = delilah_agents::pty::PtySession::spawn(cmd, 80, 24, None).unwrap();
        app.sessions
            .insert("plan-1", "PROJ-1", SessionKind::Claude, true, pty.clone());
        check_all(&app);
        let launched = Launched::default();
        on_turn_ended_with(&app, &key, &ok_launcher(&launched)).await;
        let axum::Json(items) = crate::inbox::list(State(app.clone())).await.ok().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, crate::inbox::ItemKind::Handoff);
        assert_eq!(app.notifier.recorded().len(), 1);

        set(&app, &key, Step::Review, HandoffStage::Starting, None).await;
        let axum::Json(items) = crate::inbox::list(State(app.clone())).await.ok().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].text.as_deref(),
            Some("Pasando a la revisión en una sesión nueva…")
        );
        let _ = pty.kill();
    }

    /// What a process that echoes its input has received, once it shows `needle`.
    async fn typed_into(pty: &delilah_agents::pty::PtySession, needle: &str) -> String {
        for _ in 0..100 {
            let text = String::from_utf8_lossy(&pty.attach().0).to_string();
            if text.contains(needle) {
                return text;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        String::from_utf8_lossy(&pty.attach().0).to_string()
    }

    #[tokio::test]
    async fn saving_types_exactly_save_session_and_writes_the_stage_first() {
        let (app, key) = setup().await;
        let cmd = portable_pty::CommandBuilder::new("cat");
        let pty = delilah_agents::pty::PtySession::spawn(cmd, 80, 24, None).unwrap();
        app.sessions
            .insert("plan-1", "PROJ-1", SessionKind::Claude, true, pty.clone());
        on_turn_ended(&app, &key).await;
        let launched = Launched::default();
        confirm_with(&app, &key, &ok_launcher(&launched))
            .await
            .ok()
            .unwrap();
        assert_eq!(
            handoff_of(&app, &key).await.unwrap().stage,
            HandoffStage::Saving
        );
        let text = typed_into(&pty, "/save-session").await;
        assert!(
            text.lines().any(|l| l.trim() == "/save-session"),
            "{text:?}"
        );
        let _ = pty.kill();
    }

    #[tokio::test]
    async fn a_save_turn_ending_during_the_typing_still_sees_the_saving_stage() {
        let (app, key) = setup().await;
        let cmd = portable_pty::CommandBuilder::new("cat");
        let pty = delilah_agents::pty::PtySession::spawn(cmd, 80, 24, None).unwrap();
        app.sessions
            .insert("plan-1", "PROJ-1", SessionKind::Claude, true, pty.clone());
        on_turn_ended(&app, &key).await;
        let launched = Launched::default();
        let launcher = ok_launcher(&launched);
        // The turn end races the confirmation: it must wait for the guard and then
        // find `saving`, not the offer.
        let (confirmed, _) = tokio::join!(confirm_with(&app, &key, &launcher), async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            on_turn_ended_with(&app, &key, &launcher).await;
        });
        confirmed.ok().unwrap();
        assert_eq!(launched.lock().unwrap().len(), 1);
        assert_eq!(launched.lock().unwrap()[0].0.step, Some(Step::Implement));
        let _ = pty.kill();
    }

    fn running_autopilot() -> delilah_core::Autopilot {
        delilah_core::Autopilot {
            state: delilah_core::AutopilotState::Running,
            stage: None,
            reason: None,
            since: chrono::Utc::now(),
            nudged: None,
            review_session: None,
        }
    }

    async fn set_autopilot(app: &AppState, key: &TaskKey, state: delilah_core::AutopilotState) {
        let a = delilah_core::Autopilot {
            state,
            ..running_autopilot()
        };
        app.store.set_autopilot(key, Some(&a)).await.unwrap();
    }

    #[tokio::test]
    async fn on_autopilot_the_plan_turn_end_starts_the_implementation_with_no_offer() {
        let (app, key) = setup().await;
        no_save_notes(&app).await;
        set_autopilot(&app, &key, delilah_core::AutopilotState::Running).await;
        let launched = Launched::default();
        on_turn_ended_with(&app, &key, &ok_launcher(&launched)).await;
        {
            let launched = launched.lock().unwrap();
            assert_eq!(launched.len(), 1);
            assert_eq!(launched[0].0.step, Some(Step::Implement));
            let prompt = launched[0].1.as_deref().unwrap();
            assert!(prompt.starts_with("/orchestrate specs/proj-1-x"));
            let note =
                autopilot::notes::step_note(Lang::Es, Step::Implement, Path::new("")).unwrap();
            assert!(prompt.ends_with(&note), "{prompt}");
        }
        assert!(handoff_of(&app, &key).await.is_none());
        let axum::Json(items) = crate::inbox::list(State(app.clone())).await.ok().unwrap();
        assert!(
            !items
                .iter()
                .any(|i| i.kind == crate::inbox::ItemKind::Handoff),
            "{items:?}"
        );
    }

    #[tokio::test]
    async fn on_autopilot_the_handoff_never_passes_through_offered() {
        let (app, key) = setup().await;
        // Saving the notes first: the stage goes saving, then starting, never offered.
        let cmd = portable_pty::CommandBuilder::new("cat");
        let pty = delilah_agents::pty::PtySession::spawn(cmd, 80, 24, None).unwrap();
        app.sessions
            .insert("plan-1", "PROJ-1", SessionKind::Claude, true, pty.clone());
        set_autopilot(&app, &key, delilah_core::AutopilotState::Running).await;
        let launched = Launched::default();
        on_turn_ended_with(&app, &key, &ok_launcher(&launched)).await;
        let h = handoff_of(&app, &key).await.unwrap();
        assert_eq!((h.to, h.stage), (Step::Implement, HandoffStage::Saving));
        on_turn_ended_with(&app, &key, &ok_launcher(&launched)).await;
        assert_eq!(launched.lock().unwrap().len(), 1);
        let _ = pty.kill();
    }

    #[tokio::test]
    async fn a_paused_or_finished_autopilot_still_offers_the_implementation() {
        for state in [
            delilah_core::AutopilotState::Paused,
            delilah_core::AutopilotState::Done,
        ] {
            let (app, key) = setup().await;
            set_autopilot(&app, &key, state).await;
            on_turn_ended(&app, &key).await;
            let h = handoff_of(&app, &key).await.unwrap();
            assert_eq!((h.to, h.stage), (Step::Implement, HandoffStage::Offered));
        }
    }

    #[tokio::test]
    async fn on_autopilot_the_review_prompt_carries_its_outcome_path() {
        let (app, key) = setup().await;
        no_save_notes(&app).await;
        set_autopilot(&app, &key, delilah_core::AutopilotState::Running).await;
        check_all(&app);
        let launched = Launched::default();
        on_turn_ended_with(&app, &key, &ok_launcher(&launched)).await;
        let launched = launched.lock().unwrap().clone();
        assert_eq!(launched.len(), 1);
        let (session, prompt) = &launched[0];
        assert_eq!(session.step, Some(Step::Review));
        let prompt = prompt.as_deref().unwrap();
        assert!(prompt.starts_with("/review"));
        let path = autopilot::outcome_path(&app, &key, &session.id);
        assert!(prompt.contains(&path.display().to_string()), "{prompt}");
        assert!(prompt.contains("pr_title"));
        // The folder exists before the session starts, and the session is recorded.
        assert!(path.parent().unwrap().is_dir());
        let recorded = app.store.get_task(&key).await.unwrap().unwrap().autopilot;
        assert_eq!(
            recorded.and_then(|a| a.review_session).as_deref(),
            Some(session.id.as_str())
        );
    }

    #[tokio::test]
    async fn without_autopilot_the_prompts_carry_no_autopilot_note() {
        let (app, key) = setup().await;
        no_save_notes(&app).await;
        check_all(&app);
        let launched = Launched::default();
        on_turn_ended_with(&app, &key, &ok_launcher(&launched)).await;
        let launched = launched.lock().unwrap();
        assert!(!launched[0].1.as_deref().unwrap().contains("Autopilot"));
    }

    #[test]
    fn a_review_turn_end_finishes_only_on_autopilot() {
        let mut t = task(true, None);
        assert_eq!(boundary(&t, Some(Step::Review), &spec(5, 5)), None);
        t.autopilot = Some(running_autopilot());
        assert_eq!(
            boundary(&t, Some(Step::Review), &spec(5, 5)),
            Some(Boundary::Finish)
        );
    }

    #[tokio::test]
    async fn the_whole_chain_runs_through_the_turn_ends_to_one_draft_pr() {
        use crate::gh::fake::{FakeGh, PR_URL, REPO_VIEW, bare_snapshot};
        use crate::pr_testkit::git_repo_with_origin;

        let gh = Arc::new(
            FakeGh::new()
                .with_user("me")
                .on(&["repo", "view"], Ok(REPO_VIEW))
                .on(&["pr", "list"], Ok("[]"))
                .on(&["pr", "create"], Ok(PR_URL))
                .on(&["api", "graphql"], Ok(&bare_snapshot("OPEN", false))),
        );
        let (app, key) = setup().await;
        let app = app.with_gh(gh.clone());
        no_save_notes(&app).await;
        let ws = app.config.home.join("ws");
        git_repo_with_origin(
            &ws,
            &app.config.home.join("origin"),
            "feature/PROJ-1",
            &["first"],
        );
        set_autopilot(&app, &key, delilah_core::AutopilotState::Running).await;
        let launched = Launched::default();
        let launch = ok_launcher(&launched);

        // The plan turn ends with tasks.md written: the implementation starts.
        on_turn_ended_with(&app, &key, &launch).await;
        assert_eq!(
            primary_session(&app, &key).await.unwrap().step,
            Some(Step::Implement)
        );

        // Not all checked yet: a stall, answered with the one nudge; nothing more starts.
        let implement = primary_session(&app, &key).await.unwrap();
        live(&app, &implement.id);
        on_turn_ended_with(&app, &key, &launch).await;
        assert_eq!(launched.lock().unwrap().len(), 1);
        assert_eq!(
            autopilot_state(&app, &key).await.nudged,
            Some(Step::Implement)
        );

        // Every task checked and the implementation's turn ends: the review starts.
        check_all(&app);
        on_turn_ended_with(&app, &key, &launch).await;
        let review = primary_session(&app, &key).await.unwrap();
        assert_eq!(review.step, Some(Step::Review));
        let outcome = autopilot::outcome_path(&app, &key, &review.id);
        assert!(
            launched.lock().unwrap()[1]
                .1
                .as_deref()
                .unwrap()
                .contains(&outcome.display().to_string())
        );
        assert_eq!(launched.lock().unwrap().len(), 2);

        // The review writes a ready outcome and its turn ends: one draft PR, done.
        std::fs::write(
            &outcome,
            r#"{"outcome":"ready","pr_title":"feat: chain","pr_body":"body"}"#,
        )
        .unwrap();
        // The review committed everything (the spec files included).
        for args in [&["add", "."][..], &["commit", "-m", "review"][..]] {
            let out = std::process::Command::new("git")
                .args(["-c", "user.name=t", "-c", "user.email=t@t"])
                .args(["-c", "commit.gpgsign=false"])
                .args(args)
                .current_dir(&ws)
                .output()
                .unwrap();
            assert!(out.status.success());
        }
        on_turn_ended_with(&app, &key, &launch).await;
        for _ in 0..400 {
            let a = app.store.get_task(&key).await.unwrap().unwrap().autopilot;
            if a.is_some_and(|a| a.state == delilah_core::AutopilotState::Done) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        let a = app
            .store
            .get_task(&key)
            .await
            .unwrap()
            .unwrap()
            .autopilot
            .unwrap();
        assert_eq!(a.state, delilah_core::AutopilotState::Done);
        let creates: Vec<_> = gh
            .calls()
            .into_iter()
            .filter(|c| c.starts_with(&["pr".to_string(), "create".to_string()]))
            .collect();
        assert_eq!(creates.len(), 1);
        assert!(creates[0].contains(&"--draft".to_string()));
        assert!(creates[0].contains(&"feat: chain".to_string()));
        let events = app.store.events(&key).await.unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|e| e.event.name() == "autopilot-finished")
                .count(),
            1
        );
    }

    // ---- Stalls, the nudge and failure pauses (WP-09) ----

    async fn autopilot_state(app: &AppState, key: &TaskKey) -> delilah_core::Autopilot {
        app.store
            .get_task(key)
            .await
            .unwrap()
            .unwrap()
            .autopilot
            .unwrap()
    }

    async fn event_names(app: &AppState, key: &TaskKey) -> Vec<&'static str> {
        let events = app.store.events(key).await.unwrap();
        events.iter().map(|e| e.event.name()).collect()
    }

    fn live(app: &AppState, id: &str) {
        let mut cmd = portable_pty::CommandBuilder::new("sleep");
        cmd.arg("30");
        let pty = delilah_agents::pty::PtySession::spawn(cmd, 80, 24, None).unwrap();
        app.sessions
            .insert(id, "PROJ-1", SessionKind::Claude, true, pty);
    }

    /// Makes `id` the primary session, of `step`.
    async fn switch_primary(app: &AppState, key: &TaskKey, id: &str, step: Step) {
        let new = SessionRecord {
            id: id.into(),
            task: "PROJ-1".into(),
            kind: SessionKind::Claude,
            label: "s".into(),
            primary: true,
            closed: false,
            read_only: false,
            conversation: false,
            step: Some(step),
        };
        app.store
            .hand_over(key.as_str(), "plan-1", &new)
            .await
            .unwrap();
    }

    fn tasks_md(app: &AppState, content: Option<&str>) {
        let file = app.config.home.join("ws/specs/proj-1-x/tasks.md");
        match content {
            Some(c) => std::fs::write(file, c).unwrap(),
            None => std::fs::remove_file(file).unwrap(),
        }
    }

    #[tokio::test]
    async fn a_plan_turn_end_without_tasks_nudges_once_then_pauses() {
        let (app, key) = setup().await;
        app.lang.set(Lang::En);
        set_autopilot(&app, &key, delilah_core::AutopilotState::Running).await;
        tasks_md(&app, None);
        live(&app, "plan-1");

        on_turn_ended(&app, &key).await;
        let a = autopilot_state(&app, &key).await;
        assert_eq!(a.state, delilah_core::AutopilotState::Running);
        assert_eq!(a.nudged, Some(Step::Plan));
        assert_eq!(event_names(&app, &key).await, vec!["autopilot-nudged"]);
        assert_eq!(
            app.sessions.get("plan-1").unwrap().state(),
            crate::sessions::LiveState::Working,
            "the nudge was typed"
        );

        on_turn_ended(&app, &key).await;
        let a = autopilot_state(&app, &key).await;
        assert_eq!(a.state, delilah_core::AutopilotState::Paused);
        assert_eq!(a.reason.as_deref(), Some("tasks.md not written"));
        assert_eq!(app.notifier.recorded().len(), 1);
        assert_eq!(
            event_names(&app, &key)
                .await
                .iter()
                .filter(|n| **n == "autopilot-nudged")
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn the_stall_reason_and_nudge_follow_the_app_language() {
        let (app, key) = setup().await;
        set_autopilot(&app, &key, delilah_core::AutopilotState::Running).await;
        tasks_md(&app, None);
        live(&app, "plan-1");
        on_turn_ended(&app, &key).await;
        on_turn_ended(&app, &key).await;
        let a = autopilot_state(&app, &key).await;
        assert_eq!(a.reason.as_deref(), Some("tasks.md no está escrito"));
        assert!(autopilot::notes::nudge_text(Lang::Es).starts_with("Autopilot: sigue"));
        assert!(autopilot::notes::nudge_text(Lang::En).contains("carry on without asking"));
    }

    #[tokio::test]
    async fn implement_with_unchecked_tasks_nudges_then_pauses_with_the_count() {
        let (app, key) = setup().await;
        app.lang.set(Lang::En);
        switch_primary(&app, &key, "impl-1", Step::Implement).await;
        tasks_md(
            &app,
            Some("- [x] T001 One\n- [ ] T002 Two\n- [ ] T003 Three\n"),
        );
        set_autopilot(&app, &key, delilah_core::AutopilotState::Running).await;
        live(&app, "impl-1");
        on_turn_ended(&app, &key).await;
        assert_eq!(
            autopilot_state(&app, &key).await.nudged,
            Some(Step::Implement)
        );
        on_turn_ended(&app, &key).await;
        let a = autopilot_state(&app, &key).await;
        assert_eq!(a.state, delilah_core::AutopilotState::Paused);
        assert_eq!(a.reason.as_deref(), Some("2 tasks unchecked"));
    }

    #[tokio::test]
    async fn a_blocked_review_pauses_with_its_reason_and_no_nudge() {
        let (app, key) = setup().await;
        switch_primary(&app, &key, "rev-1", Step::Review).await;
        set_autopilot(&app, &key, delilah_core::AutopilotState::Running).await;
        let path = autopilot::outcome_path(&app, &key, "rev-1");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"outcome":"blocked","reason":"tests fail"}"#).unwrap();
        live(&app, "rev-1");
        on_turn_ended(&app, &key).await;
        for _ in 0..200 {
            if autopilot_state(&app, &key).await.state == delilah_core::AutopilotState::Paused {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let a = autopilot_state(&app, &key).await;
        assert_eq!(a.state, delilah_core::AutopilotState::Paused);
        assert!(a.reason.unwrap().contains("tests fail"));
        assert_eq!(a.nudged, None);
        assert!(!event_names(&app, &key).await.contains(&"autopilot-nudged"));
    }

    #[tokio::test]
    async fn reaching_a_boundary_after_a_nudge_goes_on_normally() {
        let (app, key) = setup().await;
        set_autopilot(&app, &key, delilah_core::AutopilotState::Running).await;
        tasks_md(&app, None);
        live(&app, "plan-1");
        on_turn_ended(&app, &key).await;
        assert_eq!(autopilot_state(&app, &key).await.nudged, Some(Step::Plan));
        // The nudged session wrote the tasks.
        tasks_md(&app, Some("- [ ] T001 One\n"));
        no_save_notes(&app).await;
        let launched = Launched::default();
        on_turn_ended_with(&app, &key, &ok_launcher(&launched)).await;
        assert_eq!(launched.lock().unwrap().len(), 1);
        assert_eq!(
            autopilot_state(&app, &key).await.state,
            delilah_core::AutopilotState::Running
        );
    }

    #[tokio::test]
    async fn a_stall_while_not_on_autopilot_does_nothing() {
        let (app, key) = setup().await;
        tasks_md(&app, None);
        live(&app, "plan-1");
        on_turn_ended(&app, &key).await;
        assert!(event_names(&app, &key).await.is_empty());
        assert!(app.notifier.recorded().is_empty());
    }

    #[tokio::test]
    async fn a_handoff_that_fails_to_launch_pauses_the_autopilot_with_the_error() {
        let (app, key) = setup().await;
        set_autopilot(&app, &key, delilah_core::AutopilotState::Running).await;
        no_save_notes(&app).await;
        let failing = failing_launcher();
        on_turn_ended_with(&app, &key, &failing).await;
        let h = handoff_of(&app, &key).await.unwrap();
        assert_eq!(h.stage, HandoffStage::Failed);
        let a = autopilot_state(&app, &key).await;
        assert_eq!(a.state, delilah_core::AutopilotState::Paused);
        assert_eq!(a.reason.as_deref(), Some("no claude"));
        // The step's announcement and the pause's notification.
        let sent = app.notifier.recorded();
        assert_eq!(
            sent.iter().filter(|n| n.body.contains("no claude")).count(),
            1
        );
        assert!(event_names(&app, &key).await.contains(&"autopilot-paused"));
    }

    #[tokio::test]
    async fn a_failed_turn_of_the_primary_pauses_a_running_autopilot() {
        let (app, key) = setup().await;
        app.lang.set(Lang::En);
        set_autopilot(&app, &key, delilah_core::AutopilotState::Running).await;
        on_turn_failed(&app, &key).await;
        let a = autopilot_state(&app, &key).await;
        assert_eq!(a.state, delilah_core::AutopilotState::Paused);
        assert_eq!(a.reason.as_deref(), Some("the session failed"));
    }
}
