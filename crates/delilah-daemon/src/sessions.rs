//! Sessions alive in this daemon right now, by session id.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use delilah_agents::claude::ActivityEvent;
use delilah_agents::runtime::Runtime;
use serde::Serialize;
use tokio::sync::{Notify, watch};

use crate::activity::Activity;
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
    /// The sub-agents a terminal session's held Stop waits on (its `background_tasks`).
    held_agents: HashSet<String>,
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
    /// When the turn in progress began (a prompt was submitted, or the session went
    /// back to work), as the hooks said; taken when the turn ends.
    turn_started: Arc<Mutex<Option<chrono::DateTime<chrono::Utc>>>>,
    /// Progress of the turn, background work and sub-agents, plus the held Stop.
    progress: Arc<Mutex<ProgressState>>,
    /// Snapshots of `progress`, sent whenever it changes.
    progress_tx: Arc<watch::Sender<Progress>>,
    /// Held while a message is decided on and typed, so two never interleave.
    pub typing: Arc<tokio::sync::Mutex<()>>,
    /// What its agents are doing right now (Claude sessions only), for `/activity`.
    /// Not to be confused with [`Live::activity`], a conversation's one-line status.
    pub activity: Arc<Mutex<Activity>>,
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
        // Waiting on the user or paused: nothing is running any more.
        if matches!(state, LiveState::Waiting | LiveState::Paused) {
            let cleared = self
                .activity
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clear();
            if cleared {
                self.events.emit(Topic::Activity);
            }
        }
    }

    /// Applies one activity event, announcing it when the view changed; says whether it did.
    pub fn update_activity(&self, event: ActivityEvent) -> bool {
        let changed = self
            .activity
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .apply(event);
        if changed {
            self.events.emit(Topic::Activity);
        }
        changed
    }

    /// A turn began now.
    pub fn mark_turn_started(&self, at: chrono::DateTime<chrono::Utc>) {
        *self.turn_started.lock().unwrap() = Some(at);
    }

    /// When the turn that is ending began, if a hook said; forgets it.
    pub fn take_turn_started(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.turn_started.lock().unwrap().take()
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

    /// Holds a terminal session's Stop if its payload reports a sub-agent still running:
    /// a terminal has no stream to follow its background work, so the Stop's own
    /// `background_tasks` says what it waits on. Background shells do not hold it (a dev
    /// server may run for hours while Claude waits on you). A newer Stop replaces the
    /// older one and cancels its timer; a replayed one always applies. Returns whether it
    /// was held.
    pub fn hold_stop_if_reported(&self, hook: serde_json::Value) -> bool {
        if hook["delilah_replayed"] == true {
            return false;
        }
        let agents: HashSet<String> = hook["background_tasks"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|t| t["status"] == "running" && t["type"] == "subagent")
            .filter_map(|t| t["id"].as_str().map(str::to_string))
            .collect();
        if agents.is_empty() {
            return false;
        }
        let mut state = self.progress.lock().unwrap();
        if let Some(token) = state.fallback.take() {
            token.store(true, Ordering::SeqCst);
        }
        state.held_agents = agents;
        state.held_stop = Some(hook);
        true
    }

    /// A sub-agent ended (its `SubagentStop`). Returns whether it was the last one a held
    /// terminal Stop waited on.
    pub fn held_agent_ended(&self, id: &str) -> bool {
        let mut state = self.progress.lock().unwrap();
        state.held_stop.is_some() && state.held_agents.remove(id) && state.held_agents.is_empty()
    }

    /// Drops the held Stop and cancels the fallback timer, under one lock so a timer that
    /// fires meanwhile finds nothing.
    pub fn drop_held_stop(&self) {
        self.cancel_held_stop();
    }

    /// A new turn began: the held Stop is stale and the fallback timer is cancelled.
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

    /// Takes the held Stop; the fallback timer, if armed, is cancelled with it.
    pub fn take_held_stop(&self) -> Option<serde_json::Value> {
        let mut state = self.progress.lock().unwrap();
        if let Some(token) = state.fallback.take() {
            token.store(true, Ordering::SeqCst);
        }
        state.held_stop.take()
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
            turn_started: Arc::default(),
            progress: Arc::default(),
            progress_tx: Arc::new(watch::channel(Progress::default()).0),
            typing: Arc::default(),
            activity: Arc::default(),
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

    /// Every live session with its activity, for those that have any.
    pub fn busy(&self) -> Vec<(String, Activity)> {
        let lives: Vec<Live> = self.live.lock().unwrap().values().cloned().collect();
        lives
            .into_iter()
            .filter(|l| l.kind == SessionKind::Claude)
            .filter_map(|l| {
                let a = l.activity.lock().unwrap_or_else(|e| e.into_inner()).clone();
                (!a.is_empty()).then_some((l.id, a))
            })
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
        // Dropping or taking the payload cancels the armed fallback too.
        for take in [true, false] {
            let token = live.arm_fallback();
            assert!(live.hold_stop_if_background(json!({"a": 3})));
            if take {
                live.take_held_stop();
            } else {
                live.drop_held_stop();
            }
            assert!(token.load(Ordering::SeqCst));
        }
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

    #[tokio::test]
    async fn a_terminal_stop_is_held_while_its_sub_agents_run() {
        let live = live();
        let stop = |tasks: serde_json::Value| json!({ "hook_event_name": "Stop", "background_tasks": tasks });
        let agent = |id: &str| json!({ "id": id, "type": "subagent", "status": "running" });
        // Nothing reported, only a shell, or only finished entries: not held.
        assert!(!live.hold_stop_if_reported(json!({ "hook_event_name": "Stop" })));
        assert!(!live.hold_stop_if_reported(stop(json!([
            { "id": "b1", "type": "shell", "status": "running" },
            { "id": "a0", "type": "subagent", "status": "completed" },
        ]))));
        assert!(!live.has_held_stop());
        assert!(!live.held_agent_ended("a0"), "nothing held");

        assert!(live.hold_stop_if_reported(stop(json!([agent("a1"), agent("a2")]))));
        assert!(!live.held_agent_ended("zz"), "not one it waits on");
        assert!(!live.held_agent_ended("a1"));
        assert!(live.held_agent_ended("a2"), "the last one");
        assert!(!live.held_agent_ended("a2"), "said once");

        // A newer Stop replaces what it waits on and cancels the older timer.
        let token = live.arm_fallback();
        assert!(live.hold_stop_if_reported(stop(json!([agent("a3")]))));
        assert!(token.load(Ordering::SeqCst));
        assert!(live.held_agent_ended("a3"));

        // A replay is never held again.
        let mut replayed = stop(json!([agent("a4")]));
        replayed["delilah_replayed"] = json!(true);
        live.drop_held_stop();
        assert!(!live.hold_stop_if_reported(replayed));
        assert!(!live.has_held_stop());
    }
}
