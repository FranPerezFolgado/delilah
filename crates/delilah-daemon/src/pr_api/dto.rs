//! What the PR routes send and receive.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::pr_types::{Check, PrState, ReviewDecision, Rollup, Snapshot};
use crate::prs;
use crate::store_prs::PrRow;

#[derive(Serialize)]
pub struct SnapshotView {
    state: PrState,
    draft: bool,
    title: String,
    head: String,
    base: String,
    checks: Vec<Check>,
    ci: Rollup,
    review_decision: Option<ReviewDecision>,
    comments: usize,
}

#[derive(Serialize)]
pub struct PrView {
    id: i64,
    repo: String,
    host: String,
    owner_name: String,
    number: i64,
    url: String,
    linked_by: String,
    checked_at: Option<DateTime<Utc>>,
    error: Option<String>,
    snapshot: Option<SnapshotView>,
}

#[derive(Serialize)]
pub struct Problem {
    pub(super) kind: String,
    pub(super) message: String,
}

#[derive(Serialize)]
pub struct TaskPrs {
    pub(super) account: Option<String>,
    pub(super) problem: Option<Problem>,
    pub(super) prs: Vec<PrView>,
}

#[derive(Deserialize)]
pub struct LinkRequest {
    pub(super) url: String,
}

#[derive(Deserialize)]
pub struct ProposalQuery {
    pub(super) repo: Option<String>,
}

#[derive(Serialize)]
pub struct Proposal {
    pub(super) repo: String,
    pub(super) title: String,
    pub(super) body: String,
    pub(super) base: String,
    pub(super) needs_push: bool,
    /// The task's repos that have no open PR yet.
    pub(super) repos: Vec<String>,
}

#[derive(Deserialize)]
pub struct OpenRequest {
    #[serde(default)]
    pub(super) repo: Option<String>,
    pub(super) title: String,
    #[serde(default)]
    pub(super) body: String,
    #[serde(default)]
    pub(super) draft: bool,
}

impl From<&Snapshot> for SnapshotView {
    fn from(s: &Snapshot) -> Self {
        Self {
            state: s.state,
            draft: s.draft,
            title: s.title.clone(),
            head: s.head.clone(),
            base: s.base.clone(),
            checks: s.checks.clone(),
            ci: prs::ci_outcome(&s.checks),
            review_decision: s.review_decision,
            comments: s.comments.len(),
        }
    }
}

impl From<&PrRow> for PrView {
    fn from(row: &PrRow) -> Self {
        Self {
            id: row.id,
            repo: row.repo.clone(),
            host: row.host.clone(),
            owner_name: row.owner_name.clone(),
            number: row.number,
            url: row.url.clone(),
            linked_by: row.linked_by.clone(),
            checked_at: row.checked_at,
            error: row.error.clone(),
            snapshot: row.snapshot.as_ref().map(SnapshotView::from),
        }
    }
}
