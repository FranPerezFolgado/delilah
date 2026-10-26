//! The data of pull requests: what GitHub says about one (the snapshot), the marks of
//! what was already notified, the badge summary. A leaf module: it depends on nothing
//! else in the crate, so `gh`, `store_prs` and `prs` can all use it.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PrState {
    #[serde(alias = "OPEN")]
    Open,
    #[serde(alias = "MERGED")]
    Merged,
    #[serde(alias = "CLOSED")]
    Closed,
}

/// How one check came out; `Neutral` is cancelled, skipped, neutral or stale, which
/// never count as a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Pending,
    Success,
    Failure,
    Neutral,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    pub name: String,
    pub outcome: Outcome,
    #[serde(default)]
    pub url: Option<String>,
    /// When the check started and ended; `None` when GitHub does not say (or not yet).
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub finished_at: Option<DateTime<Utc>>,
}

/// The CI of a head as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Rollup {
    Pending,
    Success,
    Failure,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewDecision {
    #[serde(alias = "APPROVED")]
    Approved,
    #[serde(alias = "CHANGES_REQUESTED")]
    ChangesRequested,
    #[serde(alias = "REVIEW_REQUIRED")]
    ReviewRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewState {
    #[serde(alias = "APPROVED")]
    Approved,
    #[serde(alias = "CHANGES_REQUESTED")]
    ChangesRequested,
    #[serde(alias = "COMMENTED")]
    Commented,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Review {
    pub id: i64,
    pub author: String,
    pub state: ReviewState,
    pub body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CommentKind {
    Issue,
    Review,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    pub id: i64,
    pub kind: CommentKind,
    pub author: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub line: Option<i64>,
    pub body: String,
}

/// A merge method a repository can allow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MergeMethod {
    Merge,
    Squash,
    Rebase,
}

impl MergeMethod {
    /// The flag of `gh pr merge`.
    pub fn flag(self) -> &'static str {
        match self {
            Self::Merge => "--merge",
            Self::Squash => "--squash",
            Self::Rebase => "--rebase",
        }
    }
}

/// A review comment for the PR tab: on a file line, or the body of a review (no path).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewComment {
    pub author: String,
    pub path: Option<String>,
    pub line: Option<i64>,
    pub body: String,
    pub at: DateTime<Utc>,
}

/// What GitHub said about a PR at the last check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub state: PrState,
    pub draft: bool,
    pub title: String,
    /// Head commit sha.
    pub head: String,
    pub base: String,
    pub checks: Vec<Check>,
    pub review_decision: Option<ReviewDecision>,
    pub reviews: Vec<Review>,
    pub comments: Vec<Comment>,
    /// The newest 50 review comments, newest first. Snapshots stored before they were
    /// read lack the field.
    #[serde(default)]
    pub review_comments: Vec<ReviewComment>,
    /// What the repository allows (`merge`, `squash`, `rebase`); empty when unknown.
    #[serde(default)]
    pub merge_methods: Vec<MergeMethod>,
}

/// The highest ids already seen, so nothing is notified twice (see research R4).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Marks {
    /// Head already notified as failed.
    pub ci_failed_sha: Option<String>,
    pub review: i64,
    pub issue_comment: i64,
    pub review_comment: i64,
}

/// A PR as `gh pr list` shows it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ListedPr {
    pub number: i64,
    pub url: String,
    pub state: PrState,
    #[serde(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,
}

/// One badge per task: the worst of each field across its PRs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrSummary {
    pub task: String,
    pub state: PrState,
    pub draft: bool,
    pub ci: Rollup,
    pub review_decision: Option<ReviewDecision>,
    pub count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_stored_snapshot_still_parses() {
        let old = r#"{"state":"open","draft":false,"title":"t","head":"abc","base":"main",
          "checks":[{"name":"ci","outcome":"pending","url":null}],
          "review_decision":null,"reviews":[],"comments":[]}"#;
        let s: Snapshot = serde_json::from_str(old).unwrap();
        assert!(s.merge_methods.is_empty());
        assert!(s.review_comments.is_empty());
        assert_eq!(
            (s.checks[0].started_at, s.checks[0].finished_at),
            (None, None)
        );
    }

    #[test]
    fn merge_methods_serialize_lowercase() {
        let v =
            serde_json::to_value([MergeMethod::Merge, MergeMethod::Squash, MergeMethod::Rebase]);
        assert_eq!(v.unwrap(), serde_json::json!(["merge", "squash", "rebase"]));
    }
}
