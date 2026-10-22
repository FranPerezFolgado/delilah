//! The finish of a review: read its outcome, start the preview, open the draft PRs.
//! Every write of the autopilot's state is a compare-and-set that only lands while it
//! is still running on this review: an `off`, a pause or a newer review that came in
//! during a wait (a preview can take minutes) stops the finish without a write.

use std::time::Duration;

use chrono::Utc;
use delilah_core::{Autopilot, AutopilotStage, AutopilotState, Event, Task};
use delilah_git::GitOptions;

use super::outcome::{Outcome, read_outcome};
use super::pause::pause;
use super::{FinishGuard, MAX_ATTEMPTS, is_running, outcome_path, update};
use crate::api::ApiError;
use crate::lang::Lang;
use crate::pr_types::PrState;
use crate::preview::PreviewState;
use crate::state::AppState;
use delilah_core::TaskKey;

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
    /// The review is ready but these repos have uncommitted changes.
    Uncommitted(Vec<String>),
    /// Autopilot was turned off or paused, or another review took over, meanwhile: the
    /// finish stops quietly.
    Superseded,
}

impl FinishStop {
    /// Whether trying again changes nothing.
    pub(super) fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Blocked(_) | Self::NoCommits | Self::NoWorkspace | Self::Uncommitted(_)
        )
    }

    /// The pause's reason, in the app language.
    pub(super) fn describe(&self, lang: Lang) -> String {
        match self {
            Self::Blocked(reason) => format!(
                "{}: {reason}",
                lang.pick("la revisión está bloqueada", "the review is blocked")
            ),
            Self::NoCommits => lang
                .pick(
                    "no hay commits con los que abrir una PR",
                    "there are no commits to open a PR with",
                )
                .into(),
            Self::NoWorkspace => lang
                .pick("la tarea no tiene workspace", "the task has no workspace")
                .into(),
            Self::OpenPr(e) => format!(
                "{}: {}",
                lang.pick("no puedo abrir la PR", "cannot open the PR"),
                e.message()
            ),
            Self::StateWrite(e) => format!(
                "{}: {e}",
                lang.pick("no puedo guardar el estado", "cannot record the state")
            ),
            Self::Superseded => String::new(),
            Self::Uncommitted(repos) => {
                let repos: Vec<&str> = repos
                    .iter()
                    .map(|r| match r.as_str() {
                        "." => lang.pick("el workspace", "the workspace"),
                        other => other,
                    })
                    .collect();
                format!(
                    "{}: {}",
                    lang.pick(
                        "la revisión terminó con cambios sin commit en",
                        "the review ended with uncommitted changes in"
                    ),
                    repos.join(", ")
                )
            }
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

pub(super) async fn finish_guarded(
    app: &AppState,
    key: &TaskKey,
    session: &str,
    guard: FinishGuard,
) {
    let result = run_finish(app, key, session).await;
    // The guard goes before a pause: a resume that comes in meanwhile must find it free.
    drop(guard);
    let Err(stop) = result else {
        app.finishing.forget(key);
        return;
    };
    if matches!(stop, FinishStop::Superseded) {
        app.finishing.forget(key);
        tracing::info!(task = %key, "the autopilot's finish was superseded");
        return;
    }
    let reason = stop.describe(app.lang());
    tracing::warn!(task = %key, "autopilot stops: {reason}");
    // A terminal stop, or a retryable one that keeps failing, pauses the autopilot: a
    // paused one is not taken up again by `recover`, and a resume re-runs the (idempotent)
    // opening from the stage it left.
    if stop.is_terminal() || app.finishing.failed(key) >= MAX_ATTEMPTS {
        app.finishing.forget(key);
        pause(app, key, reason).await;
    }
}

/// A resume found a finish running (it may be about to pause): waits for it to let go,
/// then runs the interrupted finish if it is still pending and due. Gives up when
/// autopilot no longer waits at the PR (the other finish got it done, or paused and was
/// resumed elsewhere) or the wait is over (`recover` picks it up then).
pub(super) async fn finish_when_free(app: &AppState, key: &TaskKey) {
    for _ in 0..150 {
        let Ok(Some(task)) = app.store.get_task(key).await else {
            return;
        };
        let Some(session) = pending_session(&task) else {
            return;
        };
        if !app.finishing.due(key) {
            return;
        }
        if let Some(guard) = app.finishing.try_guard(key) {
            return finish_guarded(app, key, &session, guard).await;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// The review session of a running autopilot that is half way through its finish.
fn pending_session(task: &Task) -> Option<String> {
    let a = task.autopilot.as_ref().filter(|_| is_running(task))?;
    matches!(
        a.stage,
        Some(AutopilotStage::OpeningPr | AutopilotStage::Preview)
    )
    .then(|| a.review_session.clone())
    .flatten()
}

/// Edits the autopilot, only while it is running on this review (`session`, or none
/// recorded yet); records `session` with the edit. Else nothing is written and the
/// finish is `Superseded`.
async fn write(
    app: &AppState,
    key: &TaskKey,
    session: &str,
    mut change: impl FnMut(&mut Autopilot),
) -> Result<(), FinishStop> {
    let written = update(app, key, |a| {
        let ours = a.state == AutopilotState::Running
            && a.review_session.as_deref().is_none_or(|s| s == session);
        if ours {
            a.review_session = Some(session.to_string());
            change(a);
        }
        ours
    })
    .await
    .map_err(|e| {
        tracing::warn!(task = %key, "cannot record the autopilot: {e}");
        FinishStop::StateWrite(e.to_string())
    })?;
    if written {
        Ok(())
    } else {
        Err(FinishStop::Superseded)
    }
}

/// Whether autopilot still runs on this review.
async fn still_ours(app: &AppState, key: &TaskKey, session: &str) -> bool {
    let Ok(Some(task)) = app.store.get_task(key).await else {
        return false;
    };
    is_running(&task)
        && task
            .autopilot
            .and_then(|a| a.review_session)
            .is_none_or(|s| s == session)
}

/// The repos of the task's workspace with uncommitted changes; none when it cannot be
/// read (the PR step reports a missing workspace itself).
async fn uncommitted(app: &AppState, key: &TaskKey) -> Vec<String> {
    let Ok(Some((_, ws))) = app.store.workspace(key).await else {
        return vec![];
    };
    if !ws.path.is_dir() {
        return vec![];
    }
    let opts = GitOptions {
        allow_file_protocol: app.config.allow_file_protocol,
    };
    tokio::task::spawn_blocking(move || ws.uncommitted(&opts))
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default()
}

async fn run_finish(app: &AppState, key: &TaskKey, session: &str) -> Result<(), FinishStop> {
    let Ok(Some(task)) = app.store.get_task(key).await else {
        return Ok(());
    };
    let Some(autopilot) = task.autopilot.as_ref().filter(|_| is_running(&task)) else {
        return Ok(());
    };
    if autopilot
        .review_session
        .as_deref()
        .is_some_and(|s| s != session)
    {
        // Another review took over.
        return Ok(());
    }
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
    if !reopening {
        // Before anything is pushed: what the agent left uncommitted would not be in
        // the PR.
        let dirty = uncommitted(app, key).await;
        if !dirty.is_empty() {
            return Err(FinishStop::Uncommitted(dirty));
        }
        preview_step(app, key, session).await?;
    }
    let preview_url = app
        .previews
        .view(key.as_str())
        .filter(|v| v.state == PreviewState::Running)
        .and_then(|v| v.url);
    if !opening {
        write(app, key, session, |a| {
            a.stage = Some(AutopilotStage::OpeningPr)
        })
        .await?;
    }
    // Links a PR that exists already (a restart in the middle of opening, or one made
    // by hand), so none is opened twice.
    crate::pr_loop::check_task(app, key, true).await;
    open_prs(
        app,
        key,
        session,
        title.as_deref(),
        body.as_deref(),
        preview_url.as_deref(),
    )
    .await?;
    let urls = linked_urls(app, key).await;
    if urls.is_empty() {
        return Err(FinishStop::NoCommits);
    }
    write(app, key, session, |a| {
        a.state = AutopilotState::Done;
        a.stage = None;
        a.since = Utc::now();
    })
    .await?;
    if let Err(e) = app
        .transition(key, Event::AutopilotFinished { prs: urls })
        .await
    {
        tracing::warn!(task = %key, "cannot record the autopilot's end: {e}");
    }
    Ok(())
}

/// Starts the project's preview, if it has a command, and waits until it runs or
/// fails. A preview that fails never stops the PR; only the state write can (or autopilot being turned off meanwhile).
async fn preview_step(app: &AppState, key: &TaskKey, session: &str) -> Result<(), FinishStop> {
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
    write(app, key, session, |a| {
        a.stage = Some(AutopilotStage::Preview)
    })
    .await?;
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

/// Opens a draft PR in each repo that has commits and no open PR, while autopilot
/// still runs on this review.
async fn open_prs(
    app: &AppState,
    key: &TaskKey,
    session: &str,
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
        // Off, paused or replaced during the preview wait or the previous PR: no more.
        if !still_ours(app, key, session).await {
            return Err(FinishStop::Superseded);
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
            .map_err(|mut e| {
                // On a parent the pause says which repo failed.
                if ws.kind == delilah_core::ProjectKind::Parent {
                    e.message = format!("{repo}: {}", e.message);
                }
                FinishStop::OpenPr(e)
            })?;
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

#[cfg(test)]
mod tests {

    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use delilah_core::{Autopilot, AutopilotStage, AutopilotState, Event, Task, TaskKey};

    use crate::api::ApiError;
    use crate::gh::fake::{FakeGh, PR_LIST, PR_URL, REPO_VIEW};

    use crate::lang::Lang;
    use crate::state::AppState;

    use super::*;
    use crate::autopilot::pause::resume_with;
    use crate::autopilot::testkit::*;

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
        assert_eq!(autopilot_of(&app, &key).await.state, AutopilotState::Paused);
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

    /// After a stop the autopilot is paused and a recovery tick leaves it alone.
    async fn assert_not_retried(app: &AppState, key: &TaskKey, gh: &FakeGh) {
        let calls = gh.calls().len();
        let task = app.store.get_task(key).await.unwrap().unwrap();
        recover_and_wait(app, &task).await;
        assert_eq!(gh.calls().len(), calls, "recovery retried");
        assert_eq!(autopilot_of(app, key).await.state, AutopilotState::Paused);
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
    async fn no_commits_is_terminal_and_pauses_keeping_the_stage() {
        let gh = gh("[]");
        let (app, key, _dir) =
            task_with_commits(gh.clone(), Some(AutopilotStage::OpeningPr), &[]).await;
        finish(&app, &key, "rev-1").await;
        assert!(creates(&gh).is_empty());
        assert_eq!(stage_of(&app, &key).await, Some(AutopilotStage::OpeningPr));
        assert_not_retried(&app, &key, &gh).await;
    }

    #[tokio::test]
    async fn a_task_without_workspace_is_terminal_and_pauses() {
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
        assert_eq!(autopilot_of(&app, &key).await.state, AutopilotState::Paused);
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
        let a = autopilot_of(&app, &key).await;
        assert_eq!(a.state, AutopilotState::Paused);
        assert!(a.reason.unwrap().contains("network"));
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

    #[tokio::test]
    async fn ready_with_uncommitted_changes_pauses_before_any_push() {
        let gh = gh("[]");
        let (app, key, dir) = ready_task(gh.clone(), None).await;
        std::fs::write(dir.path().join("ws/forgotten.txt"), "x").unwrap();
        write_outcome(&app, &key, r#"{"outcome":"ready"}"#);
        finish(&app, &key, "rev-1").await;
        let a = autopilot_of(&app, &key).await;
        assert_eq!(a.state, AutopilotState::Paused);
        assert!(a.reason.unwrap().contains("sin commit"));
        assert!(creates(&gh).is_empty());
        let pushed = std::process::Command::new("git")
            .args(["rev-parse", "--verify", "feature/PROJ-1"])
            .current_dir(dir.path().join("origin"))
            .output()
            .unwrap();
        assert!(!pushed.status.success(), "nothing was pushed");
    }

    #[tokio::test]
    async fn the_pause_reasons_follow_the_language() {
        let stop = FinishStop::Uncommitted(vec![".".into(), "svc-a".into()]);
        assert_eq!(
            stop.describe(Lang::En),
            "the review ended with uncommitted changes in: the workspace, svc-a"
        );
        assert!(stop.describe(Lang::Es).contains("cambios sin commit"));
        assert!(stop.is_terminal());
    }

    #[tokio::test]
    async fn a_failure_on_the_second_repo_pauses_naming_it_and_a_resume_opens_only_it() {
        let gh = gh("[]");
        let (app, key, dir) = parent_ready(gh.clone()).await;
        app.lang.set(Lang::En);
        for _ in 0..MAX_ATTEMPTS {
            finish(&app, &key, "rev-1").await;
        }
        let a = autopilot_of(&app, &key).await;
        assert_eq!(a.state, AutopilotState::Paused);
        assert!(a.reason.unwrap().contains("svc-b"));
        let rows = app.store.prs_of(&key).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].repo, "svc-a");
        assert_eq!(creates(&gh).len(), 1);

        // The user fixes the remote and resumes: only svc-b is opened.
        let origin = dir.path().join("origin-svc-b");
        git_in(
            &dir.path().join("ws/svc-b"),
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        let launched = Launched::default();
        resume_with(&app, &key, &stub(&launched))
            .await
            .ok()
            .unwrap();
        wait_for(&app, &key, AutopilotState::Done).await;
        assert_eq!(creates(&gh).len(), 2);
        assert_eq!(app.store.prs_of(&key).await.unwrap().len(), 2);
    }

    async fn wait_for_stage(app: &AppState, key: &TaskKey, stage: AutopilotStage) {
        for _ in 0..200 {
            if autopilot_of(app, key).await.stage == Some(stage) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("the stage never became {stage:?}");
    }

    #[tokio::test]
    async fn off_during_the_preview_wait_opens_no_pr_and_stays_off() {
        let gh = gh("[]");
        let (mut app, key, _dir) = ready_task(gh.clone(), None).await;
        with_preview(&mut app, "sleep 30", 1500).await;
        write_outcome(&app, &key, r#"{"outcome":"ready"}"#);
        let handle = tokio::spawn({
            let (app, key) = (app.clone(), key.clone());
            async move { finish(&app, &key, "rev-1").await }
        });
        wait_for_stage(&app, &key, AutopilotStage::Preview).await;
        app.store.set_autopilot(&key, None).await.unwrap();
        handle.await.unwrap();
        assert!(creates(&gh).is_empty());
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        assert!(task.autopilot.is_none(), "off stays off");
        app.previews.stop(key.as_str());
    }

    #[tokio::test]
    async fn a_pause_during_the_preview_wait_stays_paused_and_opens_no_pr() {
        let gh = gh("[]");
        let (mut app, key, _dir) = ready_task(gh.clone(), None).await;
        with_preview(&mut app, "sleep 30", 1500).await;
        write_outcome(&app, &key, r#"{"outcome":"ready"}"#);
        let handle = tokio::spawn({
            let (app, key) = (app.clone(), key.clone());
            async move { finish(&app, &key, "rev-1").await }
        });
        wait_for_stage(&app, &key, AutopilotStage::Preview).await;
        pause(&app, &key, "user stopped it").await;
        handle.await.unwrap();
        assert!(creates(&gh).is_empty());
        let a = autopilot_of(&app, &key).await;
        assert_eq!(a.state, AutopilotState::Paused);
        assert_eq!(a.reason.as_deref(), Some("user stopped it"));
        assert!(!app.finishing.running.lock().unwrap().contains("PROJ-1"));
        app.previews.stop(key.as_str());
    }

    #[tokio::test]
    async fn a_newer_review_session_stops_the_finish_of_the_old_one() {
        let gh = gh("[]");
        let (app, key, _dir) = ready_task(gh.clone(), None).await;
        write_outcome(&app, &key, r#"{"outcome":"ready"}"#);
        let mut a = autopilot_of(&app, &key).await;
        a.review_session = Some("rev-2".into());
        app.store.set_autopilot(&key, Some(&a)).await.unwrap();
        finish(&app, &key, "rev-1").await;
        assert!(creates(&gh).is_empty());
        let a = autopilot_of(&app, &key).await;
        assert_eq!(
            (a.state, a.review_session.as_deref()),
            (AutopilotState::Running, Some("rev-2"))
        );
    }
}
