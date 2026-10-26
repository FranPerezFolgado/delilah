//! CI autofix: when a task's PR fails CI, Delilah forwards the failure to the task's
//! agent on its own, a limited number of times. The state lives in
//! `tasks.ci_autofix` (on/off) and `pull_requests.autofix` (per PR). See
//! `specs/010-ci-autofix`.

pub(crate) mod api;

use serde::{Deserialize, Serialize};

/// Autofix attempts per PR before it stops (FR-008).
#[allow(dead_code)]
pub(crate) const LIMIT: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum State {
    Watching,
    Fixing,
    Stopped,
}

/// Why autofix stopped on a PR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum StopReason {
    Limit,
    NoPush,
    NoSession,
}

/// The autofix state of one PR (`pull_requests.autofix`, JSON). Fields the migration
/// backfill leaves out read as `None`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PrAutofix {
    pub state: State,
    pub attempts: u32,
    /// The last head commit forwarded to the agent (by autofix or by hand).
    #[serde(default)]
    pub sha: Option<String>,
    /// The session of the last attempt.
    #[serde(default)]
    pub session: Option<String>,
    /// RFC 3339.
    #[serde(default)]
    pub sent_at: Option<String>,
    #[serde(default)]
    pub reason: Option<StopReason>,
}

#[allow(dead_code)]
pub(crate) fn evaluate() {}

#[allow(dead_code)]
pub(crate) fn on_turn_ended() {}

#[allow(dead_code)]
pub(crate) fn cancel() {}
