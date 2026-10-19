use std::sync::{Arc, Mutex};

use axum::extract::FromRef;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::config::Config;
use crate::notify::Notifier;
use crate::sessions::Sessions;
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
}

impl AppState {
    pub fn new(store: Store, config: Config) -> Self {
        let notifier = if config.notify {
            Notifier::MacOs
        } else {
            Notifier::Recorder(Arc::default())
        };
        Self {
            store,
            config: Arc::new(config),
            sessions: Arc::default(),
            usage: Arc::default(),
            notifier,
        }
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
