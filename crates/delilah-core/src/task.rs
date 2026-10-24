use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Jira key of the ticket a task works on, e.g. `PROJ-1234`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskKey(String);

impl TaskKey {
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    /// A key from outside (the API): letters, digits, `-` and `_`, up to 64. It names
    /// folders and goes into the agents' hook commands, so nothing else gets in.
    pub fn parse(key: impl Into<String>) -> Result<Self, InvalidKey> {
        let key = key.into();
        let valid = !key.is_empty()
            && key.len() <= 64
            && key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if valid {
            Ok(Self(key))
        } else {
            Err(InvalidKey(key))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{0}` is not a valid ticket key (letters, digits, - and _)")]
pub struct InvalidKey(pub String);

impl fmt::Display for TaskKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where a task is in its lifecycle. The workflow phase (`analysis`,
/// `implement`…) is tracked separately in [`Task::phase`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum TaskStatus {
    Queued,
    PreparingWorkspace,
    Running,
    /// The agent asked something and is blocked until the user answers.
    WaitingForUser,
    /// The agent hit its usage limit; the session is kept and resumes at `resume_at`.
    PausedByLimit {
        resume_at: DateTime<Utc>,
    },
    Done,
    Failed {
        reason: String,
    },
    /// The project's setup command failed (or timed out); the worktree is kept and
    /// the user can retry the setup or launch without it.
    SetupFailed {
        reason: String,
    },
}

impl TaskStatus {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::PreparingWorkspace => "preparing-workspace",
            Self::Running => "running",
            Self::WaitingForUser => "waiting-for-user",
            Self::PausedByLimit { .. } => "paused-by-limit",
            Self::Done => "done",
            Self::Failed { .. } => "failed",
            Self::SetupFailed { .. } => "setup-failed",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Done | Self::Failed { .. })
    }

    /// Whether its workspace may be archived: finished, or stopped for good on a
    /// failed setup.
    pub fn can_archive(&self) -> bool {
        self.is_terminal() || matches!(self, Self::SetupFailed { .. })
    }
}

/// Something that happened to a task, reported by the daemon, an agent hook or the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Event {
    Start,
    WorkspaceReady,
    /// The workflow moved to another phase; the status does not change.
    PhaseReached {
        phase: String,
    },
    AgentAsked,
    UserAnswered,
    LimitReached {
        resume_at: DateTime<Utc>,
    },
    /// The scheduler's tick: resumes a paused task once its reset time has passed.
    ResetDue {
        now: DateTime<Utc>,
    },
    /// The user resumes a paused task before the reset (e.g. with another agent).
    ManualResume,
    AgentFinished,
    /// The user declares the task finished, whatever the agent is doing.
    MarkDone,
    Fail {
        reason: String,
    },
    /// A backlog task could not get its workspace: it goes back to the backlog.
    StartAborted {
        reason: String,
    },
    /// The setup command failed while preparing the workspace.
    SetupFailed {
        reason: String,
    },
    /// The user runs the setup again after a failure.
    RetrySetup,
    /// The user launches the agent without running the setup.
    SkipSetup,
    /// The task was created from a conversation with an agent; nothing else changes.
    CreatedFromConversation {
        session: String,
        label: String,
    },
    /// A saving-mode handoff started the session of a step; the status does not change.
    StepStarted {
        step: Step,
        session: String,
    },
    /// Autopilot paused itself; the status does not change.
    AutopilotPaused {
        reason: String,
    },
    /// The user resumed autopilot; the status does not change.
    AutopilotResumed,
    /// Autopilot answered a question with its recommended option.
    AutopilotAnswered {
        question: String,
        answer: String,
    },
    /// Autopilot nudged a stalled step (once per step).
    AutopilotNudged {
        step: Step,
    },
    /// Autopilot opened the PRs (URLs) and is done.
    AutopilotFinished {
        prs: Vec<String>,
    },
}

impl Event {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::WorkspaceReady => "workspace-ready",
            Self::PhaseReached { .. } => "phase-reached",
            Self::AgentAsked => "agent-asked",
            Self::UserAnswered => "user-answered",
            Self::LimitReached { .. } => "limit-reached",
            Self::ResetDue { .. } => "reset-due",
            Self::ManualResume => "manual-resume",
            Self::AgentFinished => "agent-finished",
            Self::MarkDone => "mark-done",
            Self::Fail { .. } => "fail",
            Self::StartAborted { .. } => "start-aborted",
            Self::SetupFailed { .. } => "setup-failed",
            Self::RetrySetup => "retry-setup",
            Self::SkipSetup => "skip-setup",
            Self::CreatedFromConversation { .. } => "created-from-conversation",
            Self::StepStarted { .. } => "step-started",
            Self::AutopilotPaused { .. } => "autopilot-paused",
            Self::AutopilotResumed => "autopilot-resumed",
            Self::AutopilotAnswered { .. } => "autopilot-answered",
            Self::AutopilotNudged { .. } => "autopilot-nudged",
            Self::AutopilotFinished { .. } => "autopilot-finished",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransitionError {
    #[error("cannot apply `{event}` to a task that is {status}")]
    Invalid {
        status: &'static str,
        event: &'static str,
    },
    #[error("the usage limit resets at {resume_at}; it is only {now}")]
    NotYetDue {
        resume_at: DateTime<Utc>,
        now: DateTime<Utc>,
    },
}

/// A step of the saving-mode chain; each one runs in its own Claude session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Step {
    Plan,
    Implement,
    Review,
}

impl Step {
    pub fn name(self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::Implement => "implement",
            Self::Review => "review",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "plan" => Some(Self::Plan),
            "implement" => Some(Self::Implement),
            "review" => Some(Self::Review),
            _ => None,
        }
    }

    /// The step after this one; `None` after the last.
    pub fn next(self) -> Option<Self> {
        match self {
            Self::Plan => Some(Self::Implement),
            Self::Implement => Some(Self::Review),
            Self::Review => None,
        }
    }
}

/// How far a handoff to the next step has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HandoffStage {
    Offered,
    Saving,
    Starting,
    Failed,
    Skipped,
}

/// The handoff under way: the step it goes to and where it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Handoff {
    pub to: Step,
    pub stage: HandoffStage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Where a task's title came from: decides whether it may still be renamed
/// automatically (`initial` only; `manual` is terminal).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TitleSource {
    /// The placeholder given at creation; the first analysis may replace it.
    Initial,
    /// Taken from the task's spec.
    Spec,
    /// Set by the user, or a task that predates automatic names (Jira, old rows).
    #[default]
    Manual,
}

impl TitleSource {
    pub fn name(self) -> &'static str {
        match self {
            Self::Initial => "initial",
            Self::Spec => "spec",
            Self::Manual => "manual",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "initial" => Some(Self::Initial),
            "spec" => Some(Self::Spec),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }
}

/// Whether autopilot is moving, paused or finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AutopilotState {
    Running,
    Paused,
    Done,
}

/// Where autopilot's final actions are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutopilotStage {
    Preview,
    OpeningPr,
}

/// Autopilot of a task: the chain runs by itself until the PRs are open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Autopilot {
    pub state: AutopilotState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<AutopilotStage>,
    /// Why it paused (user-facing).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// When `state` last changed.
    pub since: DateTime<Utc>,
    /// The step that already got its one nudge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nudged: Option<Step>,
    /// The review session whose outcome file the finish reads; recorded when the
    /// review starts, so a restart reads the right file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_session: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub key: TaskKey,
    pub title: String,
    /// Name of the project the task runs against, if it has a workspace.
    #[serde(default)]
    pub project: Option<String>,
    /// Name of the workflow that drives the task, e.g. `feature`.
    pub workflow: String,
    pub phase: Option<String>,
    pub status: TaskStatus,
    /// What the user keeps with the task (free text); handed to the agent on start.
    #[serde(default)]
    pub notes: Option<String>,
    /// Where it will work once started, chosen while in the backlog: the project's
    /// checkout (`true`) or a worktree (`false`); `None` if not chosen yet.
    #[serde(default)]
    pub in_checkout: Option<bool>,
    /// The branch the task works on, once it has a workspace (read from it; never set by the client).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Saving mode: each phase runs in a fresh Claude session.
    #[serde(default)]
    pub saving_mode: bool,
    /// The handoff to the next step under way, if any.
    #[serde(default)]
    pub handoff: Option<Handoff>,
    /// Where `title` came from.
    #[serde(default)]
    pub title_source: TitleSource,
    /// The title an automatic rename replaced, so the user can restore it.
    #[serde(default)]
    pub previous_title: Option<String>,
    /// Why the task's branch couldn't be created; cleared when it is.
    #[serde(default)]
    pub branch_error: Option<String>,
    /// Autopilot, if on (`None` = off).
    #[serde(default)]
    pub autopilot: Option<Autopilot>,
}

impl Task {
    pub fn new(key: TaskKey, title: impl Into<String>, workflow: impl Into<String>) -> Self {
        Self {
            key,
            title: title.into(),
            project: None,
            workflow: workflow.into(),
            phase: None,
            status: TaskStatus::Queued,
            notes: None,
            in_checkout: None,
            branch: None,
            saving_mode: false,
            handoff: None,
            title_source: TitleSource::Manual,
            previous_title: None,
            branch_error: None,
            autopilot: None,
        }
    }

    /// Applies `event`, leaving the task untouched if the transition is not allowed.
    pub fn apply(&mut self, event: Event) -> Result<(), TransitionError> {
        if let Event::PhaseReached { phase } = event {
            // Any task under way, paused ones too (the user may move it by hand).
            if !matches!(
                self.status,
                TaskStatus::Running | TaskStatus::WaitingForUser | TaskStatus::PausedByLimit { .. }
            ) {
                return Err(self.invalid("phase-reached"));
            }
            self.phase = Some(phase);
            return Ok(());
        }
        if matches!(
            event,
            Event::StepStarted { .. }
                | Event::AutopilotPaused { .. }
                | Event::AutopilotResumed
                | Event::AutopilotAnswered { .. }
                | Event::AutopilotNudged { .. }
                | Event::AutopilotFinished { .. }
        ) {
            return Ok(());
        }
        self.status = next_status(&self.status, event)?;
        Ok(())
    }

    fn invalid(&self, event: &'static str) -> TransitionError {
        TransitionError::Invalid {
            status: self.status.name(),
            event,
        }
    }
}

fn next_status(status: &TaskStatus, event: Event) -> Result<TaskStatus, TransitionError> {
    use TaskStatus as S;

    let invalid = TransitionError::Invalid {
        status: status.name(),
        event: event.name(),
    };
    match (status, event) {
        (S::Queued, Event::Start) => Ok(S::PreparingWorkspace),
        (S::PreparingWorkspace, Event::WorkspaceReady) => Ok(S::Running),
        (S::Running, Event::AgentAsked) => Ok(S::WaitingForUser),
        (S::WaitingForUser, Event::UserAnswered) => Ok(S::Running),
        (S::Running | S::WaitingForUser, Event::LimitReached { resume_at }) => {
            Ok(S::PausedByLimit { resume_at })
        }
        (S::PausedByLimit { resume_at }, Event::ResetDue { now }) => {
            if now >= *resume_at {
                Ok(S::Running)
            } else {
                Err(TransitionError::NotYetDue {
                    resume_at: *resume_at,
                    now,
                })
            }
        }
        (S::PausedByLimit { .. }, Event::ManualResume) => Ok(S::Running),
        (S::Running, Event::AgentFinished) => Ok(S::Done),
        (
            S::Running | S::WaitingForUser | S::PausedByLimit { .. } | S::SetupFailed { .. },
            Event::MarkDone,
        ) => Ok(S::Done),
        (S::PreparingWorkspace, Event::SetupFailed { reason }) => Ok(S::SetupFailed { reason }),
        (S::SetupFailed { .. }, Event::RetrySetup | Event::SkipSetup) => Ok(S::PreparingWorkspace),
        (s, Event::Fail { reason }) if !s.is_terminal() => Ok(S::Failed { reason }),
        (S::PreparingWorkspace, Event::StartAborted { .. }) => Ok(S::Queued),
        (S::Queued, Event::CreatedFromConversation { .. }) => Ok(S::Queued),
        _ => Err(invalid),
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn at(hour: u32, min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 19, hour, min, 0).unwrap()
    }

    #[test]
    fn keys_from_outside_are_plain_names() {
        assert!(TaskKey::parse("PROJ-1234").is_ok());
        assert!(TaskKey::parse("free_7").is_ok());
        for bad in ["", "../etc", "/tmp/x", "A B", "x'y", "a/b", &"x".repeat(65)] {
            assert!(TaskKey::parse(bad).is_err(), "{bad} was accepted");
        }
    }

    fn running_task() -> Task {
        let mut task = Task::new(TaskKey::new("PROJ-1234"), "Card retries", "feature");
        task.apply(Event::Start).unwrap();
        task.apply(Event::WorkspaceReady).unwrap();
        task
    }

    #[test]
    fn created_from_conversation_is_recorded_on_a_queued_task_only() {
        let event = Event::CreatedFromConversation {
            session: "s1".into(),
            label: "Idea chat".into(),
        };
        let mut task = Task::new(TaskKey::new("PROJ-1"), "Card retries", "feature");
        let before = task.clone();
        task.apply(event.clone()).unwrap();
        assert_eq!(task, before);
        assert_eq!(event.name(), "created-from-conversation");

        let mut running = running_task();
        assert!(running.apply(event.clone()).is_err());
        assert_eq!(running.status, TaskStatus::Running);
    }

    #[test]
    fn an_aborted_start_goes_back_to_the_backlog() {
        let mut task = Task::new(TaskKey::new("PROJ-1"), "Card retries", "feature");
        task.apply(Event::Start).unwrap();
        let aborted = Event::StartAborted {
            reason: "workspace: branch exists".into(),
        };
        task.apply(aborted.clone()).unwrap();
        assert_eq!(task.status, TaskStatus::Queued);

        // Only a start being prepared can be aborted.
        assert!(task.apply(aborted.clone()).is_err());
        assert!(running_task().apply(aborted).is_err());

        // And it can be started again.
        task.apply(Event::Start).unwrap();
        task.apply(Event::WorkspaceReady).unwrap();
        assert_eq!(task.status, TaskStatus::Running);
    }

    #[test]
    fn a_setup_failure_can_be_retried_skipped_failed_or_marked_done() {
        let failed = |reason: &str| Event::SetupFailed {
            reason: reason.into(),
        };
        let mut task = Task::new(TaskKey::new("PROJ-1"), "Card retries", "feature");
        task.apply(Event::Start).unwrap();
        task.apply(failed("boom")).unwrap();
        assert_eq!(
            task.status,
            TaskStatus::SetupFailed {
                reason: "boom".into()
            }
        );
        assert_eq!(task.status.name(), "setup-failed");
        assert!(!task.status.is_terminal());

        task.apply(Event::RetrySetup).unwrap();
        assert_eq!(task.status, TaskStatus::PreparingWorkspace);
        task.apply(failed("again")).unwrap();
        task.apply(Event::SkipSetup).unwrap();
        assert_eq!(task.status, TaskStatus::PreparingWorkspace);
        task.apply(Event::WorkspaceReady).unwrap();
        assert_eq!(task.status, TaskStatus::Running);

        for end in [
            Event::Fail {
                reason: "gave up".into(),
            },
            Event::MarkDone,
        ] {
            let mut t = Task::new(TaskKey::new("PROJ-2"), "x", "feature");
            t.apply(Event::Start).unwrap();
            t.apply(failed("boom")).unwrap();
            t.apply(end).unwrap();
            assert!(t.status.is_terminal());
        }
    }

    #[test]
    fn only_finished_or_setup_failed_tasks_can_be_archived() {
        assert!(TaskStatus::Done.can_archive());
        assert!(TaskStatus::Failed { reason: "x".into() }.can_archive());
        assert!(TaskStatus::SetupFailed { reason: "x".into() }.can_archive());
        assert!(!TaskStatus::Running.can_archive());
        assert!(!TaskStatus::PreparingWorkspace.can_archive());
        assert!(!TaskStatus::Queued.can_archive());
    }

    #[test]
    fn setup_events_are_refused_from_other_statuses() {
        let events = [
            Event::SetupFailed { reason: "x".into() },
            Event::RetrySetup,
            Event::SkipSetup,
        ];
        for event in &events {
            assert!(running_task().apply(event.clone()).is_err());
            assert!(
                Task::new(TaskKey::new("PROJ-3"), "x", "feature")
                    .apply(event.clone())
                    .is_err()
            );
        }
        // Retry/skip only make sense after a failure, not while preparing.
        let mut t = Task::new(TaskKey::new("PROJ-4"), "x", "feature");
        t.apply(Event::Start).unwrap();
        assert!(t.apply(Event::RetrySetup).is_err());
        assert!(t.apply(Event::SkipSetup).is_err());
    }

    #[test]
    fn happy_path_reaches_done() {
        let mut task = running_task();
        assert_eq!(task.status, TaskStatus::Running);

        task.apply(Event::AgentFinished).unwrap();
        assert_eq!(task.status, TaskStatus::Done);
    }

    #[test]
    fn question_blocks_until_the_user_answers() {
        let mut task = running_task();
        task.apply(Event::AgentAsked).unwrap();
        assert_eq!(task.status, TaskStatus::WaitingForUser);

        let err = task.apply(Event::AgentFinished).unwrap_err();
        assert_eq!(
            err,
            TransitionError::Invalid {
                status: "waiting-for-user",
                event: "agent-finished"
            }
        );

        task.apply(Event::UserAnswered).unwrap();
        assert_eq!(task.status, TaskStatus::Running);
    }

    #[test]
    fn limit_pauses_and_resumes_once_the_reset_has_passed() {
        let mut task = running_task();
        task.apply(Event::LimitReached {
            resume_at: at(15, 0),
        })
        .unwrap();
        assert_eq!(
            task.status,
            TaskStatus::PausedByLimit {
                resume_at: at(15, 0)
            }
        );

        let err = task.apply(Event::ResetDue { now: at(14, 59) }).unwrap_err();
        assert!(matches!(err, TransitionError::NotYetDue { .. }));
        assert_eq!(task.status.name(), "paused-by-limit");

        task.apply(Event::ResetDue { now: at(15, 0) }).unwrap();
        assert_eq!(task.status, TaskStatus::Running);
    }

    #[test]
    fn paused_task_can_be_resumed_by_hand() {
        let mut task = running_task();
        task.apply(Event::LimitReached {
            resume_at: at(15, 0),
        })
        .unwrap();

        task.apply(Event::ManualResume).unwrap();
        assert_eq!(task.status, TaskStatus::Running);
    }

    #[test]
    fn limit_pauses_a_task_waiting_for_the_user_too_but_not_a_queued_one() {
        let mut task = running_task();
        task.apply(Event::AgentAsked).unwrap();
        task.apply(Event::LimitReached {
            resume_at: at(15, 0),
        })
        .unwrap();
        assert_eq!(task.status.name(), "paused-by-limit");

        let mut queued = Task::new(TaskKey::new("PROJ-9"), "t", "feature");
        let err = queued
            .apply(Event::LimitReached {
                resume_at: at(15, 0),
            })
            .unwrap_err();
        assert!(matches!(err, TransitionError::Invalid { .. }));
    }

    #[test]
    fn the_user_can_mark_an_active_task_done() {
        for prepare in [
            Event::AgentAsked,
            Event::LimitReached {
                resume_at: at(15, 0),
            },
        ] {
            let mut task = running_task();
            task.apply(prepare).unwrap();
            task.apply(Event::MarkDone).unwrap();
            assert_eq!(task.status, TaskStatus::Done);
        }
        let mut queued = Task::new(TaskKey::new("PROJ-9"), "t", "feature");
        assert!(queued.apply(Event::MarkDone).is_err());
    }

    #[test]
    fn any_active_task_can_fail_but_finished_ones_cannot() {
        let mut task = Task::new(TaskKey::new("PROJ-1"), "t", "feature");
        task.apply(Event::Fail {
            reason: "no workspace".into(),
        })
        .unwrap();
        assert_eq!(
            task.status,
            TaskStatus::Failed {
                reason: "no workspace".into()
            }
        );

        let mut done = running_task();
        done.apply(Event::AgentFinished).unwrap();
        assert!(
            done.apply(Event::Fail {
                reason: "late".into()
            })
            .is_err()
        );
        assert_eq!(done.status, TaskStatus::Done);
    }

    #[test]
    fn phase_changes_keep_the_status() {
        let mut task = running_task();
        task.apply(Event::PhaseReached {
            phase: "implement".into(),
        })
        .unwrap();
        assert_eq!(task.phase.as_deref(), Some("implement"));
        assert_eq!(task.status, TaskStatus::Running);

        let mut queued = Task::new(TaskKey::new("PROJ-2"), "t", "feature");
        assert!(
            queued
                .apply(Event::PhaseReached {
                    phase: "analysis".into()
                })
                .is_err()
        );
        assert_eq!(queued.phase, None);
    }

    #[test]
    fn title_source_is_lowercase_and_defaults_to_manual() {
        assert_eq!(TitleSource::default(), TitleSource::Manual);
        for (source, text) in [
            (TitleSource::Initial, "\"initial\""),
            (TitleSource::Spec, "\"spec\""),
            (TitleSource::Manual, "\"manual\""),
        ] {
            assert_eq!(serde_json::to_string(&source).unwrap(), text);
            assert_eq!(serde_json::from_str::<TitleSource>(text).unwrap(), source);
        }
    }

    #[test]
    fn a_task_without_the_new_fields_reads_with_defaults() {
        let json = r#"{"key":"PROJ-1","title":"t","workflow":"feature","phase":null,"status":{"type":"queued"}}"#;
        let task: Task = serde_json::from_str(json).unwrap();
        assert_eq!(task.title_source, TitleSource::Manual);
        assert_eq!(task.previous_title, None);
        assert_eq!(task.branch_error, None);
        assert_eq!(
            Task::new(TaskKey::new("PROJ-1"), "t", "feature").title_source,
            TitleSource::Manual
        );
    }

    #[test]
    fn invalid_event_leaves_the_task_untouched() {
        let mut task = Task::new(TaskKey::new("PROJ-3"), "t", "feature");
        let before = task.clone();

        assert!(task.apply(Event::AgentFinished).is_err());
        assert_eq!(task, before);
    }

    #[test]
    fn steps_chain_plan_implement_review() {
        assert_eq!(Step::Plan.next(), Some(Step::Implement));
        assert_eq!(Step::Implement.next(), Some(Step::Review));
        assert_eq!(Step::Review.next(), None);
        assert_eq!(Step::parse("implement"), Some(Step::Implement));
        assert_eq!(Step::parse("nope"), None);
    }

    #[test]
    fn steps_and_stages_serialise_lowercase() {
        assert_eq!(serde_json::to_string(&Step::Review).unwrap(), "\"review\"");
        assert_eq!(
            serde_json::to_string(&HandoffStage::Saving).unwrap(),
            "\"saving\""
        );
    }

    #[test]
    fn a_handoff_round_trips_without_an_empty_error() {
        let handoff = Handoff {
            to: Step::Implement,
            stage: HandoffStage::Offered,
            error: None,
        };
        let json = serde_json::to_string(&handoff).unwrap();
        assert_eq!(json, r#"{"to":"implement","stage":"offered"}"#);
        assert_eq!(serde_json::from_str::<Handoff>(&json).unwrap(), handoff);
    }

    #[test]
    fn old_tasks_load_without_saving_mode_or_handoff() {
        let json = r#"{"key":"PROJ-1","title":"t","workflow":"feature","phase":null,
            "status":{"type":"queued"}}"#;
        let task: Task = serde_json::from_str(json).unwrap();
        assert!(!task.saving_mode);
        assert_eq!(task.handoff, None);
    }

    #[test]
    fn a_started_step_serialises_with_its_name() {
        let event = Event::StepStarted {
            step: Step::Implement,
            session: "s-2".into(),
        };
        assert_eq!(
            serde_json::to_string(&event).unwrap(),
            r#"{"type":"step-started","step":"implement","session":"s-2"}"#
        );
    }

    #[test]
    fn starting_a_step_keeps_every_status() {
        let event = Event::StepStarted {
            step: Step::Implement,
            session: "s-2".into(),
        };
        let mut paused = running_task();
        paused
            .apply(Event::LimitReached {
                resume_at: at(15, 0),
            })
            .unwrap();
        for mut task in [
            running_task(),
            paused,
            Task::new(TaskKey::new("PROJ-5"), "t", "feature"),
        ] {
            let before = task.clone();
            task.apply(event.clone()).unwrap();
            assert_eq!(task, before);
        }
    }

    #[test]
    fn autopilot_round_trips() {
        let a = Autopilot {
            state: AutopilotState::Paused,
            stage: None,
            reason: Some("stalled".into()),
            since: at(10, 0),
            nudged: None,
            review_session: None,
        };
        let json = serde_json::to_string(&a).unwrap();
        assert_eq!(
            json,
            r#"{"state":"paused","reason":"stalled","since":"2026-10-19T10:00:00Z"}"#
        );
        assert_eq!(serde_json::from_str::<Autopilot>(&json).unwrap(), a);
        let b = Autopilot {
            state: AutopilotState::Running,
            stage: Some(AutopilotStage::OpeningPr),
            reason: None,
            since: at(10, 0),
            nudged: Some(Step::Review),
            review_session: Some("rev-1".into()),
        };
        let json = serde_json::to_string(&b).unwrap();
        assert!(json.contains(r#""stage":"opening_pr""#) && json.contains(r#""nudged":"review""#));
        assert!(json.contains(r#""review_session":"rev-1""#));
        assert_eq!(serde_json::from_str::<Autopilot>(&json).unwrap(), b);
        assert_eq!(
            serde_json::to_string(&AutopilotStage::Preview).unwrap(),
            "\"preview\""
        );
        assert_eq!(
            serde_json::to_string(&AutopilotState::Done).unwrap(),
            "\"done\""
        );
    }

    #[test]
    fn old_tasks_load_without_autopilot() {
        let json = r#"{"key":"PROJ-1","title":"t","workflow":"feature","phase":null,
            "status":{"type":"queued"}}"#;
        let task: Task = serde_json::from_str(json).unwrap();
        assert_eq!(task.autopilot, None);
    }

    fn autopilot_events() -> Vec<(Event, &'static str)> {
        vec![
            (
                Event::AutopilotPaused { reason: "r".into() },
                "autopilot-paused",
            ),
            (Event::AutopilotResumed, "autopilot-resumed"),
            (
                Event::AutopilotAnswered {
                    question: "q".into(),
                    answer: "a".into(),
                },
                "autopilot-answered",
            ),
            (
                Event::AutopilotNudged { step: Step::Plan },
                "autopilot-nudged",
            ),
            (
                Event::AutopilotFinished {
                    prs: vec!["u".into()],
                },
                "autopilot-finished",
            ),
        ]
    }

    #[test]
    fn autopilot_events_serialise_kebab_case() {
        for (event, name) in autopilot_events() {
            let json = serde_json::to_value(&event).unwrap();
            assert_eq!(json["type"], name);
            assert_eq!(event.name(), name);
            assert_eq!(serde_json::from_value::<Event>(json).unwrap(), event);
        }
    }

    #[test]
    fn autopilot_events_keep_every_status() {
        let mut paused = running_task();
        paused
            .apply(Event::LimitReached {
                resume_at: at(15, 0),
            })
            .unwrap();
        for (event, _) in autopilot_events() {
            for mut task in [
                running_task(),
                paused.clone(),
                Task::new(TaskKey::new("PROJ-5"), "t", "feature"),
            ] {
                let before = task.clone();
                task.apply(event.clone()).unwrap();
                assert_eq!(task, before);
            }
        }
    }
}
