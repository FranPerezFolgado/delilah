//! CI autofix: when a task's PR fails CI, Delilah forwards the failure to the task's
//! agent on its own, a limited number of times. The state lives in
//! `tasks.ci_autofix` (on/off) and `pull_requests.autofix` (per PR). See
//! `specs/010-ci-autofix`.

pub(crate) mod api;

#[allow(dead_code)]
pub(crate) fn evaluate() {}

#[allow(dead_code)]
pub(crate) fn on_turn_ended() {}

#[allow(dead_code)]
pub(crate) fn cancel() {}
