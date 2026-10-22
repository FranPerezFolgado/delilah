//! Autopilot: the chain of a task (plan, implement, review, PR) runs by itself. It
//! rides on saving mode's handoffs (`handoff.rs`): this module only changes who
//! confirms a handoff (`shape`), what each session is told (`notes`), and what
//! happens after the review (`finish`: read the outcome file, open the draft PRs).
//! The state lives in `tasks.autopilot`, written before each side effect, so a
//! restart repeats nothing (`finish::recover`). Every change of that state goes
//! through `update`, a compare-and-set, so a write never overwrites a newer one (an
//! `off` or a pause that came in meanwhile). See `specs/005-autopilot`.
//!
//! - `notes`: what the sessions are told, and where the outcome file is.
//! - `outcome`: reading the review's outcome file.
//! - `pause`: pause, stall, resume, advance.
//! - `ask`: answering questions with the recommended option.
//! - `finish`: the finish pipeline (preview, PRs) and its recovery.
//! - `api`: the HTTP route.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use delilah_core::{Autopilot, AutopilotState, Task, TaskKey};

use crate::api::ApiError;
use crate::events::Topic;
use crate::handoff::Boundary;
use crate::state::AppState;
use crate::store::StoreError;

mod api;
mod ask;
mod finish;
pub(crate) mod notes;
mod outcome;
mod pause;
#[cfg(test)]
mod testkit;

pub(crate) use api::act;
pub(crate) use ask::answer_question;
pub(crate) use finish::{finish, recover};
pub(crate) use notes::{autopilot_plan_note, outcome_path, prepare_step};
pub(crate) use pause::{on_attention, pause, stall};

/// Whether the task is on autopilot and not paused or done.
pub(crate) fn is_running(task: &Task) -> bool {
    matches!(&task.autopilot, Some(a) if a.state == AutopilotState::Running)
}

/// Whether `key` is a task (not a project's owner, which has no autopilot): the check
/// the entry points make before looking at a task.
fn is_task(key: &TaskKey) -> bool {
    crate::store_sessions::project_of(key.as_str()).is_none()
}

/// What a boundary becomes on autopilot: the offer of the implementation is taken on
/// the user's behalf. Paused, done or off, the boundary is left as it is.
pub(crate) fn shape(task: &Task, found: Option<Boundary>) -> Option<Boundary> {
    match found {
        Some(Boundary::Offer(to)) if is_running(task) => Some(Boundary::Start(to)),
        other => other,
    }
}

/// Changes the autopilot of `key` as one compare-and-set: `f` edits the stored value
/// and says whether to keep the edit (it runs again on a fresher value if something
/// else wrote first). `true` if it was written; no autopilot is never written.
async fn update(
    app: &AppState,
    key: &TaskKey,
    mut f: impl FnMut(&mut Autopilot) -> bool,
) -> Result<bool, StoreError> {
    let written = app
        .store
        .update_autopilot(key, |current| {
            let mut a = current?;
            f(&mut a).then_some(Some(a))
        })
        .await?;
    if written {
        app.emit(Topic::Tasks);
    }
    Ok(written)
}

async fn task_of(app: &AppState, key: &TaskKey) -> Result<Task, ApiError> {
    app.store
        .get_task(key)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("no task {key}")))
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

#[cfg(test)]
mod tests {

    use std::sync::Arc;

    use delilah_core::{AutopilotState, Step, TaskKey};

    use crate::handoff::Boundary;

    use super::*;
    use crate::autopilot::testkit::*;

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
}
