use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::FromRef;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::config::Config;
use crate::events::{Events, Topic};
use crate::notify::Notifier;
use crate::secrets::Secrets;
use crate::sessions::Sessions;
use crate::specs::SpecSummary;
use crate::store::Store;

/// The subscription's usage windows, as last reported by any agent's status line:
/// all tasks share one account, so the newest report is the truth.
#[derive(Debug, Clone, Serialize)]
pub struct UsageReport {
    pub at: DateTime<Utc>,
    pub task: String,
    pub rate_limits: Value,
}

#[derive(Clone)]
pub struct AppState {
    pub store: Store,
    pub config: Arc<Config>,
    pub sessions: Arc<Sessions>,
    pub usage: Arc<Mutex<Option<UsageReport>>>,
    pub notifier: Notifier,
    pub secrets: Secrets,
    /// Each task's spec as last read, by task key (see `specs.rs`).
    pub specs: Arc<Mutex<HashMap<String, SpecSummary>>>,
    /// What changed, for clients listening on `/api/v1/events`.
    pub events: Events,
    /// AskUserQuestion questions waiting for an answer from Delilah.
    pub asks: Arc<crate::asks::Asks>,
}

impl AppState {
    pub fn new(store: Store, config: Config) -> Self {
        let notifier = if config.notify {
            Notifier::MacOs
        } else {
            Notifier::Recorder(Arc::default())
        };
        let events = Events::default();
        Self {
            store,
            config: Arc::new(config),
            sessions: Arc::new(Sessions::new(events.clone())),
            usage: Arc::default(),
            notifier,
            // Unit tests must never touch the real Keychain.
            secrets: if cfg!(test) {
                Secrets::Memory(Arc::default())
            } else {
                Secrets::Keychain
            },
            specs: Arc::default(),
            events,
            asks: Arc::default(),
        }
    }

    pub fn emit(&self, topic: Topic) {
        self.events.emit(topic);
    }

    pub fn latest_limits(&self) -> Option<Value> {
        self.usage
            .lock()
            .unwrap()
            .as_ref()
            .map(|u| u.rate_limits.clone())
    }
}

impl FromRef<AppState> for Store {
    fn from_ref(state: &AppState) -> Self {
        state.store.clone()
    }
}
