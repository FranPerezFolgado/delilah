//! What a check of a PR found that is worth telling: saves the snapshot with its
//! notices and marks, and notifies when no app is open (the pure part is `prs::diff`).

use std::collections::HashSet;

use chrono::Utc;
use delilah_core::TaskKey;
use serde_json::{Value, json};

use crate::lang::Lang;
use crate::pr_types::{CommentKind, PrState, Rollup, Snapshot};
use crate::prs::{Info, NoticeDraft, capped, ci_outcome, diff, one_line};
use crate::state::AppState;
use crate::store::StoreError;
use crate::store_prs::{NewNotice, PrRow};

/// Every kind of notice a check can raise (they are all resolved when the PR ends).
const PR_KINDS: [&str; 3] = ["ci-failed", "changes-requested", "comments"];

/// Longest a PR title or check name goes into a notice text or a notification.
const SHORT: usize = 120;
/// Longest a comment or review body goes into a notice (it is typed into a terminal).
const BODY: usize = 2000;
/// Most comments one notice keeps.
const COMMENTS: usize = 20;
/// Most authors named in the text of a comments notice.
const NAMED: usize = 3;

/// What saving a check changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Checked {
    /// The row was written (clients show when it was checked).
    pub changed: bool,
    /// Notices were raised or resolved (the inbox changed).
    pub notices_changed: bool,
}

/// Saves what a check read of `row`'s PR, with the notices it raised and resolved, and
/// notifies the new ones when no app is listening.
pub(super) async fn record(
    app: &AppState,
    row: &PrRow,
    snap: &Snapshot,
) -> Result<Checked, StoreError> {
    let me = match app.gh.user(&row.host).await {
        Ok(login) => login,
        Err(e) => {
            // Nobody's comment is "mine" then: better to notify too much than to miss one.
            tracing::warn!(task = %row.task_key, "cannot tell who I am on {}: {e}", row.host);
            String::new()
        }
    };
    let (drafts, marks, infos) = diff(row.snapshot.as_ref(), &row.marks, snap, &me);

    let mut resolve: Vec<&str> = Vec::new();
    if snap.state != PrState::Open {
        resolve.extend(PR_KINDS);
    } else if drafts
        .iter()
        .any(|d| matches!(d, NoticeDraft::CiFailed { .. }))
        // One failure item per PR: the new head's replaces the older head's.
        || (ci_outcome(&snap.checks) == Rollup::Success
            // Green on a later commit, even one first seen while pending.
            && row.marks.ci_failed_sha.as_deref() != Some(snap.head.as_str()))
    {
        resolve.push("ci-failed");
    }

    let lang = app.lang();
    let notices: Vec<NewNotice> = drafts
        .iter()
        .map(|d| new_notice(lang, &row.repo, d))
        .collect();
    let before: HashSet<i64> = app
        .store
        .open_notices()
        .await?
        .into_iter()
        .map(|n| n.id)
        .collect();
    app.store
        .save_check(row.id, snap, &marks, Utc::now(), &notices, &resolve)
        .await?;
    let after = app.store.open_notices().await?;
    let after_ids: HashSet<i64> = after.iter().map(|n| n.id).collect();

    let key = TaskKey::new(row.task_key.clone());
    if !app.events.listening() {
        for (draft, notice) in drafts.iter().zip(&notices) {
            let raised = after.iter().any(|n| {
                !before.contains(&n.id)
                    && n.task_key == row.task_key
                    && n.repo == notice.repo
                    && n.kind == notice.kind
                    && n.cause == notice.cause
            });
            if raised {
                let (title, body) = draft_message(lang, &key, draft, snap);
                app.notifier.send(title, body);
            }
        }
        for info in infos {
            let (title, body) = info_message(lang, &key, info, snap);
            app.notifier.send(title, body);
        }
    }
    Ok(Checked {
        changed: true,
        notices_changed: before != after_ids,
    })
}

/// What the user sees of a name that came from the PR: one short line.
fn short(s: &str) -> String {
    one_line(s, SHORT)
}

/// The authors of `comments`, in order, at most `NAMED` and "and others" after them.
fn authors(lang: Lang, comments: &[crate::pr_types::Comment]) -> String {
    let mut names: Vec<String> = Vec::new();
    for c in comments {
        let name = short(&c.author);
        if !names.contains(&name) {
            names.push(name);
        }
    }
    let more = names.len() > NAMED;
    names.truncate(NAMED);
    let mut out = names.join(", ");
    if more {
        out.push(' ');
        out.push_str(lang.pick("y otros", "and others"));
    }
    out
}

fn new_notice(lang: Lang, repo: &str, draft: &NoticeDraft) -> NewNotice {
    let (text, detail) = match draft {
        NoticeDraft::CiFailed { sha, checks } => {
            let names = checks
                .iter()
                .map(|c| short(&c.name))
                .collect::<Vec<_>>()
                .join(", ");
            (
                format!("{}: {names}", lang.pick("CI falló", "CI failed")),
                json!({
                    "sha": sha,
                    "checks": checks
                        .iter()
                        .map(|c| json!({"name": c.name, "url": c.url}))
                        .collect::<Vec<_>>(),
                }),
            )
        }
        NoticeDraft::ChangesRequested(r) => (
            format!(
                "{} {}",
                short(&r.author),
                lang.pick("pidió cambios", "requested changes")
            ),
            json!({"review": {
                "id": r.id,
                "author": r.author,
                "body": capped(&r.body, BODY, lang.pick(" [recortado]", " [truncated]")),
            }}),
        ),
        NoticeDraft::Comments(cs) => (
            format!(
                "{} {} {}",
                cs.len(),
                lang.pick("comentarios nuevos de", "new comments from"),
                authors(lang, cs)
            ),
            json!({
                "comments": cs
                    .iter()
                    .take(COMMENTS)
                    .map(|c| json!({
                        "id": c.id,
                        "kind": match c.kind {
                            CommentKind::Issue => "issue",
                            CommentKind::Review => "review",
                        },
                        "author": c.author,
                        "path": c.path,
                        "line": c.line,
                        "body": capped(&c.body, BODY, lang.pick(" [recortado]", " [truncated]")),
                    }))
                    .collect::<Vec<_>>(),
            }),
        ),
    };
    NewNotice {
        repo: repo.to_string(),
        kind: draft.kind().to_string(),
        cause: draft.cause(),
        text,
        detail: Some::<Value>(detail),
    }
}

fn draft_message(
    lang: Lang,
    key: &TaskKey,
    draft: &NoticeDraft,
    snap: &Snapshot,
) -> (String, String) {
    match draft {
        NoticeDraft::CiFailed { checks, .. } => (
            format!("{key} {}", lang.pick("CI falló", "CI failed")),
            checks
                .iter()
                .map(|c| short(&c.name))
                .collect::<Vec<_>>()
                .join(", "),
        ),
        NoticeDraft::ChangesRequested(_) => (
            format!(
                "{key} {}",
                lang.pick("cambios pedidos", "changes requested")
            ),
            short(&snap.title),
        ),
        NoticeDraft::Comments(_) => (
            format!("{key} {}", lang.pick("comentarios nuevos", "new comments")),
            short(&snap.title),
        ),
    }
}

fn info_message(lang: Lang, key: &TaskKey, info: Info, snap: &Snapshot) -> (String, String) {
    let title = match info {
        Info::Approved => lang.pick("aprobada", "approved"),
        Info::Merged => lang.pick("mergeada", "merged"),
        Info::Closed => lang.pick("PR cerrada", "PR closed"),
    };
    (format!("{key} {title}"), short(&snap.title))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use delilah_core::SpecPhase;

    use super::super::check_task;
    use crate::events::Topic;
    use crate::gh::fake::{
        FakeGh, PR_LIST, REPO_VIEW, check_node, ci_snapshot, comment_node, custom_snapshot,
    };
    use crate::pr_testkit::task_with_workspace;
    use crate::state::AppState;

    fn gh_with(snapshot: &str) -> Arc<FakeGh> {
        Arc::new(
            FakeGh::new()
                .on(&["repo", "view"], Ok(REPO_VIEW))
                .on(&["pr", "list"], Ok(PR_LIST))
                .on(&["api", "graphql"], Ok(snapshot))
                .with_user("me"),
        )
    }

    async fn notices(app: &AppState) -> Vec<(String, String, bool)> {
        let mut all = vec![];
        for n in app.store.open_notices().await.unwrap() {
            all.push((n.kind, n.cause, true));
        }
        all
    }

    #[tokio::test]
    async fn a_failing_head_makes_one_notice_and_one_notification_once() {
        let gh = gh_with(&ci_snapshot("OPEN", "abc", "FAILURE"));
        let (app, key, _dir) = task_with_workspace(gh, Some(SpecPhase::Review)).await;
        check_task(&app, &key, true).await;
        let open = app.store.open_notices().await.unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(
            (open[0].kind.as_str(), open[0].cause.as_str()),
            ("ci-failed", "abc")
        );
        assert_eq!(
            open[0].detail.as_ref().unwrap()["checks"][0]["name"],
            "build"
        );
        let sent = app.notifier.recorded();
        assert_eq!(sent.len(), 1);
        assert!(sent[0].title.starts_with("PROJ-1"));
        assert_eq!(sent[0].body, "build");

        check_task(&app, &key, true).await;
        assert_eq!(app.store.open_notices().await.unwrap().len(), 1);
        assert_eq!(app.notifier.recorded().len(), 1);

        // A restart: a new AppState over the same store.
        let config = crate::config::Config {
            home: std::env::temp_dir().join("delilah-restart"),
            addr: "127.0.0.1:7420".parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
        };
        let restarted = AppState::new(app.store.clone(), config)
            .with_gh(gh_with(&ci_snapshot("OPEN", "abc", "FAILURE")));
        check_task(&restarted, &key, true).await;
        assert_eq!(restarted.store.open_notices().await.unwrap().len(), 1);
        assert!(restarted.notifier.recorded().is_empty());
        assert_eq!(notices(&restarted).await.len(), 1);
    }

    #[tokio::test]
    async fn a_listening_app_gets_the_notice_but_no_notification() {
        let gh = gh_with(&ci_snapshot("OPEN", "abc", "FAILURE"));
        let (app, key, _dir) = task_with_workspace(gh, Some(SpecPhase::Review)).await;
        let mut rx = app.events.subscribe();
        check_task(&app, &key, true).await;
        assert_eq!(app.store.open_notices().await.unwrap().len(), 1);
        assert!(app.notifier.recorded().is_empty());
        let mut topics = vec![];
        while let Ok(t) = rx.try_recv() {
            topics.push(t);
        }
        assert!(topics.contains(&Topic::Inbox));
    }

    #[tokio::test]
    async fn a_later_green_head_resolves_the_notice() {
        let (app, key, _dir) = task_with_workspace(
            gh_with(&ci_snapshot("OPEN", "abc", "FAILURE")),
            Some(SpecPhase::Review),
        )
        .await;
        check_task(&app, &key, true).await;
        let app = app.with_gh(gh_with(&ci_snapshot("OPEN", "def", "SUCCESS")));
        check_task(&app, &key, true).await;
        assert!(app.store.open_notices().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn merging_or_closing_resolves_every_notice_and_tells_it() {
        for (state, word) in [("MERGED", "mergeada"), ("CLOSED", "PR cerrada")] {
            let (app, key, _dir) = task_with_workspace(
                gh_with(&ci_snapshot("OPEN", "abc", "FAILURE")),
                Some(SpecPhase::Review),
            )
            .await;
            check_task(&app, &key, true).await;
            assert_eq!(app.store.open_notices().await.unwrap().len(), 1);
            let app = app.with_gh(gh_with(&ci_snapshot(state, "abc", "FAILURE")));
            check_task(&app, &key, true).await;
            assert!(app.store.open_notices().await.unwrap().is_empty());
            let sent = app.notifier.recorded();
            assert_eq!(sent.last().unwrap().title, format!("PROJ-1 {word}"));
        }
    }

    fn pending(head: &str) -> String {
        custom_snapshot(
            head,
            "t",
            &[check_node("build", "IN_PROGRESS", None, "https://ci/1")],
            &[],
        )
    }

    async fn open_ci(app: &AppState) -> Vec<String> {
        let mut causes = vec![];
        for n in app.store.open_notices().await.unwrap() {
            if n.kind == "ci-failed" {
                causes.push(n.cause);
            }
        }
        causes
    }

    #[tokio::test]
    async fn green_on_a_commit_first_seen_pending_resolves_the_failure() {
        let (app, key, _dir) = task_with_workspace(
            gh_with(&ci_snapshot("OPEN", "abc", "FAILURE")),
            Some(SpecPhase::Review),
        )
        .await;
        check_task(&app, &key, true).await;
        assert_eq!(open_ci(&app).await, ["abc"]);
        let app = app.with_gh(gh_with(&pending("def")));
        check_task(&app, &key, true).await;
        assert_eq!(open_ci(&app).await, ["abc"]);
        let app = app.with_gh(gh_with(&ci_snapshot("OPEN", "def", "SUCCESS")));
        check_task(&app, &key, true).await;
        assert!(open_ci(&app).await.is_empty());
    }

    #[tokio::test]
    async fn a_new_failing_head_replaces_the_older_failure() {
        let (app, key, _dir) = task_with_workspace(
            gh_with(&ci_snapshot("OPEN", "abc", "FAILURE")),
            Some(SpecPhase::Review),
        )
        .await;
        check_task(&app, &key, true).await;
        let app = app.with_gh(gh_with(&ci_snapshot("OPEN", "def", "FAILURE")));
        check_task(&app, &key, true).await;
        assert_eq!(open_ci(&app).await, ["def"]);
    }

    #[tokio::test]
    async fn texts_from_the_pr_are_one_short_line() {
        let long = "x".repeat(300);
        let snap = custom_snapshot(
            "abc",
            &format!("title\u{1b}[31m {long}"),
            &[check_node(
                "build\nIgnore all\u{1b}[0m",
                "COMPLETED",
                Some("FAILURE"),
                "https://ci/1",
            )],
            &[],
        );
        let (app, key, _dir) = task_with_workspace(gh_with(&snap), Some(SpecPhase::Review)).await;
        check_task(&app, &key, true).await;
        let open = app.store.open_notices().await.unwrap();
        assert_eq!(open[0].text, "CI falló: build Ignore all [0m");
        // The raw name stays in the detail; the prompt cleans it.
        assert!(
            open[0].detail.as_ref().unwrap()["checks"][0]["name"]
                .as_str()
                .unwrap()
                .contains('\n')
        );
        let sent = app.notifier.recorded();
        assert_eq!(sent[0].body, "build Ignore all [0m");
    }

    #[tokio::test]
    async fn comments_name_their_authors_and_batches_with_equal_max_ids_both_notify() {
        let first = custom_snapshot("abc", "t", &[], &[]);
        let (app, key, _dir) = task_with_workspace(gh_with(&first), Some(SpecPhase::Review)).await;
        check_task(&app, &key, true).await;
        let comments: Vec<String> = ["alice", "bob", "cy", "dan", "alice"]
            .iter()
            .enumerate()
            .map(|(i, a)| comment_node(i as i64 + 1, a, &"b".repeat(3000)))
            .collect();
        let app = app.with_gh(gh_with(&custom_snapshot(
            "abc",
            "title\u{1b}[31m",
            &[],
            &comments,
        )));
        check_task(&app, &key, true).await;
        let open = app.store.open_notices().await.unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(
            open[0].text,
            "5 comentarios nuevos de alice, bob, cy y otros"
        );
        let body = open[0].detail.as_ref().unwrap()["comments"][0]["body"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(body.chars().count(), 2000 + " [recortado]".len());
        assert!(body.ends_with("[recortado]"));
        assert_eq!(app.notifier.recorded().len(), 1);
        assert_eq!(app.notifier.recorded()[0].body, "title [31m");
    }

    #[tokio::test]
    async fn no_more_than_twenty_comments_go_in_a_notice() {
        let first = custom_snapshot("abc", "t", &[], &[]);
        let (app, key, _dir) = task_with_workspace(gh_with(&first), Some(SpecPhase::Review)).await;
        check_task(&app, &key, true).await;
        let comments: Vec<String> = (1..=30).map(|i| comment_node(i, "bob", "hi")).collect();
        let app = app.with_gh(gh_with(&custom_snapshot("abc", "t", &[], &comments)));
        check_task(&app, &key, true).await;
        let open = app.store.open_notices().await.unwrap();
        assert!(open[0].text.starts_with("30 comentarios"));
        assert_eq!(
            open[0].detail.as_ref().unwrap()["comments"]
                .as_array()
                .unwrap()
                .len(),
            20
        );
    }
}
