//! Agent terminals currently alive in this daemon, by task.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use delilah_agents::pty::PtySession;
use tokio::sync::Notify;

#[derive(Clone)]
pub struct Live {
    pub pty: Arc<PtySession>,
    /// Fired by the agent's SessionStart hook.
    pub started: Arc<Notify>,
}

#[derive(Default)]
pub struct Sessions {
    live: Mutex<HashMap<String, Live>>,
}

impl Sessions {
    pub fn insert(&self, key: &str, pty: Arc<PtySession>) -> Live {
        let live = Live {
            pty,
            started: Arc::new(Notify::new()),
        };
        self.live
            .lock()
            .unwrap()
            .insert(key.to_string(), live.clone());
        live
    }

    pub fn get(&self, key: &str) -> Option<Live> {
        self.live.lock().unwrap().get(key).cloned()
    }

    /// Removes the entry only if it is still `pty`, so a newer session for the same
    /// task is never dropped by the old one exiting.
    pub fn remove_if_same(&self, key: &str, pty: &Arc<PtySession>) {
        let mut live = self.live.lock().unwrap();
        if live.get(key).is_some_and(|l| Arc::ptr_eq(&l.pty, pty)) {
            live.remove(key);
        }
    }
}
