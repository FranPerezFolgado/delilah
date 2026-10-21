//! Back-off and access problems of the PR loop. A problem the user can fix (`gh`
//! missing, signed out, no access, not a GitHub remote) is kept per project in the
//! setting `pr_access:<project>`, for tasks that have no PR row to carry it, and
//! announced once with a notification when the app is not open.

use std::time::Instant;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::events::Topic;
use crate::gh::{AccessProblem, GhError, classify};
use crate::state::AppState;

/// An access problem found while looking for PRs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Access {
    pub kind: String,
    pub message: String,
}

fn flag(project: &str) -> String {
    format!("pr_access:{project}")
}

/// The access problem standing for `project`, if any.
pub(crate) async fn problem_of(app: &AppState, project: &str) -> Option<Access> {
    let value: Option<Option<Access>> = app.store.setting(&flag(project)).await.ok().flatten();
    value.flatten()
}

/// `gh` failed for `backoff_key`: back off, remember the problem if the user can fix
/// it, and tell them once.
pub(super) async fn failed(
    app: &AppState,
    project: &str,
    backoff_key: &str,
    err: &GhError,
    force: bool,
) {
    let problem = classify(err);
    let now = Instant::now();
    let reset = match &problem {
        AccessProblem::RateLimited { reset: Some(at) } => {
            Some(now + (*at - Utc::now()).to_std().unwrap_or_default())
        }
        _ => None,
    };
    if !force || reset.is_some() {
        app.pr_backoff
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .fail(backoff_key, now, reset);
    }
    let Some((title, body)) = access_message(app, project, &problem) else {
        return;
    };
    let new = Access {
        kind: problem.kind().to_string(),
        message: err.to_string(),
    };
    let current = problem_of(app, project).await;
    if current.as_ref() == Some(&new) {
        return;
    }
    if app
        .store
        .set_setting(&flag(project), &Some(&new))
        .await
        .is_err()
    {
        return;
    }
    app.emit(Topic::Prs);
    // With the app open the PR tab shows the problem; the notification is for when it is not.
    let newly = current.is_none_or(|c| c.kind != new.kind);
    if newly && !app.events.listening() {
        app.notifier.send(title, body);
    }
}

/// What to tell the user about an access problem; `None` for the ones that pass alone.
fn access_message(
    app: &AppState,
    project: &str,
    problem: &AccessProblem,
) -> Option<(String, String)> {
    let lang = app.lang();
    let body = match problem {
        AccessProblem::Missing => lang.pick(
            "Instala la CLI de GitHub (gh).",
            "Install the GitHub CLI (gh).",
        ),
        AccessProblem::SignedOut => lang.pick(
            "Inicia sesión con `gh auth login`.",
            "Sign in with `gh auth login`.",
        ),
        AccessProblem::NoAccess => lang.pick(
            "Tu cuenta de gh no tiene acceso al repositorio.",
            "Your gh account has no access to the repository.",
        ),
        AccessProblem::UnsupportedRemote => lang.pick(
            "El remoto de git no es de GitHub.",
            "The git remote is not a GitHub one.",
        ),
        AccessProblem::Network | AccessProblem::RateLimited { .. } => return None,
    };
    let title = format!(
        "{} {project}",
        lang.pick("PRs no disponibles en", "PRs unavailable in")
    );
    Some((title, body.to_string()))
}

/// `gh` answered: forget the back-off and any access problem of the project.
pub(super) async fn succeeded(app: &AppState, project: &str, backoff_keys: &[&str]) {
    {
        let mut backoff = app
            .pr_backoff
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for key in backoff_keys {
            backoff.ok(key);
        }
    }
    if problem_of(app, project).await.is_some()
        && app
            .store
            .set_setting(&flag(project), &None::<Access>)
            .await
            .is_ok()
    {
        app.emit(Topic::Prs);
    }
}
