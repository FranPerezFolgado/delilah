//! Pull requests linked to tasks: the pure logic over the snapshot types (what a check
//! found, which PR to pick, the badge summary, the back-off after a failure). The I/O
//! lives in `pr_loop`; the data types in `pr_types`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::pr_types::{
    Check, ListedPr, Outcome, PrState, PrSummary, ReviewDecision, Rollup, Snapshot,
};
use crate::store_prs::PrRow;

const BACKOFF_BASE: Duration = Duration::from_secs(30);
const BACKOFF_CAP: Duration = Duration::from_secs(600);

/// When to try each key (a repo, `host/owner/name`) again after `gh` failed: 30 s,
/// doubling per failure in a row up to 10 min, or the moment GitHub said the rate limit
/// resets.
#[derive(Debug, Default)]
pub struct Backoff {
    /// Failures in a row and when the next try is due.
    entries: HashMap<String, (u32, Instant)>,
}

impl Backoff {
    /// Whether `key` may be tried at `now`.
    pub fn due(&self, key: &str, now: Instant) -> bool {
        self.entries.get(key).is_none_or(|(_, at)| now >= *at)
    }

    pub fn fail(&mut self, key: &str, now: Instant, reset: Option<Instant>) {
        let failures = self
            .entries
            .get(key)
            .map_or(0, |(n, _)| *n)
            .saturating_add(1);
        let wait = BACKOFF_BASE
            .saturating_mul(2u32.saturating_pow((failures - 1).min(16)))
            .min(BACKOFF_CAP);
        self.entries
            .insert(key.to_string(), (failures, reset.unwrap_or(now + wait)));
    }

    pub fn ok(&mut self, key: &str) {
        self.entries.remove(key);
    }
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
    use chrono::{DateTime, Utc};

    use super::*;
    use crate::pr_types::Marks;

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

    #[test]
    fn backoff_doubles_up_to_the_cap_and_ok_forgets() {
        let t0 = Instant::now();
        let mut b = Backoff::default();
        assert!(b.due("r", t0));
        b.fail("r", t0, None);
        assert!(!b.due("r", t0 + Duration::from_secs(29)));
        assert!(b.due("r", t0 + Duration::from_secs(30)));
        b.fail("r", t0, None);
        assert!(!b.due("r", t0 + Duration::from_secs(59)));
        assert!(b.due("r", t0 + Duration::from_secs(60)));
        for _ in 0..10 {
            b.fail("r", t0, None);
        }
        assert!(!b.due("r", t0 + Duration::from_secs(599)));
        assert!(b.due("r", t0 + Duration::from_secs(600)));
        assert!(b.due("other", t0));
        b.ok("r");
        assert!(b.due("r", t0));
        b.fail("r", t0, None);
        assert!(!b.due("r", t0 + Duration::from_secs(29)));
    }

    #[test]
    fn backoff_waits_for_the_rate_limit_reset_when_known() {
        let t0 = Instant::now();
        let mut b = Backoff::default();
        b.fail("r", t0, Some(t0 + Duration::from_secs(3000)));
        assert!(!b.due("r", t0 + Duration::from_secs(2999)));
        assert!(b.due("r", t0 + Duration::from_secs(3000)));
    }
}
