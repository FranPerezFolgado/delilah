use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::FromRef;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::config::Config;
use crate::events::{Events, Topic};
use crate::gh::{CliGh, Gh, GhClient};
use crate::notify::Notifier;
use crate::prs::Backoff;
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
    /// The language Delilah speaks in (see `lang.rs`); Spanish until `main` reads
    /// the preference.
    pub lang: crate::lang::Current,
    /// Tasks the inbox suggests moving to review (see `phases.rs`).
    pub review_hints: Arc<Mutex<std::collections::HashSet<String>>>,
    /// The saving-mode handoffs' lock and in-flight starts (see `handoff.rs`).
    pub handoffs: Arc<crate::handoff::HandoffCoordinator>,
    /// Whether Claude sessions start with Remote Control (the `remote-control`
    /// preference); off until `main` reads it.
    remote_control: Arc<AtomicBool>,
    /// The user's `gh`, behind a trait so tests answer with canned output.
    pub gh: Arc<GhClient>,
    /// Per repo (`host/owner/name`): when to try `gh` again after it failed.
    pub(crate) pr_backoff: Arc<Mutex<Backoff>>,
    /// Tasks whose PRs are being checked now: one check per task at a time.
    pub(crate) pr_checking: Arc<Mutex<HashSet<String>>>,
    /// Tasks with a delayed check already waiting, so a burst of pushes makes one.
    pub(crate) pr_nudges: Arc<Mutex<HashSet<String>>>,
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
            lang: crate::lang::Current::default(),
            review_hints: Arc::default(),
            handoffs: Arc::default(),
            remote_control: Arc::default(),
            gh: Arc::new(GhClient::new(Arc::new(CliGh))),
            pr_backoff: Arc::default(),
            pr_checking: Arc::default(),
            pr_nudges: Arc::default(),
        }
    }

    /// The same state talking to another `gh` (tests inject a fake).
    pub fn with_gh(mut self, gh: Arc<dyn Gh>) -> Self {
        self.gh = Arc::new(GhClient::new(gh));
        self
    }

    pub fn lang(&self) -> crate::lang::Lang {
        self.lang.get()
    }

    /// Reads the `language` preference again (at start, and when it changes).
    pub async fn load_language(&self) {
        let value: Option<Value> = self.store.setting("pref:language").await.ok().flatten();
        let setting = value.as_ref().and_then(Value::as_str).map(str::to_string);
        let lang =
            tokio::task::spawn_blocking(move || crate::lang::from_setting(setting.as_deref()))
                .await
                .unwrap_or(crate::lang::Lang::Es);
        self.lang.set(lang);
    }

    pub fn remote_control(&self) -> bool {
        self.remote_control.load(Ordering::Relaxed)
    }

    /// Reads the `remote-control` preference again (at start, and when it changes).
    pub async fn load_remote_control(&self) {
        let value: Option<Value> = self
            .store
            .setting("pref:remote-control")
            .await
            .ok()
            .flatten();
        let on = value.as_ref().and_then(Value::as_bool).unwrap_or(false);
        self.remote_control.store(on, Ordering::Relaxed);
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
