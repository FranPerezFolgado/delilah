//! Pull requests linked to tasks: the snapshot types and the pure helpers over them
//! (what a check found, which PR to pick, the badge summary).

// The loop and the API that use these land in the next packages.
#![allow(dead_code)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::store_prs::PrRow;

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

/// Failure beats pending beats success; neutral checks are ignored; no checks at all
/// (or only neutral ones) is `None`.
pub fn ci_outcome(checks: &[Check]) -> Rollup {
    let has = |o| checks.iter().any(|c| c.outcome == o);
    if has(Outcome::Failure) {
        Rollup::Failure
    } else if has(Outcome::Pending) {
        Rollup::Pending
    } else if has(Outcome::Success) {
        Rollup::Success
    } else {
        Rollup::None
    }
}

/// The PR of a branch to follow: open first, then merged, then closed; the most
/// recently updated within each.
pub fn pick(prs: &[ListedPr]) -> Option<&ListedPr> {
    let rank = |s: PrState| match s {
        PrState::Open => 0,
        PrState::Merged => 1,
        PrState::Closed => 2,
    };
    prs.iter()
        .min_by_key(|p| (rank(p.state), std::cmp::Reverse(p.updated_at)))
}

/// The badge of a task's PRs; `None` if it has none. A PR not checked yet counts as
/// open.
pub(crate) fn summary(task: &str, rows: &[PrRow]) -> Option<PrSummary> {
    if rows.is_empty() {
        return None;
    }
    let snaps: Vec<Option<&Snapshot>> = rows.iter().map(|r| r.snapshot.as_ref()).collect();
    let state_of = |s: &Option<&Snapshot>| s.map_or(PrState::Open, |s| s.state);
    let state = if snaps.iter().any(|s| state_of(s) == PrState::Open) {
        PrState::Open
    } else if snaps.iter().any(|s| state_of(s) == PrState::Closed) {
        PrState::Closed
    } else {
        PrState::Merged
    };
    let draft = snaps
        .iter()
        .flatten()
        .any(|s| s.state == PrState::Open && s.draft);
    let rollups: Vec<Rollup> = snaps
        .iter()
        .flatten()
        .map(|s| ci_outcome(&s.checks))
        .collect();
    let ci = [Rollup::Failure, Rollup::Pending, Rollup::Success]
        .into_iter()
        .find(|r| rollups.contains(r))
        .unwrap_or(Rollup::None);
    let decisions: Vec<ReviewDecision> = snaps
        .iter()
        .flatten()
        .filter_map(|s| s.review_decision)
        .collect();
    let review_decision = [
        ReviewDecision::ChangesRequested,
        ReviewDecision::ReviewRequired,
        ReviewDecision::Approved,
    ]
    .into_iter()
    .find(|d| decisions.contains(d));
    Some(PrSummary {
        task: task.to_string(),
        state,
        draft,
        ci,
        review_decision,
        count: rows.len(),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn check(name: &str, outcome: Outcome) -> Check {
        Check {
            name: name.into(),
            outcome,
            url: None,
        }
    }

    pub(crate) fn snapshot(state: PrState, checks: Vec<Check>) -> Snapshot {
        Snapshot {
            state,
            draft: false,
            title: "t".into(),
            head: "abc".into(),
            base: "main".into(),
            checks,
            review_decision: None,
            reviews: vec![],
            comments: vec![],
        }
    }

    fn row(snapshot: Option<Snapshot>) -> PrRow {
        PrRow {
            id: 1,
            task_key: "PROJ-1".into(),
            repo: ".".into(),
            host: "github.com".into(),
            owner_name: "me/repo".into(),
            number: 1,
            url: "https://github.com/me/repo/pull/1".into(),
            linked_by: "detected".into(),
            snapshot,
            marks: Marks::default(),
            checked_at: None,
            error: None,
            created_at: Utc::now(),
        }
    }

    fn listed(number: i64, state: PrState, minutes: i64) -> ListedPr {
        ListedPr {
            number,
            url: format!("https://github.com/me/repo/pull/{number}"),
            state,
            updated_at: DateTime::from_timestamp(1_700_000_000 + minutes * 60, 0).unwrap(),
        }
    }

    #[test]
    fn any_failure_makes_the_rollup_a_failure() {
        let checks = [
            check("a", Outcome::Success),
            check("b", Outcome::Pending),
            check("c", Outcome::Failure),
        ];
        assert_eq!(ci_outcome(&checks), Rollup::Failure);
    }

    #[test]
    fn neutral_checks_are_ignored() {
        let only_neutral = [check("a", Outcome::Neutral)];
        assert_eq!(ci_outcome(&only_neutral), Rollup::None);
        let with_success = [check("a", Outcome::Neutral), check("b", Outcome::Success)];
        assert_eq!(ci_outcome(&with_success), Rollup::Success);
    }

    #[test]
    fn pending_without_failure_is_pending_and_empty_is_none() {
        let checks = [check("a", Outcome::Success), check("b", Outcome::Pending)];
        assert_eq!(ci_outcome(&checks), Rollup::Pending);
        assert_eq!(ci_outcome(&[]), Rollup::None);
    }

    #[test]
    fn pick_prefers_open_then_merged_then_closed_newest_first() {
        let prs = [
            listed(1, PrState::Closed, 50),
            listed(2, PrState::Merged, 40),
            listed(3, PrState::Open, 10),
            listed(4, PrState::Open, 20),
        ];
        assert_eq!(pick(&prs).unwrap().number, 4);
        assert_eq!(pick(&prs[..2]).unwrap().number, 2);
        let closed = [listed(5, PrState::Closed, 1), listed(6, PrState::Closed, 9)];
        assert_eq!(pick(&closed).unwrap().number, 6);
        assert!(pick(&[]).is_none());
    }

    #[test]
    fn list_states_parse_as_gh_prints_them() {
        let json = r#"[{"number":7,"url":"https://x/pull/7","state":"MERGED","updatedAt":"2026-10-20T10:00:00Z"}]"#;
        let prs: Vec<ListedPr> = serde_json::from_str(json).unwrap();
        assert_eq!(prs[0].state, PrState::Merged);
    }

    #[test]
    fn summary_takes_the_worst_of_each_field() {
        let mut open = snapshot(
            PrState::Open,
            vec![check("a", Outcome::Pending), check("b", Outcome::Success)],
        );
        open.draft = true;
        open.review_decision = Some(ReviewDecision::Approved);
        let mut other = snapshot(PrState::Merged, vec![check("a", Outcome::Failure)]);
        other.review_decision = Some(ReviewDecision::ChangesRequested);
        let rows = [row(Some(open)), row(Some(other))];
        let s = summary("PROJ-1", &rows).unwrap();
        assert_eq!(s.state, PrState::Open);
        assert!(s.draft);
        assert_eq!(s.ci, Rollup::Failure);
        assert_eq!(s.review_decision, Some(ReviewDecision::ChangesRequested));
        assert_eq!(s.count, 2);
    }

    #[test]
    fn summary_of_merged_prs_is_merged_and_unchecked_counts_as_open() {
        let merged = [row(Some(snapshot(PrState::Merged, vec![])))];
        assert_eq!(summary("PROJ-1", &merged).unwrap().state, PrState::Merged);
        assert_eq!(
            summary("PROJ-1", &[row(None)]).unwrap().state,
            PrState::Open
        );
        assert!(summary("PROJ-1", &[]).is_none());
    }

    #[test]
    fn summary_serializes_as_the_contract_says() {
        let s = summary(
            "PROJ-1",
            &[row(Some(snapshot(
                PrState::Open,
                vec![check("a", Outcome::Success)],
            )))],
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&s).unwrap(),
            serde_json::json!({"task":"PROJ-1","state":"open","draft":false,"ci":"success","review_decision":null,"count":1})
        );
    }
}
