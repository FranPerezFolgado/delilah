use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::FromRef;
use serde_json::Value;

use crate::config::Config;
use crate::sessions::Sessions;
use crate::store::Store;

#[derive(Clone)]
pub struct AppState {
    pub store: Store,
    pub config: Arc<Config>,
    pub sessions: Arc<Sessions>,
    /// Latest `rate_limits` reported by each task's status line.
    pub usage: Arc<Mutex<HashMap<String, Value>>>,
}

impl AppState {
    pub fn new(store: Store, config: Config) -> Self {
        Self {
            store,
            config: Arc::new(config),
            sessions: Arc::default(),
            usage: Arc::default(),
        }
    }
}

impl FromRef<AppState> for Store {
    fn from_ref(state: &AppState) -> Self {
        state.store.clone()
    }
}
