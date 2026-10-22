//! Autopilot: the chain of a task (plan, implement, review, PR) runs by itself. It
//! rides on saving mode's handoffs (`handoff.rs`): this module only changes who
//! confirms a handoff (`shape`), what each session is told (`step_note`), and what
//! happens after the review (`finish`: read the outcome file, open the draft PRs).
//! The state lives in `tasks.autopilot`, written before each side effect, so a
//! restart repeats nothing (`recover`). See `specs/005-autopilot`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use delilah_core::{Autopilot, AutopilotStage, AutopilotState, Event, Step, Task, TaskKey};

use crate::api::ApiError;
use crate::events::Topic;
use crate::handoff::Boundary;
use crate::lang::Lang;
use crate::pr_types::PrState;
use crate::preview::PreviewState;
use crate::state::AppState;

/// Whether the task is on autopilot and not paused or done.
pub(crate) fn is_running(task: &Task) -> bool {
    matches!(&task.autopilot, Some(a) if a.state == AutopilotState::Running)
}

/// What a boundary becomes on autopilot: the offer of the implementation is taken on
/// the user's behalf. Paused, done or off, the boundary is left as it is.
pub(crate) fn shape(task: &Task, found: Option<Boundary>) -> Option<Boundary> {
    match found {
        Some(Boundary::Offer(to)) if is_running(task) => Some(Boundary::Start(to)),
        other => other,
    }
}

/// Where the review session `session` writes its outcome.
pub(crate) fn outcome_path(app: &AppState, key: &TaskKey, session: &str) -> PathBuf {
    app.config
        .task_dir(key.as_str())
        .join("autopilot")
        .join(format!("review-{session}.json"))
}

/// What the first session of an autopilot task is told on top of its prompt: run the
/// spec workflow end to end on its own, then stop once `tasks.md` is written.
pub(crate) fn autopilot_plan_note(lang: Lang, key: &str) -> String {
    match lang {
        Lang::Es => format!(
            "Autopilot ({key}): nadie va a responder ni aprobar nada. Contesta tú las preguntas de configuración del workflow: salida en markdown, usa la rama actual de la tarea, sin fase de clarificación y con tareas generadas, solo spec y plan sin implementar. Aprueba tú cada fase y no preguntes nada que tenga una opción recomendada; si tienes que preguntar, marca una como \"(Recomendado)\". Cuando `tasks.md` esté escrito, para: Delilah seguirá en una sesión nueva."
        ),
        Lang::En => format!(
            "Autopilot ({key}): nobody will answer or approve anything. Answer the workflow's setup questions yourself: markdown output, keep the task's current branch, no clarification phase, and generate the tasks, spec and plan only with no implementation. Approve every phase yourself and do not ask anything that has a recommended option; if you must ask, mark one \"(Recommended)\". Once `tasks.md` is written, stop: Delilah will carry on in a new session."
        ),
    }
}

/// What a step's session is told on top of its prompt when the task is on autopilot.
/// `outcome` is where the review writes its outcome. The plan step has its own note
/// (`autopilot_plan_note`).
pub(crate) fn step_note(lang: Lang, step: Step, outcome: &Path) -> Option<String> {
    let path = outcome.display();
    match step {
        Step::Plan => None,
        Step::Implement => Some(
            lang.pick(
                "Autopilot: nadie va a responder ni aprobar nada. Implementa todas las tareas sin parar entre fases ni entre paquetes de trabajo y sin pedir confirmación; si una pregunta tiene una opción recomendada, elige esa. Para solo cuando todas las tareas estén marcadas.",
                "Autopilot: nobody will answer or approve anything. Implement all the tasks without stopping between phases or work packages and without asking for confirmation; if a question has a recommended option, pick it. Stop only once every task is checked.",
            )
            .to_string(),
        ),
        Step::Review => Some(match lang {
            Lang::Es => format!(
                "Autopilot: nadie va a responder ni aprobar nada. Ejecuta los checks del proyecto y arregla lo que falle, aplica los hallazgos confirmados, haz commit y escribe el título y la descripción de la PR (por ejemplo con `pr-writer`). Termina siempre con un resultado explícito: escribe en `{path}` un JSON `{{\"outcome\":\"ready\",\"pr_title\":\"...\",\"pr_body\":\"...\"}}` si la rama está lista, o `{{\"outcome\":\"blocked\",\"reason\":\"...\"}}` con el motivo si no puedes dejarla lista. No hagas push ni abras una PR: lo hace Delilah."
            ),
            Lang::En => format!(
                "Autopilot: nobody will answer or approve anything. Run the project's checks and fix what fails, apply the confirmed findings, commit, and write the PR title and description (for example with `pr-writer`). Always end with an explicit outcome: write to `{path}` a JSON `{{\"outcome\":\"ready\",\"pr_title\":\"...\",\"pr_body\":\"...\"}}` if the branch is ready, or `{{\"outcome\":\"blocked\",\"reason\":\"...\"}}` with the reason if you cannot get it ready. Do not push or open a PR: Delilah does it."
            ),
        }),
    }
}

/// What the review reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    Ready {
        pr_title: Option<String>,
        pr_body: Option<String>,
    },
    Blocked {
        reason: String,
    },
}

#[derive(serde::Deserialize)]
struct OutcomeFile {
    outcome: String,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    pr_title: Option<String>,
    #[serde(default)]
    pr_body: Option<String>,
}

/// The biggest outcome file read, and the longest title and body of a PR taken from it
/// (GitHub's body limit is 65536 characters).
const MAX_OUTCOME_BYTES: u64 = 256 * 1024;
const MAX_TITLE_CHARS: usize = 256;
const MAX_BODY_CHARS: usize = 59_000;

/// A title on one line: no control characters, cut to `MAX_TITLE_CHARS`.
fn clean_title(title: String) -> Option<String> {
    let line: String = title
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_TITLE_CHARS)
        .collect();
    non_empty(Some(line.trim().to_string()))
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|s| !s.trim().is_empty())
}

/// The outcome in `path`. A missing or invalid file is `Blocked`.
pub(crate) fn read_outcome(lang: Lang, path: &Path) -> Outcome {
    let missing = || Outcome::Blocked {
        reason: lang
            .pick(
                "la revisión no informó de un resultado",
                "the review did not report an outcome",
            )
            .to_string(),
    };
    // Written by an agent: only a regular file of a sane size is read (not a symlink
    // to somewhere else, not a device or a huge file).
    let regular = std::fs::symlink_metadata(path)
        .is_ok_and(|m| m.file_type().is_file() && m.len() <= MAX_OUTCOME_BYTES);
    let Some(file) = regular
        .then(|| std::fs::read_to_string(path).ok())
        .flatten()
        .and_then(|text| serde_json::from_str::<OutcomeFile>(&text).ok())
    else {
        return missing();
    };
    match file.outcome.as_str() {
        "ready" => Outcome::Ready {
            pr_title: non_empty(file.pr_title).and_then(clean_title),
            pr_body: non_empty(file.pr_body).map(|b| b.chars().take(MAX_BODY_CHARS).collect()),
        },
        "blocked" => Outcome::Blocked {
            reason: non_empty(file.reason).unwrap_or_else(|| {
                lang.pick(
                    "la revisión quedó bloqueada",
                    "the review reported it is blocked",
                )
                .to_string()
            }),
        },
        _ => missing(),
    }
}

async fn write_state(
    app: &AppState,
    key: &TaskKey,
    autopilot: &Autopilot,
) -> Result<(), FinishStop> {
    if let Err(e) = app.store.set_autopilot(key, Some(autopilot)).await {
        tracing::warn!(task = %key, "cannot record the autopilot: {e}");
        return Err(FinishStop::StateWrite(e.to_string()));
    }
    app.emit(Topic::Tasks);
    Ok(())
}

/// The autopilot finishes running now (one per task at most), and how many times each
/// has failed in a way worth retrying. Owned by `AppState`.
#[derive(Default)]
pub(crate) struct Finishing {
    running: Mutex<HashSet<String>>,
    retries: Mutex<HashMap<String, Retry>>,
}

struct Retry {
    attempts: u32,
    next: Instant,
}

/// How many times a finish that failed in a retryable way (gh, network) is attempted
/// in all, and the wait after the first failure (doubled after each).
const MAX_ATTEMPTS: u32 = 3;
const RETRY_BACKOFF: Duration = Duration::from_secs(60);

/// Marks a task's finish as running while it lives; dropping it (also on a panic or an
/// aborted future) lets the next one run.
pub(crate) struct FinishGuard {
    finishing: Arc<Finishing>,
    key: String,
}

impl Drop for FinishGuard {
    fn drop(&mut self) {
        let mut running = self
            .finishing
            .running
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        running.remove(&self.key);
    }
}

impl Finishing {
    /// The guard for `key`, or `None` if its finish is running already.
    pub(crate) fn try_guard(self: &Arc<Self>, key: &TaskKey) -> Option<FinishGuard> {
        let mut running = self.running.lock().unwrap_or_else(|e| e.into_inner());
        running.insert(key.to_string()).then(|| FinishGuard {
            finishing: self.clone(),
            key: key.to_string(),
        })
    }

    /// Records a retryable failure; the attempts so far.
    fn failed(&self, key: &TaskKey) -> u32 {
        let mut retries = self.retries.lock().unwrap_or_else(|e| e.into_inner());
        let r = retries.entry(key.to_string()).or_insert(Retry {
            attempts: 0,
            next: Instant::now(),
        });
        r.attempts += 1;
        r.next = Instant::now() + RETRY_BACKOFF * 2u32.pow(r.attempts - 1);
        r.attempts
    }

    /// Whether a retry may run now (no failure recorded, or its wait is over).
    fn due(&self, key: &TaskKey) -> bool {
        let retries = self.retries.lock().unwrap_or_else(|e| e.into_inner());
        retries
            .get(key.as_str())
            .is_none_or(|r| Instant::now() >= r.next)
    }

    fn forget(&self, key: &TaskKey) {
        let mut retries = self.retries.lock().unwrap_or_else(|e| e.into_inner());
        retries.remove(key.as_str());
    }
}

/// Why a finish stopped before the PRs were open.
pub(crate) enum FinishStop {
    /// The review said it is blocked (or reported nothing).
    Blocked(String),
    /// None of the task's repos has commits to open a PR with.
    NoCommits,
    NoWorkspace,
    /// `gh` or the network failed: worth trying again later.
    OpenPr(ApiError),
    /// The autopilot's state cannot be written: worth trying again later.
    StateWrite(String),
}

impl FinishStop {
    /// Whether trying again changes nothing.
    fn is_terminal(&self) -> bool {
        matches!(self, Self::Blocked(_) | Self::NoCommits | Self::NoWorkspace)
    }

    fn describe(&self) -> String {
        match self {
            Self::Blocked(reason) => format!("the review is blocked: {reason}"),
            Self::NoCommits => "there are no commits to open a PR with".into(),
            Self::NoWorkspace => "the task has no workspace".into(),
            Self::OpenPr(e) => format!("cannot open the PR: {}", e.message()),
            Self::StateWrite(e) => format!("cannot record the state: {e}"),
        }
    }
}

/// The review turn ended: if it is ready, push and open the draft PRs and finish. Only
/// one finish per task runs at a time, and a finished (or paused) autopilot does
/// nothing, so a repeated turn end opens no second PR.
pub(crate) async fn finish(app: &AppState, key: &TaskKey, session: &str) {
    if let Some(guard) = app.finishing.try_guard(key) {
        finish_guarded(app, key, session, guard).await;
    }
}

async fn finish_guarded(app: &AppState, key: &TaskKey, session: &str, _guard: FinishGuard) {
    let Err(stop) = run_finish(app, key, session).await else {
        app.finishing.forget(key);
        return;
    };
    tracing::warn!(task = %key, "autopilot stops: {}", stop.describe());
    // TODO(WP-09): pause the autopilot with this reason (`stop.describe()`).
    // Until then it only has to stop: a terminal stop, or a retryable one that keeps
    // failing, clears the stage so `recover` does not take it up again on every tick.
    if stop.is_terminal() || app.finishing.failed(key) >= MAX_ATTEMPTS {
        app.finishing.forget(key);
        clear_stage(app, key).await;
    }
}

/// Takes the stage off a running autopilot, so no recovery resumes it.
async fn clear_stage(app: &AppState, key: &TaskKey) {
    let Ok(Some(task)) = app.store.get_task(key).await else {
        return;
    };
    if let Some(mut a) = task.autopilot.filter(|a| a.stage.is_some()) {
        a.stage = None;
        let _ = write_state(app, key, &a).await;
    }
}

async fn run_finish(app: &AppState, key: &TaskKey, session: &str) -> Result<(), FinishStop> {
    let Ok(Some(task)) = app.store.get_task(key).await else {
        return Ok(());
    };
    let Some(mut autopilot) = task.autopilot.clone().filter(|_| is_running(&task)) else {
        return Ok(());
    };
    let opening = autopilot.stage == Some(AutopilotStage::OpeningPr);
    // A stage already recorded means an earlier finish was interrupted: its PR part is
    // re-run, never a new preview.
    let reopening = opening || autopilot.stage == Some(AutopilotStage::Preview);
    let (mut title, mut body) = (None, None);
    match read_outcome(app.lang(), &outcome_path(app, key, session)) {
        Outcome::Ready { pr_title, pr_body } => (title, body) = (pr_title, pr_body),
        // The file is only needed for the agent's title and description once the PRs
        // are being opened.
        Outcome::Blocked { .. } if reopening => {}
        Outcome::Blocked { reason } => return Err(FinishStop::Blocked(reason)),
    }
    autopilot.review_session = Some(session.to_string());
    if !reopening {
        preview_step(app, key, &mut autopilot).await?;
    }
    let preview_url = app
        .previews
        .view(key.as_str())
        .filter(|v| v.state == PreviewState::Running)
        .and_then(|v| v.url);
    if !opening {
        autopilot.stage = Some(AutopilotStage::OpeningPr);
        write_state(app, key, &autopilot).await?;
    }
    // Links a PR that exists already (a restart in the middle of opening, or one made
    // by hand), so none is opened twice.
    crate::pr_loop::check_task(app, key, true).await;
    open_prs(
        app,
        key,
        title.as_deref(),
        body.as_deref(),
        preview_url.as_deref(),
    )
    .await?;
    let urls = linked_urls(app, key).await;
    if urls.is_empty() {
        return Err(FinishStop::NoCommits);
    }
    autopilot.state = AutopilotState::Done;
    autopilot.stage = None;
    autopilot.since = Utc::now();
    write_state(app, key, &autopilot).await?;
    if let Err(e) = app
        .transition(key, Event::AutopilotFinished { prs: urls })
        .await
    {
        tracing::warn!(task = %key, "cannot record the autopilot's end: {e}");
    }
    Ok(())
}

/// Starts the project's preview, if it has a command, and waits until it runs or
/// fails. A preview that fails never stops the PR; only the state write can.
async fn preview_step(
    app: &AppState,
    key: &TaskKey,
    autopilot: &mut Autopilot,
) -> Result<(), FinishStop> {
    let command = match app.store.get_task(key).await {
        Ok(Some(task)) => match &task.project {
            Some(name) => app
                .store
                .project(name)
                .await
                .ok()
                .and_then(|p| p.preview_command),
            None => None,
        },
        _ => None,
    };
    if command.is_none() {
        return Ok(());
    }
    autopilot.stage = Some(AutopilotStage::Preview);
    write_state(app, key, autopilot).await?;
    match crate::preview::start_for(app, key).await {
        Ok(_) => {}
        // Already up (the user started it): use it.
        Err(e) if e.reason.as_deref() == Some("running") => {}
        Err(e) => {
            tracing::warn!(task = %key, "autopilot cannot start the preview: {}", e.message());
            return Ok(());
        }
    }
    app.previews
        .wait_settled(key.as_str(), app.previews.settle_timeout())
        .await;
    Ok(())
}

fn is_open(row: &crate::store_prs::PrRow) -> bool {
    row.snapshot
        .as_ref()
        .is_none_or(|s| s.state == PrState::Open)
}

/// The URLs of the task's open PRs.
async fn linked_urls(app: &AppState, key: &TaskKey) -> Vec<String> {
    let rows = app.store.prs_of(key).await.unwrap_or_default();
    rows.iter()
        .filter(|r| is_open(r))
        .map(|r| r.url.clone())
        .collect()
}

/// A preview URL as it goes in a PR body: no credentials, query or fragment. Loopback
/// hosts stay (the preview of this very repo is on localhost).
fn public_url(url: &str) -> String {
    let (scheme, rest) = url.split_once("://").unwrap_or(("", url));
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let path = tail.split(['?', '#']).next().unwrap_or("");
    match scheme {
        "" => format!("{host}{path}"),
        _ => format!("{scheme}://{host}{path}"),
    }
}

/// Opens a draft PR in each repo that has commits and no open PR.
async fn open_prs(
    app: &AppState,
    key: &TaskKey,
    title: Option<&str>,
    body: Option<&str>,
    preview_url: Option<&str>,
) -> Result<(), FinishStop> {
    let ws = match app.store.workspace(key).await {
        Ok(Some((_, ws))) => ws,
        Ok(None) => return Err(FinishStop::NoWorkspace),
        Err(e) => return Err(FinishStop::OpenPr(e.into())),
    };
    let rows = app.store.prs_of(key).await.unwrap_or_default();
    for (repo, _) in ws.repos() {
        if rows.iter().any(|r| r.repo == repo && is_open(r)) {
            continue;
        }
        // The proposal says whether the repo has commits to open a PR with, and is
        // what the PR says when the agent wrote no title or description.
        let (proposed_title, proposed_body) =
            match crate::pr_api::proposal_for(app, key, Some(&repo)).await {
                Ok(p) => p,
                Err(e) if matches!(e.reason.as_deref(), Some("no-commits" | "has-pr")) => continue,
                Err(e) => return Err(FinishStop::OpenPr(e)),
            };
        let title = title.map_or(proposed_title, str::to_string);
        let mut body = body.map_or(proposed_body, str::to_string);
        if let Some(url) = preview_url {
            body = format!("{}\n\nPreview: {}\n", body.trim_end(), public_url(url));
        }
        crate::pr_api::open_for(app, key, Some(&repo), &title, &body, true)
            .await
            .map_err(FinishStop::OpenPr)?;
    }
    Ok(())
}

/// Finishes what a restart left half done: a preview being started or PRs being
/// opened. The finish runs in its own task (it can wait minutes on a preview), so the
/// scheduler's tick goes on; the guard keeps a second tick from starting another.
pub(crate) fn recover(app: &AppState, task: &Task) {
    let Some(a) = task.autopilot.as_ref().filter(|_| is_running(task)) else {
        return;
    };
    if !matches!(
        a.stage,
        Some(AutopilotStage::OpeningPr | AutopilotStage::Preview)
    ) {
        return;
    }
    let Some(session) = a.review_session.clone() else {
        return;
    };
    if !app.finishing.due(&task.key) {
        return;
    }
    let Some(guard) = app.finishing.try_guard(&task.key) else {
        return;
    };
    let (app, key) = (app.clone(), task.key.clone());
    tokio::spawn(async move { finish_guarded(&app, &key, &session, guard).await });
}

/// What the step's session is told on top of its prompt, if the task is on autopilot.
/// For the review it also makes the folder of its outcome file and records the
/// session, so a restart reads that session's file. `session` is the one that runs.
pub(crate) async fn prepare_step(
    app: &AppState,
    key: &TaskKey,
    step: Step,
    session: &str,
) -> Option<String> {
    let Ok(Some(task)) = app.store.get_task(key).await else {
        return None;
    };
    if !is_running(&task) {
        return None;
    }
    let path = outcome_path(app, key, session);
    if step == Step::Review {
        if let Some(dir) = path.parent()
            && let Err(e) = std::fs::create_dir_all(dir)
        {
            tracing::warn!(task = %key, "cannot create {}: {e}", dir.display());
        }
        if let Some(mut a) = task.autopilot.clone() {
            a.review_session = Some(session.to_string());
            let _ = write_state(app, key, &a).await;
        }
    }
    step_note(app.lang(), step, &path)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::gh::fake::{FakeGh, PR_LIST, PR_URL, REPO_VIEW, bare_snapshot};
    use crate::pr_testkit::{TempDir, git_repo_with_origin, task_with_workspace};
    use crate::store_sessions::{SessionKind, SessionRecord};

    fn running() -> Autopilot {
        Autopilot {
            state: AutopilotState::Running,
            stage: None,
            reason: None,
            since: Utc::now(),
            nudged: None,
            review_session: None,
        }
    }

    fn task_with(autopilot: Option<AutopilotState>) -> Task {
        let mut t = Task::new(TaskKey::new("PROJ-1"), "t", "feature");
        t.saving_mode = true;
        t.autopilot = autopilot.map(|state| Autopilot { state, ..running() });
        t
    }

    #[test]
    fn shape_starts_the_implementation_only_while_running() {
        let offer = Some(Boundary::Offer(Step::Implement));
        let start = Some(Boundary::Start(Step::Implement));
        assert_eq!(
            shape(&task_with(Some(AutopilotState::Running)), offer),
            start
        );
        for state in [
            None,
            Some(AutopilotState::Paused),
            Some(AutopilotState::Done),
        ] {
            assert_eq!(shape(&task_with(state), offer), offer, "{state:?}");
        }
        let review = Some(Boundary::Start(Step::Review));
        assert_eq!(
            shape(&task_with(Some(AutopilotState::Running)), review),
            review
        );
        assert_eq!(shape(&task_with(Some(AutopilotState::Running)), None), None);
    }

    #[test]
    fn the_notes_follow_the_language() {
        let path = Path::new("/h/tasks/PROJ-1/autopilot/review-s1.json");
        assert!(step_note(Lang::En, Step::Plan, path).is_none());
        let es = step_note(Lang::Es, Step::Implement, path).unwrap();
        let en = step_note(Lang::En, Step::Implement, path).unwrap();
        assert!(es.contains("sin parar entre fases ni entre paquetes de trabajo"));
        assert!(en.contains("without stopping between phases or work packages"));
        let es = step_note(Lang::Es, Step::Review, path).unwrap();
        let en = step_note(Lang::En, Step::Review, path).unwrap();
        for (text, words) in [
            (
                &es,
                [
                    "checks",
                    "hallazgos confirmados",
                    "commit",
                    "título y la descripción",
                    "No hagas push ni abras una PR",
                ],
            ),
            (
                &en,
                [
                    "checks",
                    "confirmed findings",
                    "commit",
                    "title and description",
                    "Do not push or open a PR",
                ],
            ),
        ] {
            for word in words {
                assert!(text.contains(word), "{word}: {text}");
            }
            assert!(text.contains(&path.display().to_string()));
            assert!(text.contains("ready") && text.contains("blocked"));
        }
    }

    #[tokio::test]
    async fn the_outcome_file_lives_in_the_task_folder() {
        let (app, key, _dir) = task_with_workspace(Arc::new(FakeGh::new()), None).await;
        assert_eq!(
            outcome_path(&app, &key, "s1"),
            app.config
                .home
                .join("tasks/PROJ-1/autopilot/review-s1.json")
        );
    }

    fn file_with(content: Option<&str>) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("delilah-outcome-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("review-s1.json");
        if let Some(content) = content {
            std::fs::write(&path, content).unwrap();
        }
        (dir, path)
    }

    #[test]
    fn the_outcome_is_read_with_optional_fields() {
        let (_d, p) = file_with(Some(
            r#"{"outcome":"ready","pr_title":"feat: x","pr_body":"why"}"#,
        ));
        assert_eq!(
            read_outcome(Lang::En, &p),
            Outcome::Ready {
                pr_title: Some("feat: x".into()),
                pr_body: Some("why".into())
            }
        );
        let (_d, p) = file_with(Some(r#"{"outcome":"ready"}"#));
        assert_eq!(
            read_outcome(Lang::En, &p),
            Outcome::Ready {
                pr_title: None,
                pr_body: None
            }
        );
        let (_d, p) = file_with(Some(r#"{"outcome":"blocked","reason":"tests fail"}"#));
        assert_eq!(
            read_outcome(Lang::En, &p),
            Outcome::Blocked {
                reason: "tests fail".into()
            }
        );
    }

    #[test]
    fn a_missing_or_invalid_outcome_is_blocked_in_the_app_language() {
        for content in [None, Some("not json"), Some(r#"{"outcome":"maybe"}"#)] {
            let (_d, p) = file_with(content);
            assert_eq!(
                read_outcome(Lang::En, &p),
                Outcome::Blocked {
                    reason: "the review did not report an outcome".into()
                }
            );
            let Outcome::Blocked { reason } = read_outcome(Lang::Es, &p) else {
                panic!("not blocked");
            };
            assert!(reason.contains("la revisión no informó"));
        }
    }

    fn gh(list: &str) -> Arc<FakeGh> {
        Arc::new(
            FakeGh::new()
                .with_user("me")
                .on(&["repo", "view"], Ok(REPO_VIEW))
                .on(&["pr", "list"], Ok(list))
                .on(&["pr", "create"], Ok(PR_URL))
                .on(&["api", "graphql"], Ok(&bare_snapshot("OPEN", false))),
        )
    }

    /// A task on autopilot with a review session `rev-1`, whose branch has one commit
    /// beyond `main`.
    async fn ready_task(
        gh: Arc<FakeGh>,
        stage: Option<AutopilotStage>,
    ) -> (AppState, TaskKey, TempDir) {
        let (app, key, dir) = task_with_workspace(gh, None).await;
        git_repo_with_origin(
            &dir.path().join("ws"),
            &dir.path().join("origin"),
            "feature/PROJ-1",
            &["first"],
        );
        let a = Autopilot {
            stage,
            review_session: Some("rev-1".into()),
            ..running()
        };
        app.store.set_autopilot(&key, Some(&a)).await.unwrap();
        app.store
            .create_session(&SessionRecord {
                id: "rev-1".into(),
                task: "PROJ-1".into(),
                kind: SessionKind::Claude,
                label: "Revisión".into(),
                primary: true,
                closed: false,
                read_only: false,
                conversation: false,
                step: Some(Step::Review),
            })
            .await
            .unwrap();
        (app, key, dir)
    }

    fn write_outcome(app: &AppState, key: &TaskKey, content: &str) {
        let path = outcome_path(app, key, "rev-1");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    async fn autopilot_of(app: &AppState, key: &TaskKey) -> Autopilot {
        app.store
            .get_task(key)
            .await
            .unwrap()
            .unwrap()
            .autopilot
            .unwrap()
    }

    /// Runs `recover` and waits for the finish it spawned, if any.
    async fn recover_and_wait(app: &AppState, task: &Task) {
        recover(app, task);
        for _ in 0..400 {
            let running = app
                .finishing
                .running
                .lock()
                .unwrap()
                .contains(task.key.as_str());
            if !running {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("the recovery never ended");
    }

    fn creates(gh: &FakeGh) -> Vec<Vec<String>> {
        gh.calls()
            .into_iter()
            .filter(|c| c.starts_with(&["pr".to_string(), "create".to_string()]))
            .collect()
    }

    #[tokio::test]
    async fn a_ready_review_opens_one_draft_pr_with_the_agents_text_and_finishes() {
        let gh = gh("[]");
        let (app, key, _dir) = ready_task(gh.clone(), None).await;
        write_outcome(
            &app,
            &key,
            r#"{"outcome":"ready","pr_title":"feat: agent title","pr_body":"agent body"}"#,
        );
        finish(&app, &key, "rev-1").await;

        let made = creates(&gh);
        assert_eq!(made.len(), 1);
        assert!(made[0].contains(&"--draft".to_string()));
        assert!(made[0].contains(&"feat: agent title".to_string()));
        assert!(gh.stdins().iter().flatten().any(|s| s == "agent body"));
        let a = autopilot_of(&app, &key).await;
        assert_eq!((a.state, a.stage), (AutopilotState::Done, None));
        assert_eq!(app.store.prs_of(&key).await.unwrap().len(), 1);
        let events = app.store.events(&key).await.unwrap();
        let finished: Vec<_> = events
            .iter()
            .filter(|e| e.event.name() == "autopilot-finished")
            .collect();
        assert_eq!(finished.len(), 1);
        assert_eq!(
            finished[0].event,
            Event::AutopilotFinished {
                prs: vec![PR_URL.into()]
            }
        );

        // Finishing again opens nothing.
        finish(&app, &key, "rev-1").await;
        assert_eq!(creates(&gh).len(), 1);
    }

    #[tokio::test]
    async fn the_stage_is_opening_pr_while_the_prs_are_opened() {
        // `gh pr create` fails: the PR is not opened and the stage stays recorded.
        let failing = Arc::new(
            FakeGh::new()
                .with_user("me")
                .on(&["repo", "view"], Ok(REPO_VIEW))
                .on(&["pr", "list"], Ok("[]"))
                .on(
                    &["pr", "create"],
                    Err(crate::gh::GhError::Failed {
                        code: Some(1),
                        stderr: "boom".into(),
                    }),
                ),
        );
        let (app, key, _dir) = ready_task(failing, None).await;
        write_outcome(&app, &key, r#"{"outcome":"ready"}"#);
        finish(&app, &key, "rev-1").await;
        let a = autopilot_of(&app, &key).await;
        assert_eq!(
            (a.state, a.stage),
            (AutopilotState::Running, Some(AutopilotStage::OpeningPr))
        );
    }

    #[tokio::test]
    async fn without_title_or_body_the_pr_uses_the_proposal() {
        let gh = gh("[]");
        let (app, key, _dir) = ready_task(gh.clone(), None).await;
        write_outcome(&app, &key, r#"{"outcome":"ready"}"#);
        finish(&app, &key, "rev-1").await;
        let made = creates(&gh);
        assert_eq!(made.len(), 1);
        assert!(made[0].contains(&"t".to_string()), "the task's title");
        assert!(gh.stdins().iter().flatten().any(|s| s.contains("- first")));
    }

    #[tokio::test]
    async fn a_blocked_review_opens_nothing() {
        let gh = gh("[]");
        let (app, key, _dir) = ready_task(gh.clone(), None).await;
        write_outcome(&app, &key, r#"{"outcome":"blocked","reason":"tests fail"}"#);
        finish(&app, &key, "rev-1").await;
        assert!(creates(&gh).is_empty());
        assert_eq!(
            autopilot_of(&app, &key).await.state,
            AutopilotState::Running
        );
    }

    #[tokio::test]
    async fn recovery_links_the_pr_gh_already_has_and_never_creates_one() {
        let gh = gh(PR_LIST);
        let (app, key, _dir) = ready_task(gh.clone(), Some(AutopilotStage::OpeningPr)).await;
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        recover_and_wait(&app, &task).await;
        assert!(creates(&gh).is_empty());
        let prs = app.store.prs_of(&key).await.unwrap();
        assert_eq!(prs.len(), 1);
        assert_eq!(autopilot_of(&app, &key).await.state, AutopilotState::Done);
    }

    async fn with_preview(app: &mut AppState, command: &str, timeout_ms: u64) {
        app.store
            .set_preview_command("proj", Some(command))
            .await
            .unwrap();
        app.previews = Arc::new(crate::preview::Previews::with_timeout(
            crate::events::Events::default(),
            std::time::Duration::from_millis(timeout_ms),
        ));
    }

    #[tokio::test]
    async fn a_preview_that_runs_puts_its_url_in_the_pr_body() {
        let gh = gh("[]");
        let (mut app, key, _dir) = ready_task(gh.clone(), None).await;
        with_preview(&mut app, "echo http://127.0.0.1:9/x; sleep 30", 5000).await;
        write_outcome(&app, &key, r#"{"outcome":"ready","pr_body":"agent body"}"#);
        finish(&app, &key, "rev-1").await;
        assert_eq!(creates(&gh).len(), 1);
        assert!(
            gh.stdins()
                .iter()
                .flatten()
                .any(|s| s.contains("agent body") && s.contains("Preview: http://127.0.0.1:9/x")),
            "{:?}",
            gh.stdins()
        );
        assert_eq!(autopilot_of(&app, &key).await.state, AutopilotState::Done);
        app.previews.stop(key.as_str());
    }

    #[tokio::test]
    async fn a_failed_preview_still_opens_the_pr_and_does_not_pause() {
        let gh = gh("[]");
        let (mut app, key, _dir) = ready_task(gh.clone(), None).await;
        with_preview(&mut app, "echo oops; exit 3", 5000).await;
        write_outcome(&app, &key, r#"{"outcome":"ready","pr_body":"agent body"}"#);
        finish(&app, &key, "rev-1").await;
        assert_eq!(creates(&gh).len(), 1);
        assert!(!gh.stdins().iter().flatten().any(|s| s.contains("Preview:")));
        assert_eq!(autopilot_of(&app, &key).await.state, AutopilotState::Done);
    }

    #[tokio::test]
    async fn a_preview_that_prints_no_url_times_out_and_the_pr_opens() {
        let gh = gh("[]");
        let (mut app, key, _dir) = ready_task(gh.clone(), None).await;
        with_preview(&mut app, "sleep 30", 300).await;
        write_outcome(&app, &key, r#"{"outcome":"ready"}"#);
        finish(&app, &key, "rev-1").await;
        assert_eq!(creates(&gh).len(), 1);
        assert_eq!(autopilot_of(&app, &key).await.state, AutopilotState::Done);
        app.previews.stop(key.as_str());
    }

    #[tokio::test]
    async fn the_stage_is_preview_while_the_preview_starts() {
        let gh = gh("[]");
        let (mut app, key, _dir) = ready_task(gh.clone(), None).await;
        with_preview(&mut app, "sleep 30", 60_000).await;
        write_outcome(&app, &key, r#"{"outcome":"ready"}"#);
        let a2 = app.clone();
        let k2 = key.clone();
        let handle = tokio::spawn(async move { finish(&a2, &k2, "rev-1").await });
        for _ in 0..100 {
            if autopilot_of(&app, &key).await.stage == Some(AutopilotStage::Preview) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert_eq!(
            autopilot_of(&app, &key).await.stage,
            Some(AutopilotStage::Preview)
        );
        assert!(creates(&gh).is_empty());
        // Recovery leaves it alone while its finish is alive.
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        recover(&app, &task);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(creates(&gh).is_empty());
        // Aborting the finish (a dropped future) leaves no task stuck as finishing.
        handle.abort();
        let _ = handle.await;
        assert!(app.finishing.try_guard(&key).is_some());
        app.previews.stop(key.as_str());
    }

    #[tokio::test]
    async fn recovery_of_a_lost_preview_goes_on_to_the_pr() {
        let gh = gh("[]");
        let (mut app, key, _dir) = ready_task(gh.clone(), Some(AutopilotStage::Preview)).await;
        with_preview(&mut app, "echo oops; exit 3", 5000).await;
        write_outcome(&app, &key, r#"{"outcome":"ready"}"#);
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        recover_and_wait(&app, &task).await;
        assert_eq!(creates(&gh).len(), 1);
        assert_eq!(autopilot_of(&app, &key).await.state, AutopilotState::Done);
    }

    #[tokio::test]
    async fn the_finishing_guard_is_one_per_task_and_released_on_drop_or_panic() {
        let f = Arc::new(Finishing::default());
        let (a, b) = (TaskKey::new("A-1"), TaskKey::new("B-1"));
        let guard = f.try_guard(&a).unwrap();
        assert!(f.try_guard(&a).is_none());
        assert!(f.try_guard(&b).is_some());
        drop(guard);
        assert!(f.try_guard(&a).is_some());
        let f2 = f.clone();
        let a2 = a.clone();
        let panicked = tokio::spawn(async move {
            let _guard = f2.try_guard(&a2).unwrap();
            panic!("boom");
        })
        .await;
        assert!(panicked.is_err());
        assert!(f.try_guard(&a).is_some());
    }

    /// A ready task whose repo has `subjects` commits.
    async fn task_with_commits(
        gh: Arc<FakeGh>,
        stage: Option<AutopilotStage>,
        subjects: &[&str],
    ) -> (AppState, TaskKey, TempDir) {
        let (app, key, dir) = task_with_workspace(gh, None).await;
        git_repo_with_origin(
            &dir.path().join("ws"),
            &dir.path().join("origin"),
            "feature/PROJ-1",
            subjects,
        );
        let a = Autopilot {
            stage,
            review_session: Some("rev-1".into()),
            ..running()
        };
        app.store.set_autopilot(&key, Some(&a)).await.unwrap();
        (app, key, dir)
    }

    async fn stage_of(app: &AppState, key: &TaskKey) -> Option<AutopilotStage> {
        autopilot_of(app, key).await.stage
    }

    /// After a stop the autopilot is still running but a recovery tick leaves it alone.
    async fn assert_not_retried(app: &AppState, key: &TaskKey, gh: &FakeGh) {
        let calls = gh.calls().len();
        let task = app.store.get_task(key).await.unwrap().unwrap();
        recover_and_wait(app, &task).await;
        assert_eq!(gh.calls().len(), calls, "recovery retried");
        assert_eq!(autopilot_of(app, key).await.state, AutopilotState::Running);
    }

    #[tokio::test]
    async fn a_blocked_review_stops_for_good() {
        // The review is blocked at a turn end: no stage is set, so recovery has nothing
        // to take up; and nothing was written to GitHub.
        let gh = gh("[]");
        let (app, key, _dir) = ready_task(gh.clone(), None).await;
        write_outcome(&app, &key, r#"{"outcome":"blocked","reason":"tests fail"}"#);
        finish(&app, &key, "rev-1").await;
        assert_eq!(stage_of(&app, &key).await, None);
        assert_not_retried(&app, &key, &gh).await;
        assert!(creates(&gh).is_empty());
    }

    #[tokio::test]
    async fn no_commits_is_terminal_and_clears_the_stage() {
        let gh = gh("[]");
        let (app, key, _dir) =
            task_with_commits(gh.clone(), Some(AutopilotStage::OpeningPr), &[]).await;
        finish(&app, &key, "rev-1").await;
        assert!(creates(&gh).is_empty());
        assert_eq!(stage_of(&app, &key).await, None);
        assert_not_retried(&app, &key, &gh).await;
    }

    #[tokio::test]
    async fn a_task_without_workspace_is_terminal_and_clears_the_stage() {
        let config = crate::config::Config {
            home: std::env::temp_dir().join(format!("delilah-ap-{}", uuid::Uuid::new_v4())),
            addr: "127.0.0.1:7420".parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            agent_program: None,
        };
        let gh = gh("[]");
        let app = AppState::new(crate::store::Store::in_memory().await.unwrap(), config)
            .with_gh(gh.clone());
        let key = TaskKey::new("PROJ-1");
        let mut t = Task::new(key.clone(), "t", "feature");
        t.saving_mode = true;
        app.store.create_task(&t).await.unwrap();
        let a = Autopilot {
            stage: Some(AutopilotStage::OpeningPr),
            review_session: Some("rev-1".into()),
            ..running()
        };
        app.store.set_autopilot(&key, Some(&a)).await.unwrap();
        finish(&app, &key, "rev-1").await;
        assert_eq!(stage_of(&app, &key).await, None);
        assert_not_retried(&app, &key, &gh).await;
    }

    fn failing_create() -> Arc<FakeGh> {
        Arc::new(
            FakeGh::new()
                .with_user("me")
                .on(&["repo", "view"], Ok(REPO_VIEW))
                .on(&["pr", "list"], Ok("[]"))
                .on(
                    &["pr", "create"],
                    Err(crate::gh::GhError::Failed {
                        code: Some(1),
                        stderr: "network".into(),
                    }),
                ),
        )
    }

    #[tokio::test]
    async fn a_gh_failure_is_retried_with_a_backoff_and_a_cap() {
        let gh = failing_create();
        let (app, key, _dir) = ready_task(gh.clone(), None).await;
        write_outcome(&app, &key, r#"{"outcome":"ready"}"#);
        finish(&app, &key, "rev-1").await;
        assert_eq!(creates(&gh).len(), 1);
        assert_eq!(stage_of(&app, &key).await, Some(AutopilotStage::OpeningPr));
        // The next ticks wait for the backoff: no second attempt right away.
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        recover_and_wait(&app, &task).await;
        recover_and_wait(&app, &task).await;
        assert_eq!(creates(&gh).len(), 1);
        // Once it is due, it is tried again, and after the cap it stops for good.
        app.finishing
            .retries
            .lock()
            .unwrap()
            .get_mut("PROJ-1")
            .unwrap()
            .next = Instant::now();
        recover_and_wait(&app, &task).await;
        assert_eq!(creates(&gh).len(), 2);
        assert_eq!(stage_of(&app, &key).await, Some(AutopilotStage::OpeningPr));
        app.finishing
            .retries
            .lock()
            .unwrap()
            .get_mut("PROJ-1")
            .unwrap()
            .next = Instant::now();
        recover_and_wait(&app, &task).await;
        assert_eq!(creates(&gh).len(), MAX_ATTEMPTS as usize);
        assert_eq!(stage_of(&app, &key).await, None);
        assert_not_retried(&app, &key, &gh).await;
        assert_eq!(creates(&gh).len(), MAX_ATTEMPTS as usize);
    }

    #[test]
    fn stops_are_classified() {
        assert!(FinishStop::Blocked("r".into()).is_terminal());
        assert!(FinishStop::NoCommits.is_terminal());
        assert!(FinishStop::NoWorkspace.is_terminal());
        assert!(!FinishStop::OpenPr(ApiError::conflict("x")).is_terminal());
        assert!(!FinishStop::StateWrite("x".into()).is_terminal());
    }

    #[tokio::test]
    async fn recovery_reads_the_outcome_of_the_recorded_review_session() {
        let gh = gh("[]");
        let (app, key, _dir) = ready_task(gh.clone(), Some(AutopilotStage::OpeningPr)).await;
        // `rev-1` is the main session, but the review that ran was `rev-0`.
        let mut a = autopilot_of(&app, &key).await;
        a.review_session = Some("rev-0".into());
        app.store.set_autopilot(&key, Some(&a)).await.unwrap();
        let path = outcome_path(&app, &key, "rev-0");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, r#"{"outcome":"ready","pr_title":"title of rev-0"}"#).unwrap();
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        recover_and_wait(&app, &task).await;
        assert!(creates(&gh)[0].contains(&"title of rev-0".to_string()));
    }

    #[tokio::test]
    async fn finishing_records_the_review_session() {
        let gh = gh("[]");
        let (app, key, _dir) = ready_task(gh.clone(), None).await;
        let mut a = autopilot_of(&app, &key).await;
        a.review_session = None;
        app.store.set_autopilot(&key, Some(&a)).await.unwrap();
        write_outcome(&app, &key, r#"{"outcome":"ready"}"#);
        finish(&app, &key, "rev-1").await;
        assert_eq!(
            autopilot_of(&app, &key).await.review_session.as_deref(),
            Some("rev-1")
        );
    }

    #[tokio::test]
    async fn preparing_the_review_makes_its_folder_and_records_the_session() {
        let (app, key, _dir) = ready_task(gh("[]"), None).await;
        let path = outcome_path(&app, &key, "rev-2");
        assert!(!path.parent().unwrap().exists());
        let note = prepare_step(&app, &key, Step::Review, "rev-2")
            .await
            .unwrap();
        assert!(note.contains(&path.display().to_string()));
        assert!(path.parent().unwrap().is_dir());
        assert_eq!(
            autopilot_of(&app, &key).await.review_session.as_deref(),
            Some("rev-2")
        );
        // Not on autopilot: nothing.
        app.store.set_autopilot(&key, None).await.unwrap();
        assert!(
            prepare_step(&app, &key, Step::Review, "rev-3")
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn recovery_of_a_lost_preview_never_starts_a_new_one() {
        let gh = gh("[]");
        let (mut app, key, _dir) = ready_task(gh.clone(), Some(AutopilotStage::Preview)).await;
        with_preview(&mut app, "echo http://127.0.0.1:9/x; sleep 30", 5000).await;
        write_outcome(&app, &key, r#"{"outcome":"ready"}"#);
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        recover_and_wait(&app, &task).await;
        assert!(app.previews.view(key.as_str()).is_none());
        assert_eq!(creates(&gh).len(), 1);
        assert!(!gh.stdins().iter().flatten().any(|s| s.contains("Preview:")));
    }

    #[test]
    fn an_outcome_that_is_not_a_regular_file_or_is_too_big_is_blocked() {
        let blocked = Outcome::Blocked {
            reason: "the review did not report an outcome".into(),
        };
        // A symlink, even to a good file.
        let (d, good) = file_with(Some(r#"{"outcome":"ready"}"#));
        let link = d.join("link.json");
        std::os::unix::fs::symlink(&good, &link).unwrap();
        assert_eq!(read_outcome(Lang::En, &link), blocked);
        // A directory.
        assert_eq!(read_outcome(Lang::En, &d), blocked);
        // Too big, though valid JSON.
        let big = format!(
            r#"{{"outcome":"ready","pr_body":"{}"}}"#,
            "x".repeat(MAX_OUTCOME_BYTES as usize)
        );
        let (_d, p) = file_with(Some(&big));
        assert_eq!(read_outcome(Lang::En, &p), blocked);
    }

    #[test]
    fn the_title_is_one_short_line_and_the_body_is_capped() {
        let long = "t".repeat(1000);
        let json = serde_json::json!({
            "outcome": "ready",
            "pr_title": format!("feat:\n x\r\u{7}y\t{long}"),
            "pr_body": "b".repeat(70_000),
        });
        let (_d, p) = file_with(Some(&json.to_string()));
        let Outcome::Ready { pr_title, pr_body } = read_outcome(Lang::En, &p) else {
            panic!("not ready");
        };
        let title = pr_title.unwrap();
        assert!(title.starts_with("feat: xy"), "{title}");
        assert!(!title.chars().any(char::is_control));
        assert_eq!(title.chars().count(), MAX_TITLE_CHARS);
        assert_eq!(pr_body.unwrap().chars().count(), MAX_BODY_CHARS);
        // A title of only control characters is no title.
        let (_d, p) = file_with(Some(r#"{"outcome":"ready","pr_title":"\n\t"}"#));
        assert_eq!(
            read_outcome(Lang::En, &p),
            Outcome::Ready {
                pr_title: None,
                pr_body: None
            }
        );
    }

    #[test]
    fn the_preview_url_loses_credentials_query_and_fragment_but_keeps_loopback() {
        assert_eq!(
            public_url("http://user:pw@localhost:5173/app/?token=1#frag"),
            "http://localhost:5173/app/"
        );
        assert_eq!(
            public_url("https://a:b@c@127.0.0.1:9?x=1"),
            "https://127.0.0.1:9"
        );
        assert_eq!(public_url("http://127.0.0.1:9/x"), "http://127.0.0.1:9/x");
    }

    #[tokio::test]
    async fn the_pr_body_carries_the_sanitised_preview_url() {
        let gh = gh("[]");
        let (mut app, key, _dir) = ready_task(gh.clone(), None).await;
        with_preview(
            &mut app,
            "echo 'http://u:p@127.0.0.1:9/x?token=s#f'; sleep 30",
            5000,
        )
        .await;
        write_outcome(&app, &key, r#"{"outcome":"ready","pr_body":"agent body"}"#);
        finish(&app, &key, "rev-1").await;
        let stdins = gh.stdins();
        let body = stdins
            .iter()
            .flatten()
            .find(|s| s.contains("agent body"))
            .unwrap();
        assert!(body.contains("Preview: http://127.0.0.1:9/x\n"), "{body}");
        assert!(!body.contains("token") && !body.contains("u:p"));
        app.previews.stop(key.as_str());
    }
}
