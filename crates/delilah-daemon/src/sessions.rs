//! Sessions alive in this daemon right now, by session id.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use delilah_agents::runtime::Runtime;
use serde::Serialize;
use tokio::sync::{Notify, watch};

use crate::events::{Events, Topic};
use crate::progress::Progress;
use crate::store_sessions::SessionKind;

/// What a live session is doing, from its own hooks. The task's status follows the
/// primary session; this is what an extra session shows next to its terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LiveState {
    Starting,
    Working,
    Waiting,
    /// Its last turn ended on the usage limit; resumed with the task.
    Paused,
}

/// What a conversation's view shows of the turn in progress.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConvoState {
    /// What the agent is doing right now.
    pub activity: Option<String>,
    /// Claude waits on something only its terminal can answer (a permission prompt).
    pub needs_terminal: bool,
}

/// The live progress and the Stop hook held back while background work runs, under one
/// lock so a Stop is never held after the last background item ended.
#[derive(Default)]
struct ProgressState {
    progress: Progress,
    held_stop: Option<serde_json::Value>,
    /// Cancels the timer that replays the held Stop if no follow-up turn comes.
    fallback: Option<Arc<AtomicBool>>,
}

#[derive(Clone)]
pub struct Live {
    pub id: String,
    pub task: String,
    pub kind: SessionKind,
    pub primary: bool,
    pub runtime: Runtime,
    /// Fired by the agent's SessionStart hook.
    pub started: Arc<Notify>,
    /// Clients attached to the terminal right now.
    pub viewers: Arc<AtomicUsize>,
    pub state: Arc<Mutex<LiveState>>,
    /// An idea conversation (cached from its record, so a hook needs no store read).
    pub conversation: Arc<AtomicBool>,
    /// What a conversation's view shows of the turn in progress.
    pub convo: Arc<Mutex<ConvoState>>,
    /// Where Claude saves this session's transcript, as its latest hook said.
    transcript: Arc<Mutex<Option<PathBuf>>>,
    /// Progress of the turn, background work and sub-agents, plus the held Stop.
    progress: Arc<Mutex<ProgressState>>,
    /// Snapshots of `progress`, sent whenever it changes.
    progress_tx: Arc<watch::Sender<Progress>>,
    /// Held while a message is decided on and typed, so two never interleave.
    pub typing: Arc<tokio::sync::Mutex<()>>,
    events: Events,
}

impl Live {
    /// Whether what a viewer types reaches the process: not for a setup's output.
    pub fn accepts_input(&self) -> bool {
        self.kind != SessionKind::Setup
    }

    pub fn state(&self) -> LiveState {
        *self.state.lock().unwrap()
    }

    pub fn set_state(&self, state: LiveState) {
        let changed = std::mem::replace(&mut *self.state.lock().unwrap(), state) != state;
        if changed {
            self.events.emit(Topic::Sessions);
        }
    }

    pub fn is_idea(&self) -> bool {
        self.conversation.load(Ordering::Relaxed)
    }

    pub fn set_idea(&self, conversation: bool) {
        self.conversation.store(conversation, Ordering::Relaxed);
    }

    /// The transcript path the latest hook carried, if any.
    pub fn transcript(&self) -> Option<PathBuf> {
        self.transcript.lock().unwrap().clone()
    }

    pub fn set_transcript(&self, path: PathBuf) {
        *self.transcript.lock().unwrap() = Some(path);
    }

    pub fn activity(&self) -> Option<String> {
        self.convo.lock().unwrap().activity.clone()
    }

    pub fn needs_terminal(&self) -> bool {
        self.convo.lock().unwrap().needs_terminal
    }

    /// The one way to change the conversation state: tells the view once, and only
    /// when something changed.
    pub fn update_convo(&self, change: impl FnOnce(&mut ConvoState)) {
        let changed = {
            let mut convo = self.convo.lock().unwrap();
            let before = convo.clone();
            change(&mut convo);
            *convo != before
        };
        if changed {
            self.events.emit(Topic::Conversations);
        }
    }

    /// A snapshot of the live progress.
    pub fn progress(&self) -> Progress {
        self.progress.lock().unwrap().progress.clone()
    }

    /// Follows the progress: a new value is seen at each change.
    pub fn subscribe_progress(&self) -> watch::Receiver<Progress> {
        self.progress_tx.subscribe()
    }

    /// Changes the progress; `change` says whether it did, and only then is a snapshot
    /// sent. Returns that.
    pub fn update_progress(&self, change: impl FnOnce(&mut Progress) -> bool) -> bool {
        let mut state = self.progress.lock().unwrap();
        let changed = change(&mut state.progress);
        if changed {
            // Sent under the lock so snapshots reach watchers in order.
            self.progress_tx.send_replace(state.progress.clone());
        }
        changed
    }

    /// Holds the Stop hook if background work runs, checked and stored under one lock.
    /// Returns whether it was held.
    pub fn hold_stop_if_background(&self, hook: serde_json::Value) -> bool {
        let mut state = self.progress.lock().unwrap();
        if state.progress.has_background() {
            state.held_stop = Some(hook);
            true
        } else {
            false
        }
    }

    pub fn drop_held_stop(&self) {
        self.progress.lock().unwrap().held_stop = None;
    }

    /// A new turn began: the held Stop is stale and the fallback timer is cancelled,
    /// both under one lock so a timer that fires meanwhile finds nothing.
    pub fn cancel_held_stop(&self) {
        let mut state = self.progress.lock().unwrap();
        state.held_stop = None;
        if let Some(token) = state.fallback.take() {
            token.store(true, Ordering::SeqCst);
        }
    }

    /// Arms the fallback timer's token (cancelling any earlier one).
    pub fn arm_fallback(&self) -> Arc<AtomicBool> {
        let mut state = self.progress.lock().unwrap();
        if let Some(old) = state.fallback.take() {
            old.store(true, Ordering::SeqCst);
        }
        let token = Arc::new(AtomicBool::new(false));
        state.fallback = Some(Arc::clone(&token));
        token
    }

    /// The held Stop for the fallback timer, unless it was cancelled meanwhile.
    pub fn take_held_stop_unless_cancelled(&self, token: &AtomicBool) -> Option<serde_json::Value> {
        let mut state = self.progress.lock().unwrap();
        if token.load(Ordering::SeqCst) {
            return None;
        }
        state.fallback = None;
        state.held_stop.take()
    }

    pub fn take_held_stop(&self) -> Option<serde_json::Value> {
        self.progress.lock().unwrap().held_stop.take()
    }

    pub fn has_held_stop(&self) -> bool {
        self.progress.lock().unwrap().held_stop.is_some()
    }

    pub fn set_activity(&self, activity: Option<String>) {
        self.update_convo(|c| c.activity = activity);
    }

    /// A turn starts or ends: nothing is being done, nothing waits on the terminal.
    pub fn reset_turn_state(&self) {
        self.update_convo(|c| *c = ConvoState::default());
    }
}

#[derive(Default)]
pub struct Sessions {
    live: Mutex<HashMap<String, Live>>,
    /// Setup sessions the user cancelled, until their end is handled.
    cancelled: Mutex<HashSet<String>>,
    events: Events,
}

impl Sessions {
    pub fn new(events: Events) -> Self {
        Self {
            live: Mutex::default(),
            cancelled: Mutex::default(),
            events,
        }
    }

    pub fn insert(
        &self,
        id: &str,
        task: &str,
        kind: SessionKind,
        primary: bool,
        runtime: impl Into<Runtime>,
    ) -> Live {
        let live = Live {
            id: id.to_string(),
            task: task.to_string(),
            kind,
            primary,
            runtime: runtime.into(),
            started: Arc::new(Notify::new()),
            viewers: Arc::default(),
            state: Arc::new(Mutex::new(match kind {
                SessionKind::Shell | SessionKind::Setup => LiveState::Working,
                SessionKind::Claude => LiveState::Starting,
            })),
            conversation: Arc::default(),
            convo: Arc::default(),
            transcript: Arc::default(),
            progress: Arc::default(),
            progress_tx: Arc::new(watch::channel(Progress::default()).0),
            typing: Arc::default(),
            events: self.events.clone(),
        };
        self.live
            .lock()
            .unwrap()
            .insert(id.to_string(), live.clone());
        self.events.emit(Topic::Sessions);
        live
    }

    /// Notes that the user cancelled this (setup) session, so its end is told from a
    /// failure.
    pub fn mark_cancelled(&self, id: &str) {
        self.cancelled.lock().unwrap().insert(id.to_string());
    }

    /// Whether it was cancelled; forgets the mark.
    pub fn take_cancelled(&self, id: &str) -> bool {
        self.cancelled.lock().unwrap().remove(id)
    }

    pub fn get(&self, id: &str) -> Option<Live> {
        self.live.lock().unwrap().get(id).cloned()
    }

    /// Every live session.
    pub fn all(&self) -> Vec<Live> {
        self.live.lock().unwrap().values().cloned().collect()
    }

    pub fn of_task(&self, task: &str) -> Vec<Live> {
        self.live
            .lock()
            .unwrap()
            .values()
            .filter(|l| l.task == task)
            .cloned()
            .collect()
    }

    pub fn primary(&self, task: &str) -> Option<Live> {
        self.of_task(task).into_iter().find(|l| l.primary)
    }

    /// Whether someone is looking at any of the task's terminals.
    pub fn watched(&self, task: &str) -> bool {
        self.of_task(task)
            .iter()
            .any(|l| l.viewers.load(Ordering::Relaxed) > 0)
    }

    /// Removes the entry only if it is still `runtime`, so a reopened session is never
    /// dropped by its previous process exiting.
    pub fn remove_if_same(&self, id: &str, runtime: &Runtime) {
        let mut live = self.live.lock().unwrap();
        if live.get(id).is_some_and(|l| l.runtime.same(runtime)) {
            live.remove(id);
            // Stopped: its tab shows "Reabrir", and a permission it asked for is gone.
            self.events.emit(Topic::Sessions);
            self.events.emit(Topic::Inbox);
        }
    }
}

#[cfg(test)]
mod tests {
    use delilah_agents::stream::{StreamSession, TaskEvent, TaskKind};
    use serde_json::json;

    use super::*;
    use crate::progress::apply;

    fn live() -> Live {
        let mut cmd = tokio::process::Command::new("sleep");
        cmd.arg("5");
        let (session, _rx) = StreamSession::spawn(cmd).unwrap();
        Sessions::new(Events::default()).insert(
            "s-1",
            "PROJ-1",
            SessionKind::Claude,
            true,
            Arc::new(session),
        )
    }

    fn bg_started() -> delilah_agents::stream::StreamEvent {
        delilah_agents::stream::StreamEvent::Task(TaskEvent::Started {
            task_id: "b1".into(),
            tool_use_id: None,
            kind: TaskKind::Command,
            description: "sleep".into(),
            background: true,
        })
    }

    fn start_background(live: &Live) {
        assert!(live.update_progress(|p| apply(p, &bg_started(), chrono::Utc::now())));
    }

    #[tokio::test]
    async fn progress_is_a_snapshot_and_update_sends_only_when_modified() {
        let live = live();
        let mut rx = live.subscribe_progress();
        assert_eq!(live.progress(), Progress::default());
        assert!(!live.update_progress(|_| false));
        assert!(!rx.has_changed().unwrap());
        start_background(&live);
        assert!(rx.has_changed().unwrap());
        assert_eq!(rx.borrow_and_update().background.len(), 1);
        assert_eq!(live.progress().background.len(), 1);
    }

    #[tokio::test]
    async fn stop_is_held_only_while_background_runs() {
        let live = live();
        assert!(!live.hold_stop_if_background(json!({"a": 1})));
        assert!(!live.has_held_stop());
        start_background(&live);
        assert!(live.hold_stop_if_background(json!({"a": 2})));
        assert!(live.has_held_stop());
        assert_eq!(live.take_held_stop(), Some(json!({"a": 2})));
        assert!(!live.has_held_stop());
        assert!(live.hold_stop_if_background(json!({"a": 3})));
        live.drop_held_stop();
        assert_eq!(live.take_held_stop(), None);
    }

    #[tokio::test]
    async fn clones_share_progress_and_the_held_stop() {
        let live = live();
        let other = live.clone();
        start_background(&live);
        assert!(other.hold_stop_if_background(json!(1)));
        assert!(live.has_held_stop());
        assert_eq!(other.progress(), live.progress());
    }
}
