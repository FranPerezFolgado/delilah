//! The GraphQL side of `gh`: the query for everything one check needs about a PR and
//! the parsing of its answer into a [`Snapshot`].

use serde::Deserialize;

use super::GhError;
use crate::pr_types::{
    Check, Comment, CommentKind, Outcome, PrState, Review, ReviewDecision, ReviewState, Snapshot,
};

/// Everything one check needs about a PR, in a single call (research R2).
pub(super) const SNAPSHOT_QUERY: &str = "query($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      state isDraft merged headRefOid baseRefName title url reviewDecision
      commits(last: 1) { nodes { commit { statusCheckRollup { contexts(first: 100) { nodes {
        __typename
        ... on CheckRun { name status conclusion detailsUrl }
        ... on StatusContext { context state targetUrl }
      } } } } } }
      reviews(last: 50) { nodes { databaseId state author { login } body submittedAt } }
      comments(last: 100) { nodes { databaseId author { login } body createdAt } }
      reviewThreads(last: 100) { nodes { comments(first: 50) { nodes {
        databaseId author { login } path line body createdAt
      } } } }
    }
  }
}";

#[derive(Deserialize)]
struct Nodes<T> {
    #[serde(default = "Vec::new")]
    nodes: Vec<Option<T>>,
}

impl<T> Nodes<T> {
    fn into_items(self) -> impl Iterator<Item = T> {
        self.nodes.into_iter().flatten()
    }
}

#[derive(Deserialize)]
struct Author {
    login: String,
}

fn login(author: Option<Author>) -> String {
    // A deleted account comes back as null.
    author.map_or_else(|| "ghost".into(), |a| a.login)
}

#[derive(Deserialize)]
struct Envelope {
    data: Option<Data>,
    #[serde(default)]
    errors: Vec<GqlError>,
}

#[derive(Deserialize)]
struct GqlError {
    message: String,
}

#[derive(Deserialize)]
struct Data {
    repository: Option<Repository>,
}

#[derive(Deserialize)]
struct Repository {
    #[serde(rename = "pullRequest")]
    pull_request: Option<RawPr>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPr {
    state: PrState,
    is_draft: bool,
    head_ref_oid: String,
    base_ref_name: String,
    title: String,
    review_decision: Option<ReviewDecision>,
    commits: Nodes<RawCommitNode>,
    reviews: Nodes<RawReview>,
    comments: Nodes<RawComment>,
    review_threads: Nodes<RawThread>,
}

#[derive(Deserialize)]
struct RawCommitNode {
    commit: RawCommit,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCommit {
    status_check_rollup: Option<RawRollup>,
}

#[derive(Deserialize)]
struct RawRollup {
    contexts: Nodes<RawContext>,
}

/// A check run or a commit status, told apart by which fields it has.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawContext {
    name: Option<String>,
    status: Option<String>,
    conclusion: Option<String>,
    details_url: Option<String>,
    context: Option<String>,
    state: Option<String>,
    target_url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawReview {
    database_id: i64,
    state: ReviewState,
    author: Option<Author>,
    body: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawComment {
    database_id: i64,
    author: Option<Author>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    line: Option<i64>,
    body: String,
}

#[derive(Deserialize)]
struct RawThread {
    comments: Nodes<RawComment>,
}

fn check_of(c: RawContext) -> Option<Check> {
    if let Some(name) = c.name {
        let outcome = match (c.status.as_deref(), c.conclusion.as_deref()) {
            (Some("COMPLETED"), Some(conclusion)) => match conclusion {
                "SUCCESS" => Outcome::Success,
                "FAILURE" | "TIMED_OUT" | "ACTION_REQUIRED" | "STARTUP_FAILURE" => Outcome::Failure,
                // CANCELLED, SKIPPED, NEUTRAL, STALE
                _ => Outcome::Neutral,
            },
            _ => Outcome::Pending,
        };
        return Some(Check {
            name,
            outcome,
            url: c.details_url,
        });
    }
    let name = c.context?;
    let outcome = match c.state.as_deref() {
        Some("SUCCESS") => Outcome::Success,
        Some("FAILURE" | "ERROR") => Outcome::Failure,
        _ => Outcome::Pending,
    };
    Some(Check {
        name,
        outcome,
        url: c.target_url,
    })
}

/// Reads the GraphQL answer of [`SNAPSHOT_QUERY`].
pub(crate) fn parse_snapshot(out: &str) -> Result<Snapshot, GhError> {
    let env: Envelope = serde_json::from_str(out).map_err(|e| GhError::Parse(e.to_string()))?;
    let Some(pr) = env
        .data
        .and_then(|d| d.repository)
        .and_then(|r| r.pull_request)
    else {
        // GitHub answers 200 with the problem in `errors`: keep its words so
        // `classify` can read them.
        let message = env.errors.first().map_or_else(
            || "pull request not found".to_string(),
            |e| e.message.clone(),
        );
        return Err(GhError::failed(message));
    };
    let checks = pr
        .commits
        .into_items()
        .next()
        .and_then(|c| c.commit.status_check_rollup)
        .map(|r| r.contexts.into_items().filter_map(check_of).collect())
        .unwrap_or_default();
    let reviews = pr
        .reviews
        .into_items()
        .map(|r| Review {
            id: r.database_id,
            author: login(r.author),
            state: r.state,
            body: r.body,
        })
        .collect();
    let comment = |c: RawComment, kind| Comment {
        id: c.database_id,
        kind,
        author: login(c.author),
        path: c.path,
        line: c.line,
        body: c.body,
    };
    let mut comments: Vec<Comment> = pr
        .comments
        .into_items()
        .map(|c| comment(c, CommentKind::Issue))
        .collect();
    for thread in pr.review_threads.into_items() {
        comments.extend(
            thread
                .comments
                .into_items()
                .map(|c| comment(c, CommentKind::Review)),
        );
    }
    Ok(Snapshot {
        state: pr.state,
        draft: pr.is_draft,
        title: pr.title,
        head: pr.head_ref_oid,
        base: pr.base_ref_name,
        checks,
        review_decision: pr.review_decision,
        reviews,
        comments,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::gh::{AccessProblem, classify};

    pub(crate) const GRAPHQL: &str = r#"{"data":{"repository":{"pullRequest":{
      "state":"OPEN","isDraft":true,"merged":false,"headRefOid":"abc123","baseRefName":"main",
      "title":"Add things","url":"https://github.com/me/repo/pull/42","reviewDecision":"CHANGES_REQUESTED",
      "commits":{"nodes":[{"commit":{"statusCheckRollup":{"contexts":{"nodes":[
        {"__typename":"CheckRun","name":"build","status":"COMPLETED","conclusion":"SUCCESS","detailsUrl":"https://ci/1"},
        {"__typename":"CheckRun","name":"test","status":"COMPLETED","conclusion":"FAILURE","detailsUrl":"https://ci/2"},
        {"__typename":"CheckRun","name":"cancelled","status":"COMPLETED","conclusion":"CANCELLED","detailsUrl":null},
        {"__typename":"CheckRun","name":"skipped","status":"COMPLETED","conclusion":"SKIPPED","detailsUrl":null},
        {"__typename":"CheckRun","name":"neutral","status":"COMPLETED","conclusion":"NEUTRAL","detailsUrl":null},
        {"__typename":"CheckRun","name":"slow","status":"COMPLETED","conclusion":"TIMED_OUT","detailsUrl":null},
        {"__typename":"CheckRun","name":"queued","status":"QUEUED","conclusion":null,"detailsUrl":null},
        {"__typename":"StatusContext","context":"legacy/ci","state":"ERROR","targetUrl":"https://legacy/1"},
        {"__typename":"StatusContext","context":"legacy/lint","state":"SUCCESS","targetUrl":null}
      ]}}}}]},
      "reviews":{"nodes":[
        {"databaseId":11,"state":"CHANGES_REQUESTED","author":{"login":"alice"},"body":"Please fix","submittedAt":"2026-10-20T10:00:00Z"},
        {"databaseId":12,"state":"DISMISSED","author":null,"body":"","submittedAt":"2026-10-20T11:00:00Z"}]},
      "comments":{"nodes":[{"databaseId":21,"author":{"login":"bob"},"body":"Looks odd","createdAt":"2026-10-20T10:30:00Z"}]},
      "reviewThreads":{"nodes":[{"comments":{"nodes":[
        {"databaseId":31,"author":{"login":"alice"},"path":"src/lib.rs","line":7,"body":"Rename","createdAt":"2026-10-20T10:00:00Z"}]}}]}
    }}}}"#;

    #[test]
    fn graphql_answer_becomes_a_snapshot() {
        let s = parse_snapshot(GRAPHQL).unwrap();
        assert_eq!(s.state, PrState::Open);
        assert!(s.draft);
        assert_eq!((s.head.as_str(), s.base.as_str()), ("abc123", "main"));
        assert_eq!(s.review_decision, Some(ReviewDecision::ChangesRequested));
        let outcomes: Vec<_> = s
            .checks
            .iter()
            .map(|c| (c.name.as_str(), c.outcome))
            .collect();
        assert_eq!(
            outcomes,
            [
                ("build", Outcome::Success),
                ("test", Outcome::Failure),
                ("cancelled", Outcome::Neutral),
                ("skipped", Outcome::Neutral),
                ("neutral", Outcome::Neutral),
                ("slow", Outcome::Failure),
                ("queued", Outcome::Pending),
                ("legacy/ci", Outcome::Failure),
                ("legacy/lint", Outcome::Success),
            ]
        );
        assert_eq!(s.checks[1].url.as_deref(), Some("https://ci/2"));
        assert_eq!(s.reviews.len(), 2);
        assert_eq!(s.reviews[0].state, ReviewState::ChangesRequested);
        assert_eq!(s.reviews[1].state, ReviewState::Other);
        assert_eq!(s.reviews[1].author, "ghost");
        let ids: Vec<_> = s.comments.iter().map(|c| (c.id, c.kind)).collect();
        assert_eq!(ids, [(21, CommentKind::Issue), (31, CommentKind::Review)]);
        assert_eq!(s.comments[1].path.as_deref(), Some("src/lib.rs"));
        assert_eq!(s.comments[1].line, Some(7));
    }

    #[test]
    fn a_pr_without_checks_parses_and_a_missing_one_is_an_error() {
        let bare = crate::gh::fake::bare_snapshot("MERGED", true);
        let s = parse_snapshot(&bare).unwrap();
        assert_eq!(s.state, PrState::Merged);
        assert!(s.checks.is_empty());
        assert_eq!(s.review_decision, None);

        let missing = r#"{"data":{"repository":null},"errors":[{"message":"Could not resolve to a Repository with the name 'a/b'."}]}"#;
        let err = parse_snapshot(missing).unwrap_err();
        assert_eq!(classify(&err), AccessProblem::NoAccess);
    }
}
