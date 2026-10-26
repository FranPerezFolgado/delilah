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

/// A GraphQL answer for an open PR at `head` with one check `build` that ended in
/// `conclusion` (`SUCCESS`, `FAILURE`...), and the given merged/closed `state`.
pub(crate) fn ci_snapshot(state: &str, head: &str, conclusion: &str) -> String {
    format!(
        r#"{{"data":{{"repository":{{"mergeCommitAllowed":true,"squashMergeAllowed":true,"rebaseMergeAllowed":false,
      "pullRequest":{{"state":"{state}","isDraft":false,"merged":{merged},
      "headRefOid":"{head}","baseRefName":"main","title":"Add things","url":"u","reviewDecision":null,
      "commits":{{"nodes":[{{"commit":{{"statusCheckRollup":{{"contexts":{{"nodes":[
        {{"__typename":"CheckRun","name":"build","status":"COMPLETED","conclusion":"{conclusion}","detailsUrl":"https://ci/1",
          "startedAt":"2026-10-20T09:00:00Z","completedAt":"2026-10-20T09:01:30Z"}}
      ]}}}}}}}}]}},
      "reviews":{{"nodes":[]}},"comments":{{"nodes":[]}},"reviewThreads":{{"nodes":[]}}}}}}}}}}"#,
        merged = state == "MERGED"
    )
}

/// A GraphQL answer for an open PR at `head` titled `title`, with the given JSON
/// `checks` (check-run nodes) and issue `comments` (nodes).
pub(crate) fn custom_snapshot(
    head: &str,
    title: &str,
    checks: &[String],
    comments: &[String],
) -> String {
    format!(
        r#"{{"data":{{"repository":{{"pullRequest":{{"state":"OPEN","isDraft":false,"merged":false,
      "headRefOid":"{head}","baseRefName":"main","title":{title},"url":"u","reviewDecision":null,
      "commits":{{"nodes":[{{"commit":{{"statusCheckRollup":{{"contexts":{{"nodes":[{checks}]}}}}}}}}]}},
      "reviews":{{"nodes":[]}},"comments":{{"nodes":[{comments}]}},"reviewThreads":{{"nodes":[]}}}}}}}}}}"#,
        title = serde_json::Value::from(title),
        checks = checks.join(","),
        comments = comments.join(","),
    )
}

/// A check-run node named `name` that is `status` (`COMPLETED`, `IN_PROGRESS`) with
/// `conclusion`.
pub(crate) fn check_node(name: &str, status: &str, conclusion: Option<&str>, url: &str) -> String {
    serde_json::json!({
        "__typename": "CheckRun", "name": name, "status": status,
        "conclusion": conclusion, "detailsUrl": url,
    })
    .to_string()
}

/// An issue-comment node.
pub(crate) fn comment_node(id: i64, author: &str, body: &str) -> String {
    serde_json::json!({
        "databaseId": id, "author": {"login": author}, "body": body,
        "createdAt": "2026-10-20T10:00:00Z",
    })
    .to_string()
}

type Reply = (Vec<String>, Result<String, GhError>);

pub(crate) struct FakeGh {
    replies: Mutex<Vec<Reply>>,
    calls: Mutex<Vec<Vec<String>>>,
    stdins: Mutex<Vec<Option<String>>>,
}

impl FakeGh {
    pub(crate) fn new() -> Self {
        Self {
            replies: Mutex::default(),
            calls: Mutex::default(),
            stdins: Mutex::default(),
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

    /// Answers calls whose arguments start with `prefix` (the first match wins; when
    /// several replies match, the first is used up, so a call can be answered differently
    /// the second time).
    pub(crate) fn on(self, prefix: &[&str], reply: Result<&str, GhError>) -> Self {
        self.replies
            .lock()
            .unwrap()
            .push((strings(prefix), reply.map(str::to_string)));
        self
    }

    /// The stdin of every call made, in order.
    pub(crate) fn stdins(&self) -> Vec<Option<String>> {
        self.stdins.lock().unwrap().clone()
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
        stdin: Option<String>,
    ) -> BoxFuture<'_, Result<String, GhError>> {
        let reply = {
            let mut replies = self.replies.lock().unwrap();
            let mut matching = replies
                .iter()
                .enumerate()
                .filter(|(_, (prefix, _))| args.starts_with(prefix))
                .map(|(i, _)| i);
            let first = matching.next();
            let more = matching.next().is_some();
            first.map(|i| {
                if more {
                    replies.remove(i).1
                } else {
                    replies[i].1.clone()
                }
            })
        };
        let unsupported = format!("fake gh: unsupported: {}", args.join(" "));
        self.stdins.lock().unwrap().push(stdin);
        self.calls.lock().unwrap().push(args);
        Box::pin(async move { reply.unwrap_or_else(|| Err(GhError::failed(unsupported))) })
    }
}
