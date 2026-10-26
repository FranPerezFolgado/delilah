//! The GraphQL side of `gh`: the query for everything one check needs about a PR and
//! the parsing of its answer into a [`Snapshot`].

use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::GhError;
use crate::pr_types::{
    Check, Comment, CommentKind, MergeMethod, Outcome, PrState, Review, ReviewComment,
    ReviewDecision, ReviewState, Snapshot,
};

/// The most review comments kept in a snapshot.
const MAX_REVIEW_COMMENTS: usize = 50;

/// Everything one check needs about a PR, in a single call (research R2).
pub(super) const SNAPSHOT_QUERY: &str = "query($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    mergeCommitAllowed squashMergeAllowed rebaseMergeAllowed
    pullRequest(number: $number) {
      state isDraft merged headRefOid baseRefName title url reviewDecision
      commits(last: 1) { nodes { commit { statusCheckRollup { contexts(first: 100) { nodes {
        __typename
        ... on CheckRun { name status conclusion detailsUrl startedAt completedAt }
        ... on StatusContext { context state targetUrl createdAt }
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

fn login_ref(author: Option<&Author>) -> String {
    author.map_or_else(|| "ghost".into(), |a| a.login.clone())
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
#[serde(rename_all = "camelCase")]
struct Repository {
    #[serde(default)]
    merge_commit_allowed: bool,
    #[serde(default)]
    squash_merge_allowed: bool,
    #[serde(default)]
    rebase_merge_allowed: bool,
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
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    created_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawReview {
    database_id: i64,
    state: ReviewState,
    author: Option<Author>,
    body: String,
    submitted_at: Option<DateTime<Utc>>,
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
    created_at: Option<DateTime<Utc>>,
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
            started_at: c.started_at,
            finished_at: if outcome == Outcome::Pending {
                None
            } else {
                c.completed_at
            },
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
        // A commit status has one time: when it was set.
        started_at: c.created_at,
        finished_at: if outcome == Outcome::Pending {
            None
        } else {
            c.created_at
        },
    })
}

/// Reads the GraphQL answer of [`SNAPSHOT_QUERY`].
pub(crate) fn parse_snapshot(out: &str) -> Result<Snapshot, GhError> {
    let env: Envelope = serde_json::from_str(out).map_err(|e| GhError::Parse(e.to_string()))?;
    let repo = env.data.and_then(|d| d.repository);
    let merge_methods: Vec<MergeMethod> = repo
        .as_ref()
        .map(|r| {
            [
                (r.merge_commit_allowed, MergeMethod::Merge),
                (r.squash_merge_allowed, MergeMethod::Squash),
                (r.rebase_merge_allowed, MergeMethod::Rebase),
            ]
            .into_iter()
            .filter_map(|(allowed, m)| allowed.then_some(m))
            .collect()
        })
        .unwrap_or_default();
    let Some(pr) = repo.and_then(|r| r.pull_request) else {
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
    let mut review_comments = Vec::new();
    let mut reviews = Vec::new();
    for r in pr.reviews.into_items() {
        // The body of a review is a comment without a file.
        if let (false, Some(at)) = (r.body.trim().is_empty(), r.submitted_at) {
            review_comments.push(ReviewComment {
                author: login_ref(r.author.as_ref()),
                path: None,
                line: None,
                body: r.body.clone(),
                at,
            });
        }
        reviews.push(Review {
            id: r.database_id,
            author: login(r.author),
            state: r.state,
            body: r.body,
        });
    }
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
        for c in thread.comments.into_items() {
            if let Some(at) = c.created_at {
                review_comments.push(ReviewComment {
                    author: login_ref(c.author.as_ref()),
                    path: c.path.clone(),
                    line: c.line,
                    body: c.body.clone(),
                    at,
                });
            }
            comments.push(comment(c, CommentKind::Review));
        }
    }
    review_comments.sort_by_key(|c| std::cmp::Reverse(c.at));
    review_comments.truncate(MAX_REVIEW_COMMENTS);
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
        review_comments,
        merge_methods,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::gh::{AccessProblem, classify};

    pub(crate) const GRAPHQL: &str = r#"{"data":{"repository":{
      "mergeCommitAllowed":true,"squashMergeAllowed":true,"rebaseMergeAllowed":false,
      "pullRequest":{
      "state":"OPEN","isDraft":true,"merged":false,"headRefOid":"abc123","baseRefName":"main",
      "title":"Add things","url":"https://github.com/me/repo/pull/42","reviewDecision":"CHANGES_REQUESTED",
      "commits":{"nodes":[{"commit":{"statusCheckRollup":{"contexts":{"nodes":[
        {"__typename":"CheckRun","name":"build","status":"COMPLETED","conclusion":"SUCCESS","detailsUrl":"https://ci/1","startedAt":"2026-10-20T09:00:00Z","completedAt":"2026-10-20T09:02:30Z"},
        {"__typename":"CheckRun","name":"test","status":"COMPLETED","conclusion":"FAILURE","detailsUrl":"https://ci/2"},
        {"__typename":"CheckRun","name":"cancelled","status":"COMPLETED","conclusion":"CANCELLED","detailsUrl":null},
        {"__typename":"CheckRun","name":"skipped","status":"COMPLETED","conclusion":"SKIPPED","detailsUrl":null},
        {"__typename":"CheckRun","name":"neutral","status":"COMPLETED","conclusion":"NEUTRAL","detailsUrl":null},
        {"__typename":"CheckRun","name":"slow","status":"COMPLETED","conclusion":"TIMED_OUT","detailsUrl":null},
        {"__typename":"CheckRun","name":"queued","status":"QUEUED","conclusion":null,"detailsUrl":null,"startedAt":"2026-10-20T09:05:00Z","completedAt":null},
        {"__typename":"StatusContext","context":"legacy/ci","state":"ERROR","targetUrl":"https://legacy/1"},
        {"__typename":"StatusContext","context":"legacy/lint","state":"SUCCESS","targetUrl":null,"createdAt":"2026-10-20T09:01:00Z"}
      ]}}}}]},
      "reviews":{"nodes":[
        {"databaseId":11,"state":"CHANGES_REQUESTED","author":{"login":"alice"},"body":"Please fix","submittedAt":"2026-10-20T10:00:00Z"},
        {"databaseId":12,"state":"DISMISSED","author":null,"body":"","submittedAt":"2026-10-20T11:00:00Z"}]},
      "comments":{"nodes":[{"databaseId":21,"author":{"login":"bob"},"body":"Looks odd","createdAt":"2026-10-20T10:30:00Z"}]},
      "reviewThreads":{"nodes":[{"comments":{"nodes":[
        {"databaseId":31,"author":{"login":"alice"},"path":"src/lib.rs","line":7,"body":"Rename","createdAt":"2026-10-20T10:30:00Z"}]}}]}
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
    fn checks_carry_their_times() {
        let s = parse_snapshot(GRAPHQL).unwrap();
        let at = |t: &str| Some(t.parse::<DateTime<Utc>>().unwrap());
        assert_eq!(s.checks[0].started_at, at("2026-10-20T09:00:00Z"));
        assert_eq!(s.checks[0].finished_at, at("2026-10-20T09:02:30Z"));
        // Running: started, not finished.
        assert_eq!(s.checks[6].started_at, at("2026-10-20T09:05:00Z"));
        assert_eq!(s.checks[6].finished_at, None);
        // A commit status has one time.
        assert_eq!(s.checks[8].started_at, at("2026-10-20T09:01:00Z"));
        assert_eq!(s.checks[8].finished_at, at("2026-10-20T09:01:00Z"));
        assert_eq!(s.checks[7].started_at, None);
    }

    #[test]
    fn review_comments_are_threads_and_review_bodies_newest_first() {
        let s = parse_snapshot(GRAPHQL).unwrap();
        let got: Vec<_> = s
            .review_comments
            .iter()
            .map(|c| {
                (
                    c.author.as_str(),
                    c.path.as_deref(),
                    c.line,
                    c.body.as_str(),
                )
            })
            .collect();
        // The dismissed review has no body: not listed.
        assert_eq!(
            got,
            [
                ("alice", Some("src/lib.rs"), Some(7), "Rename"),
                ("alice", None, None, "Please fix"),
            ]
        );
    }

    #[test]
    fn only_the_newest_fifty_review_comments_are_kept() {
        let nodes: Vec<String> = (0..60)
            .map(|i| {
                format!(
                    r#"{{"databaseId":{i},"author":{{"login":"a"}},"path":"f","line":1,"body":"c{i}","createdAt":"2026-10-20T10:{:02}:00Z"}}"#,
                    i
                )
            })
            .collect();
        let json = format!(
            r#"{{"data":{{"repository":{{"pullRequest":{{"state":"OPEN","isDraft":false,"headRefOid":"a","baseRefName":"main","title":"t","reviewDecision":null,
            "commits":{{"nodes":[]}},"reviews":{{"nodes":[]}},"comments":{{"nodes":[]}},
            "reviewThreads":{{"nodes":[{{"comments":{{"nodes":[{}]}}}}]}}}}}}}}}}"#,
            nodes.join(",")
        );
        let s = parse_snapshot(&json).unwrap();
        assert_eq!(s.review_comments.len(), 50);
        assert_eq!(s.review_comments[0].body, "c59");
        assert_eq!(s.review_comments[49].body, "c10");
        // The repo flags are absent: methods unknown.
        assert!(s.merge_methods.is_empty());
    }

    #[test]
    fn merge_methods_follow_the_repository_flags() {
        let s = parse_snapshot(GRAPHQL).unwrap();
        assert_eq!(s.merge_methods, [MergeMethod::Merge, MergeMethod::Squash]);
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
