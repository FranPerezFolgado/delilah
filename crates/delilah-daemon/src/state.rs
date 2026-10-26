use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
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

/// How task and project agents are launched with respect to permissions (the
/// `agent-permissions` preference): `Auto` is Claude's auto mode, `Default` is Claude's
/// own default (it asks for everything not already allowed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentPermissions {
    Auto,
    Default,
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
    /// Permissions a chat-mode agent waits on (see `permissions.rs`).
    pub permissions: Arc<crate::permissions::Permissions>,
    /// The language Delilah speaks in (see `lang.rs`); Spanish until `main` reads
    /// the preference.
    pub lang: crate::lang::Current,
    /// Tasks the inbox suggests moving to review (see `phases.rs`).
    pub review_hints: Arc<Mutex<std::collections::HashSet<String>>>,
    /// Last size seen of each live session's transcript (see `transcript_watch.rs`).
    pub(crate) transcript_seen: Arc<Mutex<HashMap<String, u64>>>,
    /// The saving-mode handoffs' lock and in-flight starts (see `handoff.rs`).
    pub handoffs: Arc<crate::handoff::HandoffCoordinator>,
    /// One lock per task for the branch and name triggers (see `branching.rs`).
    pub(crate) branching: Arc<crate::branching::BranchLocks>,
    /// The autopilot finishes running now, and their retries (see `autopilot/finish.rs`).
    pub(crate) finishing: Arc<crate::autopilot::Finishing>,
    /// Whether Claude sessions start with Remote Control (the `remote-control`
    /// preference); off until `main` reads it.
    remote_control: Arc<AtomicBool>,
    /// The `agent-permissions` preference, as an `AgentPermissions` (0 = Auto, 1 =
    /// Default); `Auto` until `main` reads it.
    agent_permissions: Arc<AtomicU8>,
    /// The user's `gh`, behind a trait so tests answer with canned output.
    pub gh: Arc<GhClient>,
    /// Per repo (`host/owner/name`): when to try `gh` again after it failed.
    pub(crate) pr_backoff: Arc<Mutex<Backoff>>,
    /// Tasks whose PRs are being checked now: one check per task at a time.
    pub(crate) pr_checking: Arc<Mutex<HashSet<String>>>,
    /// Tasks with a delayed check already waiting, so a burst of pushes makes one.
    pub(crate) pr_nudges: Arc<Mutex<HashSet<String>>>,
    /// The previews the daemon runs, one per task at most (see `preview.rs`).
    pub(crate) previews: Arc<crate::preview::Previews>,
}

/// Unit tests must never touch the real Keychain; `DELILAH_SECRETS=memory` also asks for it.
fn use_memory_secrets(secrets_in_memory: bool, under_test: bool) -> bool {
    under_test || secrets_in_memory
}

impl AppState {
    pub fn new(store: Store, config: Config) -> Self {
        let notifier = if config.notify {
            Notifier::MacOs
        } else {
            Notifier::Recorder(Arc::default())
        };
        let secrets_in_memory = config.secrets_in_memory;
        let gh_program = config.gh_program.clone();
        let events = Events::default();
        Self {
            store,
            config: Arc::new(config),
            previews: Arc::new(crate::preview::Previews::new(events.clone())),
            sessions: Arc::new(Sessions::new(events.clone())),
            usage: Arc::default(),
            notifier,
            // Unit tests must never touch the real Keychain.
            secrets: if use_memory_secrets(secrets_in_memory, cfg!(test)) {
                Secrets::Memory(Arc::default())
            } else {
                Secrets::Keychain
            },
            specs: Arc::default(),
            events,
            asks: Arc::default(),
            permissions: Arc::default(),
            // English unless told otherwise (decision 81).
            lang: crate::lang::Current::default(),
            review_hints: Arc::default(),
            transcript_seen: Arc::default(),
            handoffs: Arc::default(),
            branching: Arc::default(),
            finishing: Arc::default(),
            remote_control: Arc::default(),
            agent_permissions: Arc::default(),
            gh: Arc::new(GhClient::new(Arc::new(CliGh {
                program: gh_program,
            }))),
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
                .unwrap_or(crate::lang::Lang::En);
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

    pub fn agent_permissions(&self) -> AgentPermissions {
        match self.agent_permissions.load(Ordering::Relaxed) {
            1 => AgentPermissions::Default,
            _ => AgentPermissions::Auto,
        }
    }

    /// Reads the `agent-permissions` preference again (at start, and when it changes):
    /// `"default"` gives `Default`; anything else, including unset, gives `Auto`. A store
    /// that cannot be read leaves the mode as it was, so a failed read never widens it.
    pub async fn load_agent_permissions(&self) {
        let value: Option<Value> = match self.store.setting("pref:agent-permissions").await {
            Ok(value) => value,
            Err(e) => {
                tracing::warn!("cannot read the agent-permissions preference: {e}");
                return;
            }
        };
        let mode = match value.as_ref().and_then(Value::as_str) {
            Some("default") => 1,
            _ => 0,
        };
        self.agent_permissions.store(mode, Ordering::Relaxed);
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

#[cfg(test)]
mod secrets_tests {
    use super::use_memory_secrets;

    #[test]
    fn memory_secrets_under_test_or_when_asked() {
        assert!(use_memory_secrets(true, false));
        assert!(use_memory_secrets(false, true));
        assert!(!use_memory_secrets(false, false));
    }
}
