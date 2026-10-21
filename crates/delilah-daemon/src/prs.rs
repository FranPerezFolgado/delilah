//! Pull requests linked to tasks: the pure logic over the snapshot types (what a check
//! found, which PR to pick, the badge summary, the back-off after a failure). The I/O
//! lives in `pr_loop`; the data types in `pr_types`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::pr_types::{
    Check, Comment, CommentKind, ListedPr, Marks, Outcome, PrState, PrSummary, Review,
    ReviewDecision, ReviewState, Rollup, Snapshot,
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

/// Something a check found that needs the user: it becomes a notice (an inbox item).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoticeDraft {
    /// The head failed CI; the checks that failed.
    CiFailed {
        sha: String,
        checks: Vec<Check>,
    },
    ChangesRequested(Review),
    /// New comments (and reviews with a body) by others, in one notice.
    Comments(Vec<Comment>),
}

impl NoticeDraft {
    /// The notice kind stored in `pr_notices`.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::CiFailed { .. } => "ci-failed",
            Self::ChangesRequested(_) => "changes-requested",
            Self::Comments(_) => "comments",
        }
    }

    /// What makes it a different notice from the one before: sha, review id, or the
    /// highest comment id.
    pub fn cause(&self) -> String {
        match self {
            Self::CiFailed { sha, .. } => sha.clone(),
            Self::ChangesRequested(r) => r.id.to_string(),
            Self::Comments(cs) => cs.iter().map(|c| c.id).max().unwrap_or(0).to_string(),
        }
    }
}

/// Good or neutral news: told in a notification, never stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Info {
    Approved,
    Merged,
    Closed,
}

/// What a check found new, given the last snapshot, the marks and the login `me` of
/// the signed-in account (its own comments are not news). Returns the notices to
/// raise, the marks after this check and the info to tell. The first snapshot
/// (`prev` is `None`) takes the reviews and comments there as the baseline, but a
/// failed head is still notified. A PR that is not open raises no notices.
pub fn diff(
    prev: Option<&Snapshot>,
    marks: &Marks,
    next: &Snapshot,
    me: &str,
) -> (Vec<NoticeDraft>, Marks, Vec<Info>) {
    let mut after = marks.clone();
    let newest = |kind| {
        next.comments
            .iter()
            .filter(move |c| c.kind == kind)
            .map(|c| c.id)
            .max()
    };
    after.review = marks
        .review
        .max(next.reviews.iter().map(|r| r.id).max().unwrap_or(0));
    after.issue_comment = marks
        .issue_comment
        .max(newest(CommentKind::Issue).unwrap_or(0));
    after.review_comment = marks
        .review_comment
        .max(newest(CommentKind::Review).unwrap_or(0));

    let mut drafts = Vec::new();
    if next.state == PrState::Open {
        if ci_outcome(&next.checks) == Rollup::Failure
            && marks.ci_failed_sha.as_deref() != Some(next.head.as_str())
        {
            let checks = next
                .checks
                .iter()
                .filter(|c| c.outcome == Outcome::Failure)
                .cloned()
                .collect();
            drafts.push(NoticeDraft::CiFailed {
                sha: next.head.clone(),
                checks,
            });
            after.ci_failed_sha = Some(next.head.clone());
        }
        if prev.is_some() {
            let new_reviews = || {
                next.reviews
                    .iter()
                    .filter(|r| r.id > marks.review && r.author != me)
            };
            for r in new_reviews().filter(|r| r.state == ReviewState::ChangesRequested) {
                drafts.push(NoticeDraft::ChangesRequested(r.clone()));
            }
            let mut comments: Vec<Comment> = next
                .comments
                .iter()
                .filter(|c| {
                    let seen = match c.kind {
                        CommentKind::Issue => marks.issue_comment,
                        CommentKind::Review => marks.review_comment,
                    };
                    c.id > seen && c.author != me
                })
                .cloned()
                .collect();
            comments.extend(
                new_reviews()
                    .filter(|r| {
                        matches!(r.state, ReviewState::Approved | ReviewState::Commented)
                            && !r.body.trim().is_empty()
                    })
                    .map(|r| Comment {
                        id: r.id,
                        kind: CommentKind::Review,
                        author: r.author.clone(),
                        path: None,
                        line: None,
                        body: r.body.clone(),
                    }),
            );
            if !comments.is_empty() {
                drafts.push(NoticeDraft::Comments(comments));
            }
        }
    }

    let mut infos = Vec::new();
    if let Some(prev) = prev
        && prev.state == PrState::Open
    {
        match next.state {
            PrState::Merged => infos.push(Info::Merged),
            PrState::Closed => infos.push(Info::Closed),
            PrState::Open => {
                if next.review_decision == Some(ReviewDecision::Approved)
                    && prev.review_decision != Some(ReviewDecision::Approved)
                {
                    infos.push(Info::Approved);
                }
            }
        }
    }
    (drafts, after, infos)
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
            error_kind: None,
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

    fn review(id: i64, author: &str, state: ReviewState, body: &str) -> Review {
        Review {
            id,
            author: author.into(),
            state,
            body: body.into(),
        }
    }

    fn comment(id: i64, kind: CommentKind, author: &str) -> Comment {
        Comment {
            id,
            kind,
            author: author.into(),
            path: None,
            line: None,
            body: "b".into(),
        }
    }

    fn failing(head: &str) -> Snapshot {
        let mut s = snapshot(PrState::Open, vec![check("test", Outcome::Failure)]);
        s.head = head.into();
        s
    }

    #[test]
    fn first_snapshot_takes_reviews_and_comments_as_baseline() {
        let mut next = snapshot(PrState::Open, vec![]);
        next.reviews = vec![review(5, "alice", ReviewState::ChangesRequested, "x")];
        next.comments = vec![
            comment(8, CommentKind::Issue, "bob"),
            comment(9, CommentKind::Review, "bob"),
        ];
        let (drafts, marks, infos) = diff(None, &Marks::default(), &next, "me");
        assert!(drafts.is_empty() && infos.is_empty());
        assert_eq!(
            (marks.review, marks.issue_comment, marks.review_comment),
            (5, 8, 9)
        );
    }

    #[test]
    fn a_failed_head_is_notified_once_per_head() {
        let first = failing("a");
        let (drafts, marks, _) = diff(None, &Marks::default(), &first, "me");
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].kind(), "ci-failed");
        assert_eq!(drafts[0].cause(), "a");
        assert_eq!(marks.ci_failed_sha.as_deref(), Some("a"));

        let (drafts, marks, _) = diff(Some(&first), &marks, &failing("a"), "me");
        assert!(drafts.is_empty());
        let (drafts, marks, _) = diff(Some(&first), &marks, &failing("b"), "me");
        assert_eq!(drafts.len(), 1);
        assert_eq!(marks.ci_failed_sha.as_deref(), Some("b"));
    }

    #[test]
    fn changes_requested_by_another_login_is_a_notice() {
        let prev = snapshot(PrState::Open, vec![]);
        let mut next = prev.clone();
        next.reviews = vec![
            review(3, "alice", ReviewState::ChangesRequested, "fix"),
            review(4, "me", ReviewState::ChangesRequested, "own"),
        ];
        let (drafts, marks, _) = diff(Some(&prev), &Marks::default(), &next, "me");
        assert_eq!(
            drafts,
            [NoticeDraft::ChangesRequested(next.reviews[0].clone())]
        );
        assert_eq!(drafts[0].cause(), "3");
        assert_eq!(marks.review, 4);
        let (again, _, _) = diff(Some(&next), &marks, &next, "me");
        assert!(again.is_empty());
    }

    #[test]
    fn comments_of_people_and_bots_make_one_notice_but_not_my_own() {
        let prev = snapshot(PrState::Open, vec![]);
        let mut next = prev.clone();
        next.comments = vec![
            comment(10, CommentKind::Issue, "bob"),
            comment(11, CommentKind::Issue, "ci[bot]"),
            comment(12, CommentKind::Review, "me"),
        ];
        let (drafts, marks, _) = diff(Some(&prev), &Marks::default(), &next, "me");
        assert_eq!(drafts.len(), 1);
        let NoticeDraft::Comments(cs) = &drafts[0] else {
            panic!("{drafts:?}")
        };
        assert_eq!(cs.iter().map(|c| c.id).collect::<Vec<_>>(), [10, 11]);
        assert_eq!(drafts[0].cause(), "11");
        assert_eq!((marks.issue_comment, marks.review_comment), (11, 12));

        let mut own = prev.clone();
        own.comments = vec![comment(20, CommentKind::Issue, "me")];
        let (drafts, marks, _) = diff(Some(&prev), &Marks::default(), &own, "me");
        assert!(drafts.is_empty());
        assert_eq!(marks.issue_comment, 20);
    }

    #[test]
    fn reviews_with_a_body_count_as_comments() {
        let prev = snapshot(PrState::Open, vec![]);
        let mut next = prev.clone();
        next.reviews = vec![
            review(6, "alice", ReviewState::Approved, "nice"),
            review(7, "bob", ReviewState::Commented, ""),
        ];
        let (drafts, _, _) = diff(Some(&prev), &Marks::default(), &next, "me");
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].kind(), "comments");
    }

    #[test]
    fn approval_merge_and_close_are_info() {
        let prev = snapshot(PrState::Open, vec![]);
        let mut approved = prev.clone();
        approved.review_decision = Some(ReviewDecision::Approved);
        let (_, _, infos) = diff(Some(&prev), &Marks::default(), &approved, "me");
        assert_eq!(infos, [Info::Approved]);
        let (_, _, infos) = diff(Some(&approved), &Marks::default(), &approved, "me");
        assert!(infos.is_empty());

        let merged = snapshot(PrState::Merged, vec![]);
        let (_, _, infos) = diff(Some(&prev), &Marks::default(), &merged, "me");
        assert_eq!(infos, [Info::Merged]);
        let closed = snapshot(PrState::Closed, vec![]);
        let (_, _, infos) = diff(Some(&prev), &Marks::default(), &closed, "me");
        assert_eq!(infos, [Info::Closed]);
    }

    #[test]
    fn a_pr_that_is_not_open_raises_no_notices() {
        let prev = snapshot(PrState::Open, vec![]);
        let mut merged = failing("z");
        merged.state = PrState::Merged;
        let (drafts, _, _) = diff(Some(&prev), &Marks::default(), &merged, "me");
        assert!(drafts.is_empty());
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
