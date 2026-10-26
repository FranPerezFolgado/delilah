//! Sessions alive in this daemon right now, by session id.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use delilah_agents::claude::ActivityEvent;
use delilah_agents::runtime::Runtime;
use serde::Serialize;
use tokio::sync::Notify;

use crate::activity::Activity;
use crate::events::{Events, Topic};
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
            let cleared = self.activity.lock().unwrap_or_else(|e| e.into_inner()).clear();
            if cleared {
                self.events.emit(Topic::Activity);
            }
        }
    }

    /// Applies one activity event, announcing it when the view changed; says whether it did.
    pub fn update_activity(&self, event: ActivityEvent) -> bool {
        let changed = self.activity.lock().unwrap_or_else(|e| e.into_inner()).apply(event);
        if changed {
            self.events.emit(Topic::Activity);
        }
        changed
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
