//! Pausing, stalling, resuming and moving the chain on.

use chrono::Utc;
use delilah_core::{AutopilotStage, AutopilotState, Event, HandoffStage, Step, Task, TaskKey};

use super::finish::{finish_guarded, finish_when_free};
use super::notes::{nudge_text, outcome_path, recheck_text, tasks_unchecked};
use super::outcome::{Outcome, read_outcome};
use super::{is_running, is_task, task_of, update};
use crate::api::ApiError;
use crate::events::Topic;
use crate::handoff::Launcher;
use crate::inbox::{AttentionKind, Change};
use crate::specs::SpecSummary;
use crate::state::AppState;

/// A permission prompt of the task's primary session is something autopilot cannot
/// answer: it pauses (a no-op when the task is not on autopilot, is a project's owner,
/// or the prompt comes from another session, which autopilot does not drive). The
/// usage limit never gets here: it clears attention.
pub(crate) async fn on_attention(app: &AppState, key: &TaskKey, session: &str, change: &Change) {
    if !is_task(key) {
        return;
    }
    if !matches!(change, Change::Set(a) if a.kind == AttentionKind::Permission) {
        return;
    }
    let primary = crate::handoff::primary_session(app, key).await;
    if primary.is_some_and(|p| p.id == session) {
        let reason = app.lang().pick(
            "La sesión espera un permiso.",
            "The session is waiting for a permission.",
        );
        pause(app, key, reason).await;
    }
}

/// Pauses a running autopilot: records why and when, writes it to the history, tells
/// the inbox and sends one notification (none when the app is listening). The session
/// and the task's phase are left alone. A paused, done or absent autopilot is not
/// touched, so a second cause for the same pause notifies nothing.
///
/// It must never take `app.handoffs.guard`: its callers (the handoff's failure paths)
/// hold it.
pub(crate) async fn pause(app: &AppState, key: &TaskKey, reason: impl Into<String>) {
    let reason = reason.into();
    let Ok(Some(task)) = app.store.get_task(key).await else {
        return;
    };
    let paused = update(app, key, |a| {
        if a.state != AutopilotState::Running {
            return false;
        }
        a.state = AutopilotState::Paused;
        a.reason = Some(reason.clone());
        a.since = Utc::now();
        true
    })
    .await;
    match paused {
        Ok(true) => {}
        Ok(false) => return,
        Err(e) => {
            tracing::warn!(task = %key, "cannot record the autopilot: {e}");
            return;
        }
    }
    if let Err(e) = app
        .transition(
            key,
            Event::AutopilotPaused {
                reason: reason.clone(),
            },
        )
        .await
    {
        tracing::warn!(task = %key, "cannot record the autopilot's pause: {e}");
    }
    app.emit(Topic::Inbox);
    if !app.events.listening() {
        let lang = app.lang();
        app.notifier.send(
            format!(
                "{key} {}",
                lang.pick("autopiloto en pausa", "autopilot paused")
            ),
            format!("{}: {reason}", task.title),
        );
    }
}

/// What `stall` decided under the compare-and-set.
enum Stalled {
    /// Autopilot is no longer running: nothing to do.
    Gone,
    /// Nudged already, or there is no session to nudge: pause.
    Pause,
    /// This call took the step's one nudge.
    Nudge,
}

/// The primary session's turn ended on autopilot without reaching its step's end. The
/// first stall of a step gets one nudge typed into the session; a second pauses with
/// the step's reason. The review never stalls here: its end is its outcome file.
/// `task` is what the caller read at the turn end: no stall while a handoff is under
/// way. Taking the nudge is atomic, so two turn ends close together nudge once.
pub(crate) async fn stall(
    app: &AppState,
    key: &TaskKey,
    task: &Task,
    step: Option<Step>,
    spec: &SpecSummary,
) {
    let Some(step) = step.filter(|s| *s != Step::Review) else {
        return;
    };
    if !is_running(task)
        || task
            .handoff
            .as_ref()
            .is_some_and(|h| h.stage != HandoffStage::Skipped)
    {
        return;
    }
    let lang = app.lang();
    let reason = if spec.dir.is_none() || spec.total == 0 {
        lang.pick("tasks.md no está escrito", "tasks.md not written")
            .to_string()
    } else {
        tasks_unchecked(lang, spec.total.saturating_sub(spec.done))
    };
    let live = app.sessions.primary(key.as_str());
    let mut decided = Stalled::Gone;
    let written = update(app, key, |a| {
        decided = Stalled::Gone;
        if a.state != AutopilotState::Running {
            return false;
        }
        if a.nudged == Some(step) || live.is_none() {
            decided = Stalled::Pause;
            return false;
        }
        a.nudged = Some(step);
        decided = Stalled::Nudge;
        true
    })
    .await;
    if let Err(e) = written {
        tracing::warn!(task = %key, "cannot record the autopilot: {e}");
        return;
    }
    let live = match (decided, live) {
        (Stalled::Gone, _) => return,
        (Stalled::Pause, _) | (Stalled::Nudge, None) => return pause(app, key, reason).await,
        (Stalled::Nudge, Some(live)) => live,
    };
    if let Err(e) = app.transition(key, Event::AutopilotNudged { step }).await {
        tracing::warn!(task = %key, "cannot record the nudge: {e}");
    }
    if let Err(e) = crate::actions::type_prompt(&live, nudge_text(lang)).await {
        pause(app, key, format!("{reason}: {}", e.message())).await;
    }
}

/// Resumes a paused autopilot: running again, the nudge forgotten, and the chain
/// re-evaluated from the current state (`advance_with`).
pub(crate) async fn resume(app: &AppState, key: &TaskKey) -> Result<(), ApiError> {
    resume_with(app, key, &crate::run::launch_session_api).await
}

pub(crate) async fn resume_with<L: Launcher>(
    app: &AppState,
    key: &TaskKey,
    launch: &L,
) -> Result<(), ApiError> {
    task_of(app, key).await?;
    let resumed = update(app, key, |a| {
        if a.state != AutopilotState::Paused {
            return false;
        }
        a.state = AutopilotState::Running;
        a.reason = None;
        a.nudged = None;
        a.since = Utc::now();
        true
    })
    .await
    .map_err(|e| {
        tracing::warn!(task = %key, "cannot record the autopilot: {e}");
        ApiError::internal(format!("cannot record the state: {e}"))
    })?;
    if !resumed {
        return Err(ApiError::conflict_reason(
            format!("the autopilot of {key} is not paused"),
            "not-paused",
        ));
    }
    app.finishing.forget(key);
    if let Err(e) = app.transition(key, Event::AutopilotResumed).await {
        tracing::warn!(task = %key, "cannot record the autopilot's resume: {e}");
    }
    app.emit(Topic::Inbox);
    advance_with(app, key, launch).await;
    Ok(())
}

/// Goes on with the chain from what exists now: an interrupted finish is run again
/// (waiting for the one that is running, if any), a session still working is left to
/// reach its turn end, a failed or offered handoff goes on (and pauses again with the
/// error if it cannot), and an idle session is looked at as at a turn end.
/// Types the re-check into the review session (its turn end finishes again), recorded
/// as the review's nudge.
async fn recheck(
    app: &AppState,
    key: &TaskKey,
    review: &crate::sessions::Live,
) -> Result<(), ApiError> {
    if let Err(e) = app
        .transition(key, Event::AutopilotNudged { step: Step::Review })
        .await
    {
        tracing::warn!(task = %key, "cannot record the nudge: {e}");
    }
    let text = recheck_text(app.lang(), &outcome_path(app, key, &review.id));
    crate::actions::type_prompt(review, &text).await
}

pub(crate) async fn advance_with<L: Launcher>(app: &AppState, key: &TaskKey, launch: &L) {
    let Ok(Some(task)) = app.store.get_task(key).await else {
        return;
    };
    let Some(a) = task.autopilot.as_ref().filter(|_| is_running(&task)) else {
        return;
    };
    if matches!(
        a.stage,
        Some(AutopilotStage::OpeningPr | AutopilotStage::Preview)
    ) && let Some(session) = a.review_session.clone()
    {
        let (app, key) = (app.clone(), key.clone());
        match app.finishing.try_guard(&key) {
            Some(guard) => {
                tokio::spawn(async move { finish_guarded(&app, &key, &session, guard).await });
            }
            // A finish is still running (maybe just about to pause): this resume must
            // not be lost, so it takes over once that finish lets go.
            None => {
                tokio::spawn(async move { finish_when_free(&app, &key).await });
            }
        }
        return;
    }
    let primary = app.sessions.primary(key.as_str());
    if primary
        .as_ref()
        .is_some_and(|l| l.state() == crate::sessions::LiveState::Working)
    {
        return;
    }
    let failed = match task.handoff.as_ref().map(|h| h.stage) {
        Some(HandoffStage::Failed) => crate::handoff::retry_with(app, key, launch).await.err(),
        Some(HandoffStage::Offered) => crate::handoff::confirm_with(app, key, launch).await.err(),
        // Saving notes: an idle session's save turn is over, so the handoff goes on. With
        // no live session the notes cannot be saved any more, and the handoff's own
        // recovery (`handoff::recover`, on the scheduler's tick) carries on without them.
        Some(HandoffStage::Saving) => {
            if primary.is_some() {
                crate::handoff::on_turn_ended_with(app, key, launch).await;
            }
            None
        }
        Some(HandoffStage::Starting) => None,
        _ => match primary.filter(|l| a.review_session.as_deref() == Some(l.id.as_str())) {
            // An idle review whose outcome still says blocked would only pause again
            // with the same reason: it is asked to check again instead.
            Some(review)
                if matches!(
                    read_outcome(app.lang(), &outcome_path(app, key, &review.id)),
                    Outcome::Blocked { .. }
                ) =>
            {
                recheck(app, key, &review).await.err()
            }
            _ => {
                crate::handoff::on_turn_ended_with(app, key, launch).await;
                None
            }
        },
    };
    if let Some(e) = failed {
        pause(app, key, e.message()).await;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::lang::Lang;

    use delilah_core::{AutopilotStage, AutopilotState, Event, Step};

    use crate::gh::fake::PR_LIST;

    use super::*;
    use crate::autopilot::testkit::*;
    use crate::events::Topic;
    use crate::store_sessions::SessionKind;

    #[tokio::test]
    async fn pausing_records_the_reason_emits_the_inbox_and_notifies_once() {
        let (app, key, _dir) = ready_task(gh("[]"), None).await;
        pause(&app, &key, "tests fail").await;
        let a = autopilot_of(&app, &key).await;
        assert_eq!(a.state, AutopilotState::Paused);
        assert_eq!(a.reason.as_deref(), Some("tests fail"));
        let events = app.store.events(&key).await.unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|e| e.event
                    == Event::AutopilotPaused {
                        reason: "tests fail".into()
                    })
                .count(),
            1
        );
        let sent = app.notifier.recorded();
        assert_eq!(sent.len(), 1);
        assert!(sent[0].body.contains("tests fail"));
        // Already paused: nothing more.
        pause(&app, &key, "other").await;
        assert_eq!(app.notifier.recorded().len(), 1);
        assert_eq!(
            autopilot_of(&app, &key).await.reason.as_deref(),
            Some("tests fail")
        );
        // The task's phase and status are untouched.
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        assert!(!task.status.is_terminal());
    }

    #[tokio::test]
    async fn pausing_emits_the_inbox_topic_and_stays_quiet_when_the_app_listens() {
        let (app, key, _dir) = ready_task(gh("[]"), None).await;
        let mut rx = app.events.subscribe();
        pause(&app, &key, "r").await;
        let mut topics = vec![];
        while let Ok(t) = rx.try_recv() {
            topics.push(t);
        }
        assert!(topics.contains(&Topic::Inbox), "{topics:?}");
        assert!(app.notifier.recorded().is_empty());
    }

    #[tokio::test]
    async fn resuming_a_paused_autopilot_that_is_not_paused_is_409_not_paused() {
        let (app, key, _dir) = ready_task(gh("[]"), None).await;
        let launched = Launched::default();
        let err = resume_with(&app, &key, &stub(&launched)).await.unwrap_err();
        assert_eq!(err.status, axum::http::StatusCode::CONFLICT);
        assert_eq!(err.reason.as_deref(), Some("not-paused"));
        // No autopilot at all is the same.
        app.store.set_autopilot(&key, None).await.unwrap();
        let err = resume_with(&app, &key, &stub(&launched)).await.unwrap_err();
        assert_eq!(err.reason.as_deref(), Some("not-paused"));
    }

    #[tokio::test]
    async fn resuming_an_idle_plan_with_tasks_starts_the_implementation() {
        let (app, key, _dir) = paused_task(gh("[]"), None).await;
        session(&app, "plan-1", Step::Plan).await;
        write_spec(&app, "- [ ] T001 One\n- [ ] T002 Two\n", false);
        let launched = Launched::default();
        resume_with(&app, &key, &stub(&launched))
            .await
            .ok()
            .unwrap();
        let a = autopilot_of(&app, &key).await;
        assert_eq!(
            (a.state, a.reason, a.nudged),
            (AutopilotState::Running, None, None)
        );
        {
            let launched = launched.lock().unwrap();
            assert_eq!(launched.len(), 1);
            assert_eq!(launched[0].0.step, Some(Step::Implement));
        }
        let events = app.store.events(&key).await.unwrap();
        assert!(event_names(&events).contains(&"autopilot-resumed"));
    }

    #[tokio::test]
    async fn resuming_an_idle_review_with_a_ready_outcome_finishes() {
        let gh = gh("[]");
        let (app, key, _dir) = ready_task(gh.clone(), None).await;
        app.store.set_saving_mode(&key, true).await.unwrap();
        write_spec(&app, "- [x] T001 One\n", true);
        write_outcome(&app, &key, r#"{"outcome":"ready"}"#);
        let mut a = autopilot_of(&app, &key).await;
        a.state = AutopilotState::Paused;
        app.store.set_autopilot(&key, Some(&a)).await.unwrap();
        let launched = Launched::default();
        resume_with(&app, &key, &stub(&launched))
            .await
            .ok()
            .unwrap();
        wait_for(&app, &key, AutopilotState::Done).await;
        assert_eq!(creates(&gh).len(), 1);
        assert!(launched.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn resuming_at_opening_pr_reopens_without_a_second_pr() {
        let gh = gh(PR_LIST);
        let (app, key, _dir) = paused_task(gh.clone(), Some(AutopilotStage::OpeningPr)).await;
        let launched = Launched::default();
        resume_with(&app, &key, &stub(&launched))
            .await
            .ok()
            .unwrap();
        wait_for(&app, &key, AutopilotState::Done).await;
        assert!(creates(&gh).is_empty());
        assert_eq!(app.store.prs_of(&key).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn resuming_while_the_session_works_only_marks_running() {
        let (app, key, _dir) = paused_task(gh("[]"), None).await;
        session(&app, "plan-1", Step::Plan).await;
        write_spec(&app, "- [ ] T001 One\n", false);
        let mut cmd = portable_pty::CommandBuilder::new("sleep");
        cmd.arg("30");
        let pty = delilah_agents::pty::PtySession::spawn(cmd, 80, 24, None).unwrap();
        app.sessions
            .insert("plan-1", "PROJ-1", SessionKind::Claude, true, pty);
        app.sessions
            .get("plan-1")
            .unwrap()
            .set_state(crate::sessions::LiveState::Working);
        let launched = Launched::default();
        resume_with(&app, &key, &stub(&launched))
            .await
            .ok()
            .unwrap();
        assert_eq!(
            autopilot_of(&app, &key).await.state,
            AutopilotState::Running
        );
        assert!(launched.lock().unwrap().is_empty());
    }

    fn spec(total: usize) -> SpecSummary {
        SpecSummary {
            phase: delilah_core::SpecPhase::Implement,
            dir: Some("specs/x".into()),
            done: 0,
            total,
            current: None,
        }
    }

    fn live(app: &AppState, id: &str) {
        let mut cmd = portable_pty::CommandBuilder::new("sleep");
        cmd.arg("30");
        let pty = delilah_agents::pty::PtySession::spawn(cmd, 80, 24, None).unwrap();
        app.sessions
            .insert(id, "PROJ-1", SessionKind::Claude, true, pty);
    }

    #[tokio::test]
    async fn resuming_a_blocked_review_asks_it_to_check_again() {
        let (app, key, _dir) = ready_task(gh("[]"), None).await;
        live(&app, "rev-1");
        write_outcome(&app, &key, r#"{"outcome":"blocked","reason":"tests fail"}"#);
        app.store
            .set_autopilot(&key, Some(&paused()))
            .await
            .unwrap();
        update(&app, &key, |a| {
            a.review_session = Some("rev-1".into());
            true
        })
        .await
        .unwrap();
        let launched = Launched::default();
        resume_with(&app, &key, &stub(&launched))
            .await
            .ok()
            .unwrap();
        let a = autopilot_of(&app, &key).await;
        assert_eq!(a.state, AutopilotState::Running, "{:?}", a.reason);
        let events = app.store.events(&key).await.unwrap();
        assert!(
            events
                .iter()
                .any(|e| e.event == Event::AutopilotNudged { step: Step::Review }),
            "{:?}",
            event_names(&events)
        );
        assert!(launched.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn two_stalls_at_once_nudge_once() {
        let (app, key, _dir) = ready_task(gh("[]"), None).await;
        app.lang.set(Lang::En);
        live(&app, "rev-1");
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        let spec = spec(2);
        tokio::join!(
            stall(&app, &key, &task, Some(Step::Implement), &spec),
            stall(&app, &key, &task, Some(Step::Implement), &spec),
        );
        let events = app.store.events(&key).await.unwrap();
        let nudges = events
            .iter()
            .filter(|e| {
                e.event
                    == Event::AutopilotNudged {
                        step: Step::Implement,
                    }
            })
            .count();
        assert_eq!(nudges, 1, "{:?}", event_names(&events));
        // The one that lost the nudge saw it taken: a second stall of the step pauses.
        let a = autopilot_of(&app, &key).await;
        assert_eq!(a.state, AutopilotState::Paused);
        assert_eq!(a.reason.as_deref(), Some("2 tasks unchecked"));
    }

    #[tokio::test]
    async fn a_stall_while_a_handoff_is_under_way_does_nothing() {
        let (app, key, _dir) = ready_task(gh("[]"), None).await;
        live(&app, "rev-1");
        let mut task = app.store.get_task(&key).await.unwrap().unwrap();
        task.handoff = Some(delilah_core::Handoff {
            to: Step::Review,
            stage: delilah_core::HandoffStage::Saving,
            error: None,
        });
        stall(&app, &key, &task, Some(Step::Implement), &spec(2)).await;
        let a = autopilot_of(&app, &key).await;
        assert_eq!((a.state, a.nudged), (AutopilotState::Running, None));
    }

    #[tokio::test]
    async fn a_resume_that_finds_the_finish_running_is_not_lost() {
        let gh = gh("[]");
        let (app, key, _dir) = paused_task(gh.clone(), Some(AutopilotStage::OpeningPr)).await;
        write_outcome(&app, &key, r#"{"outcome":"ready"}"#);
        // A finish is still winding down (about to pause) and holds the guard.
        let guard = app.finishing.try_guard(&key).unwrap();
        let launched = Launched::default();
        resume_with(&app, &key, &stub(&launched))
            .await
            .ok()
            .unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(
            creates(&gh).is_empty(),
            "nothing runs while the guard is held"
        );
        drop(guard);
        wait_for(&app, &key, AutopilotState::Done).await;
        assert_eq!(creates(&gh).len(), 1);
    }

    #[tokio::test]
    async fn resuming_with_a_saving_handoff_and_an_idle_session_goes_on() {
        let (app, key, _dir) = paused_task(gh("[]"), None).await;
        session(&app, "plan-1", Step::Plan).await;
        live(&app, "plan-1");
        write_spec(&app, "- [ ] T001 One\n", false);
        app.store
            .set_handoff(
                &key,
                Some(&delilah_core::Handoff {
                    to: Step::Implement,
                    stage: delilah_core::HandoffStage::Saving,
                    error: None,
                }),
            )
            .await
            .unwrap();
        let launched = Launched::default();
        resume_with(&app, &key, &stub(&launched))
            .await
            .ok()
            .unwrap();
        let launched = launched.lock().unwrap();
        assert_eq!(launched.len(), 1);
        assert_eq!(launched[0].0.step, Some(Step::Implement));
    }

    #[tokio::test]
    async fn resuming_with_a_saving_handoff_and_no_session_leaves_it_to_the_recovery() {
        let (app, key, _dir) = paused_task(gh("[]"), None).await;
        session(&app, "plan-1", Step::Plan).await;
        app.store
            .set_handoff(
                &key,
                Some(&delilah_core::Handoff {
                    to: Step::Implement,
                    stage: delilah_core::HandoffStage::Saving,
                    error: None,
                }),
            )
            .await
            .unwrap();
        let launched = Launched::default();
        resume_with(&app, &key, &stub(&launched))
            .await
            .ok()
            .unwrap();
        assert!(launched.lock().unwrap().is_empty());
        assert_eq!(
            autopilot_of(&app, &key).await.state,
            AutopilotState::Running
        );
    }

    #[tokio::test]
    async fn the_pause_notification_title_is_in_spanish_with_autopiloto() {
        let (app, key, _dir) = ready_task(gh("[]"), None).await;
        app.lang.set(Lang::Es);
        pause(&app, &key, "x").await;
        let sent = app.notifier.recorded();
        assert!(
            sent[0].title.contains("autopiloto en pausa"),
            "{}",
            sent[0].title
        );
    }
}
