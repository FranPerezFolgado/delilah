//! A `Gh` answering from canned replies, for the tests of any module.

use std::path::Path;
use std::sync::Mutex;

use futures_util::future::BoxFuture;

use super::{Gh, GhError, strings};

/// `gh repo view` of `o/r` on github.com.
pub(crate) const REPO_VIEW: &str =
    r#"{"nameWithOwner":"o/r","url":"https://github.com/o/r","defaultBranchRef":{"name":"main"}}"#;
/// `gh pr list` with one open PR, `PR_URL`.
pub(crate) const PR_LIST: &str = r#"[{"number":7,"url":"https://github.com/o/r/pull/7","state":"OPEN","updatedAt":"2026-10-20T10:00:00Z"}]"#;
pub(crate) const PR_URL: &str = "https://github.com/o/r/pull/7";

/// A GraphQL answer for a PR with no checks, reviews or comments.
pub(crate) fn bare_snapshot(state: &str, merged: bool) -> String {
    format!(
        r#"{{"data":{{"repository":{{"pullRequest":{{"state":"{state}","isDraft":false,"merged":{merged},
      "headRefOid":"abc","baseRefName":"main","title":"t","url":"u","reviewDecision":null,
      "commits":{{"nodes":[{{"commit":{{"statusCheckRollup":null}}}}]}},
      "reviews":{{"nodes":[]}},"comments":{{"nodes":[]}},"reviewThreads":{{"nodes":[]}}}}}}}}}}"#
    )
}

type Reply = (Vec<String>, Result<String, GhError>);

pub(crate) struct FakeGh {
    replies: Mutex<Vec<Reply>>,
    calls: Mutex<Vec<Vec<String>>>,
}

impl FakeGh {
    pub(crate) fn new() -> Self {
        Self {
            replies: Mutex::default(),
            calls: Mutex::default(),
        }
    }

    /// A repo `o/r` with one open PR (`PR_URL`) that `gh api graphql` can read.
    pub(crate) fn happy() -> Self {
        Self::new()
            .on(&["repo", "view"], Ok(REPO_VIEW))
            .on(&["pr", "list"], Ok(PR_LIST))
            .on(&["api", "graphql"], Ok(&bare_snapshot("OPEN", false)))
    }

    /// Signed in as `login`.
    pub(crate) fn with_user(self, login: &str) -> Self {
        self.on(&["api", "user"], Ok(&format!("{login}\n")))
    }

    /// Answers calls whose arguments start with `prefix` (the first match wins).
    pub(crate) fn on(self, prefix: &[&str], reply: Result<&str, GhError>) -> Self {
        self.replies
            .lock()
            .unwrap()
            .push((strings(prefix), reply.map(str::to_string)));
        self
    }

    /// Every call made, in order.
    pub(crate) fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }
}

impl Gh for FakeGh {
    fn run(
        &self,
        _dir: Option<&Path>,
        args: Vec<String>,
        _stdin: Option<String>,
    ) -> BoxFuture<'_, Result<String, GhError>> {
        let reply = self
            .replies
            .lock()
            .unwrap()
            .iter()
            .find(|(prefix, _)| args.starts_with(prefix))
            .map(|(_, r)| r.clone());
        let unsupported = format!("fake gh: unsupported: {}", args.join(" "));
        self.calls.lock().unwrap().push(args);
        Box::pin(async move { reply.unwrap_or_else(|| Err(GhError::failed(unsupported))) })
    }
}
