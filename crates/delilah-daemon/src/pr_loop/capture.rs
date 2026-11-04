//! The PRs the agent opens itself (FR-003, FR-004): the `PostToolUse` hook of a Bash
//! command that runs `gh pr create` carries its output, and the PR URLs in it are linked
//! to the task. Only that output counts: any other command or tool is ignored.

use delilah_core::TaskKey;

use super::access::failed;
use super::{link_known, repo_of};
use crate::gh::{PrRef, parse_pr_url};
use crate::state::AppState;

/// More URLs than a real `gh pr create` prints are not looked at.
const MAX_URLS: usize = 4;

/// Whether `command` runs `gh pr create`: some simple command of it (split at `;`, `&`,
/// `|` and newlines) starts with it, after any `VAR=value` prefixes. Merely mentioning it
/// (`echo gh pr create`) does not count.
fn runs_pr_create(command: &str) -> bool {
    command.split([';', '&', '|', '\n']).any(|part| {
        let mut words = part.split_whitespace().skip_while(|w| {
            w.split_once('=').is_some_and(|(name, _)| {
                !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            })
        });
        words.next() == Some("gh") && words.next() == Some("pr") && words.next() == Some("create")
    })
}

/// Whether `hook` is the `PostToolUse` of a Bash `gh pr create`.
pub(crate) fn is_pr_create(hook: &serde_json::Value) -> bool {
    hook["hook_event_name"] == "PostToolUse"
        && hook["tool_name"] == "Bash"
        && hook["tool_input"]["command"]
            .as_str()
            .is_some_and(runs_pr_create)
}

/// The PRs named in the output of the command, once each, in order of appearance.
fn urls_in(hook: &serde_json::Value) -> Vec<PrRef> {
    let mut found: Vec<PrRef> = vec![];
    for stream in ["stdout", "stderr"] {
        let Some(text) = hook["tool_response"][stream].as_str() else {
            continue;
        };
        for word in text.split_whitespace() {
            let word = word.trim_matches(|c: char| {
                matches!(
                    c,
                    '"' | '\'' | '`' | '<' | '>' | '(' | ')' | '.' | ',' | ':'
                )
            });
            if !word.starts_with("https://") {
                continue;
            }
            // `parse_pr_url` ignores a trailing path; the whole word must be the PR.
            let Some(pr) = parse_pr_url(word) else {
                continue;
            };
            if word.trim_end_matches('/') != pr.url() {
                continue;
            }
            if !found.contains(&pr) && found.len() < MAX_URLS {
                found.push(pr);
            }
        }
    }
    found
}

/// Links the PRs that a `gh pr create` of the task printed. Never fails the hook: what
/// goes wrong is reported like any other `gh` problem.
pub(crate) async fn on_bash_output(app: &AppState, key: &TaskKey, hook: &serde_json::Value) {
    if !is_pr_create(hook) {
        return;
    }
    let project = match app.store.get_task(key).await {
        Ok(Some(task)) => task.project.unwrap_or_default(),
        _ => return,
    };
    for pr in urls_in(hook) {
        let url = pr.url();
        if app.store.is_unlinked(key, &url).await.unwrap_or(false) {
            continue;
        }
        let existing = app.store.prs_of(key).await.unwrap_or_default();
        if existing.iter().any(|r| r.url == url) {
            continue;
        }
        let backoff_key = format!("{}/{}", pr.host, pr.owner_name);
        let (_, snapshot) = match app.gh.pr_view_url(&url).await {
            Ok(read) => read,
            Err(e) => {
                failed(app, &project, &backoff_key, &e, true).await;
                continue;
            }
        };
        let repo = match repo_of(app, key, &pr).await {
            Ok(repo) => repo,
            Err(e) => {
                tracing::warn!(%key, "could not place the agent's PR: {e:?}");
                continue;
            }
        };
        if let Err(e) = link_known(app, key, repo, pr, &snapshot, "agent").await {
            tracing::warn!(%key, "could not link the agent's PR: {e:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use delilah_core::SpecPhase;
    use serde_json::{Value, json};

    use super::*;
    use crate::gh::GhError;
    use crate::gh::fake::{FakeGh, PR_URL};
    use crate::pr_testkit::task_with_workspace;

    const CREATED: &str =
        include_str!("../../tests/fixtures/hooks/post_tool_use_gh_pr_create.json");
    const EXISTS: &str =
        include_str!("../../tests/fixtures/hooks/post_tool_use_gh_pr_create_exists.json");

    fn bash(command: &str, stdout: &str, stderr: &str) -> Value {
        json!({
            "hook_event_name": "PostToolUse", "tool_name": "Bash",
            "tool_input": { "command": command },
            "tool_response": { "stdout": stdout, "stderr": stderr },
        })
    }

    async fn setup() -> (AppState, TaskKey, Arc<FakeGh>, impl Sized) {
        let fake = Arc::new(FakeGh::happy());
        let (app, key, dir) = task_with_workspace(fake.clone(), Some(SpecPhase::Implement)).await;
        (app, key, fake, dir)
    }

    #[tokio::test]
    async fn a_url_on_stdout_is_linked_as_the_agents_in_a_foreign_repo() {
        let (app, key, _fake, _d) = setup().await;
        let hook: Value = serde_json::from_str(CREATED).unwrap();
        on_bash_output(&app, &key, &hook).await;
        let rows = app.store.prs_of(&key).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].linked_by, "agent");
        assert_eq!(rows[0].repo, "github.com/o/svc-a");
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        assert_eq!(task.phase.as_deref(), Some("review"));
    }

    #[tokio::test]
    async fn a_url_on_stderr_is_linked_and_not_duplicated() {
        let (app, key, _fake, _d) = setup().await;
        let hook: Value = serde_json::from_str(EXISTS).unwrap();
        on_bash_output(&app, &key, &hook).await;
        on_bash_output(&app, &key, &hook).await;
        let rows = app.store.prs_of(&key).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].url, "https://github.com/o/svc-a/pull/12");
    }

    #[tokio::test]
    async fn a_pr_of_the_workspace_repo_gets_its_name_and_a_repeated_url_counts_once() {
        let (app, key, _fake, _d) = setup().await;
        let out = format!("{PR_URL}\n{PR_URL}\n");
        on_bash_output(&app, &key, &bash("gh pr create --fill", &out, "")).await;
        let rows = app.store.prs_of(&key).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].repo, ".");
    }

    #[tokio::test]
    async fn a_url_already_linked_or_unlinked_is_left_alone() {
        let (app, key, fake, _d) = setup().await;
        let hook = bash("gh pr create --fill", PR_URL, "");
        on_bash_output(&app, &key, &hook).await;
        let calls = fake.calls().len();
        on_bash_output(&app, &key, &hook).await;
        assert_eq!(fake.calls().len(), calls, "a linked URL is not read again");
        let row = app.store.prs_of(&key).await.unwrap().remove(0);
        app.store.unlink_pr(row.id).await.unwrap();
        on_bash_output(&app, &key, &hook).await;
        assert!(app.store.prs_of(&key).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn the_output_of_other_commands_and_tools_is_ignored() {
        let (app, key, fake, _d) = setup().await;
        for command in [
            "gh pr view 7",
            "gh pr list",
            &format!("echo {PR_URL}"),
            &format!("echo gh pr create {PR_URL}"),
            "git push && gh pr view",
        ] {
            on_bash_output(&app, &key, &bash(command, PR_URL, PR_URL)).await;
        }
        let mut other_tool = bash("gh pr create", PR_URL, "");
        other_tool["tool_name"] = json!("Read");
        on_bash_output(&app, &key, &other_tool).await;
        let mut pre = bash("gh pr create", PR_URL, "");
        pre["hook_event_name"] = json!("PreToolUse");
        on_bash_output(&app, &key, &pre).await;
        assert!(app.store.prs_of(&key).await.unwrap().is_empty());
        assert!(fake.calls().is_empty());
    }

    #[tokio::test]
    async fn only_whole_pr_urls_are_taken() {
        let (app, key, _fake, _d) = setup().await;
        let out = "http://github.com/o/r/pull/7 https://evil@github.com/o/r/pull/8 \
                   https://github.com/o/r/pull/9/files https://github.com/o/r/issues/3";
        on_bash_output(&app, &key, &bash("gh pr create", out, "")).await;
        assert!(app.store.prs_of(&key).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_url_gh_cannot_read_is_reported_and_not_linked() {
        let fake = Arc::new(FakeGh::new().on(
            &["api", "graphql"],
            Err(GhError::Failed {
                code: None,
                stderr: "To get started with GitHub CLI, please run: gh auth login".into(),
            }),
        ));
        let (app, key, _d) = task_with_workspace(fake, Some(SpecPhase::Implement)).await;
        on_bash_output(&app, &key, &bash("gh pr create", PR_URL, "")).await;
        assert!(app.store.prs_of(&key).await.unwrap().is_empty());
        assert!(
            super::super::access::problem_of(&app, "proj")
                .await
                .is_some()
        );
    }

    #[test]
    fn what_counts_as_running_gh_pr_create() {
        assert!(runs_pr_create("gh pr create --fill"));
        assert!(runs_pr_create("cd x && GH_TOKEN=a gh pr create"));
        assert!(runs_pr_create("git push; gh  pr  create -t 'a'"));
        assert!(!runs_pr_create("echo gh pr create"));
        assert!(!runs_pr_create("gh pr view"));
    }
}
