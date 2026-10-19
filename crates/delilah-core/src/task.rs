use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Jira key of the ticket a task works on, e.g. `PROJ-1234`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskKey(String);

impl TaskKey {
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TaskKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where a task is in its lifecycle. The workflow phase (`analysis`,
/// `implement`…) is tracked separately in [`Task::phase`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Done | Self::Failed { .. })
    }
}

/// Something that happened to a task, reported by the daemon, an agent hook or the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    Fail {
        reason: String,
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
            Self::Fail { .. } => "fail",
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub key: TaskKey,
    pub title: String,
    /// Name of the workflow that drives the task, e.g. `feature`.
    pub workflow: String,
    pub phase: Option<String>,
    pub status: TaskStatus,
}

impl Task {
    pub fn new(key: TaskKey, title: impl Into<String>, workflow: impl Into<String>) -> Self {
        Self {
            key,
            title: title.into(),
            workflow: workflow.into(),
            phase: None,
            status: TaskStatus::Queued,
        }
    }

    /// Applies `event`, leaving the task untouched if the transition is not allowed.
    pub fn apply(&mut self, event: Event) -> Result<(), TransitionError> {
        if let Event::PhaseReached { phase } = event {
            if !matches!(
                self.status,
                TaskStatus::Running | TaskStatus::WaitingForUser
            ) {
                return Err(self.invalid("phase-reached"));
            }
            self.phase = Some(phase);
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
        (S::Running, Event::LimitReached { resume_at }) => Ok(S::PausedByLimit { resume_at }),
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
        (s, Event::Fail { reason }) if !s.is_terminal() => Ok(S::Failed { reason }),
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

    fn running_task() -> Task {
        let mut task = Task::new(TaskKey::new("PROJ-1234"), "Card retries", "feature");
        task.apply(Event::Start).unwrap();
        task.apply(Event::WorkspaceReady).unwrap();
        task
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
    fn limit_is_only_reported_while_running() {
        let mut task = running_task();
        task.apply(Event::AgentAsked).unwrap();

        let err = task
            .apply(Event::LimitReached {
                resume_at: at(15, 0),
            })
            .unwrap_err();
        assert!(matches!(err, TransitionError::Invalid { .. }));
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
    fn invalid_event_leaves_the_task_untouched() {
        let mut task = Task::new(TaskKey::new("PROJ-3"), "t", "feature");
        let before = task.clone();

        assert!(task.apply(Event::AgentFinished).is_err());
        assert_eq!(task, before);
    }
}
