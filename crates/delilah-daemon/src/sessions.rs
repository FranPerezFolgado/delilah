//! Sessions alive in this daemon right now, by session id.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use delilah_agents::pty::PtySession;
use serde::Serialize;
use tokio::sync::Notify;

use crate::events::{Events, Topic};
use crate::store_sessions::SessionKind;

/// At daemon start: tasks stuck preparing their workspace (their setup died with the
/// previous daemon) stop as `setup-failed` ("interrupted").
pub async fn recover_interrupted_setups(app: &crate::state::AppState) -> Result<(), String> {
    crate::setup::recover_interrupted(app)
        .await
        .map_err(|e| e.to_string())
}

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

#[derive(Clone)]
pub struct Live {
    pub id: String,
    pub task: String,
    pub kind: SessionKind,
    pub primary: bool,
    pub pty: Arc<PtySession>,
    /// Fired by the agent's SessionStart hook.
    pub started: Arc<Notify>,
    /// Clients attached to the terminal right now.
    pub viewers: Arc<AtomicUsize>,
    pub state: Arc<Mutex<LiveState>>,
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
}

#[derive(Default)]
pub struct Sessions {
    live: Mutex<HashMap<String, Live>>,
    events: Events,
}

impl Sessions {
    pub fn new(events: Events) -> Self {
        Self {
            live: Mutex::default(),
            events,
        }
    }

    pub fn insert(
        &self,
        id: &str,
        task: &str,
        kind: SessionKind,
        primary: bool,
        pty: Arc<PtySession>,
    ) -> Live {
        let live = Live {
            id: id.to_string(),
            task: task.to_string(),
            kind,
            primary,
            pty,
            started: Arc::new(Notify::new()),
            viewers: Arc::default(),
            state: Arc::new(Mutex::new(match kind {
                SessionKind::Shell | SessionKind::Setup => LiveState::Working,
                SessionKind::Claude => LiveState::Starting,
            })),
            events: self.events.clone(),
        };
        self.live
            .lock()
            .unwrap()
            .insert(id.to_string(), live.clone());
        self.events.emit(Topic::Sessions);
        live
    }

    pub fn get(&self, id: &str) -> Option<Live> {
        self.live.lock().unwrap().get(id).cloned()
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

    /// Removes the entry only if it is still `pty`, so a reopened session is never
    /// dropped by its previous process exiting.
    pub fn remove_if_same(&self, id: &str, pty: &Arc<PtySession>) {
        let mut live = self.live.lock().unwrap();
        if live.get(id).is_some_and(|l| Arc::ptr_eq(&l.pty, pty)) {
            live.remove(id);
            // Stopped: its tab shows "Reabrir", and a permission it asked for is gone.
            self.events.emit(Topic::Sessions);
            self.events.emit(Topic::Inbox);
        }
    }
}
