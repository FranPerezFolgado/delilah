use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use delilah_core::ProjectKind;
use delilah_git::{
    ChangeKind, Deadline, FetchOutcome, GitError, GitOptions, RepoBase, Workspace, WorkspaceSpec,
    archive_workspace, branch_taken, branch_tip, changes, check_merged, check_repos,
    check_workspace, commit_subjects, create_workspace, delete_branch, fetch, file_diff,
    free_branch, leave_checkout, list_submodules, needs_push, push_branch, remove_workspace,
    repo_root, use_checkout,
};
use tempfile::TempDir;

mod common;
use common::*;

fn commit_change(ws: &Workspace, sub: &str) {
    let dir = ws.path.join(sub);
    std::fs::write(dir.join("change.txt"), "retry with jitter\n").unwrap();
    git(&dir, &["add", "change.txt"]);
    git(&dir, &["commit", "-m", "Add jitter"]);
}

#[test]
fn lists_the_parent_submodules() {
    let fx = Fixture::new();
    let mut paths: Vec<_> = list_submodules(&fx.parent())
        .unwrap()
        .into_iter()
        .map(|s| s.path)
        .collect();
    paths.sort();
    assert_eq!(
        paths,
        ["common-lib", "svc-billing", "svc-orders", "svc-payments"]
    );
}

#[test]
fn initialises_only_the_requested_submodules_on_the_branch() {
    let fx = Fixture::new();
    let ws = create_workspace(
        &fx.parent(),
        &fx.spec(&["svc-payments", "svc-orders"]),
        &opts(),
        dl(),
    )
    .unwrap()
    .0;

    for sub in ["svc-payments", "svc-orders"] {
        assert_eq!(
            git(&ws.path.join(sub), &["branch", "--show-current"]),
            BRANCH
        );
    }
    for sub in ["svc-billing", "common-lib"] {
        let dir = ws.path.join(sub);
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            0,
            "{sub} should stay empty"
        );
    }
    // The parent's own checkout is untouched.
    assert_eq!(git(&fx.parent(), &["branch", "--show-current"]), "main");
}

#[test]
fn rejects_unknown_submodules_without_leaving_anything_behind() {
    let fx = Fixture::new();
    let spec = fx.spec(&["svc-payments", "svc-nope"]);

    let err = create_workspace(&fx.parent(), &spec, &opts(), dl()).unwrap_err();
    assert!(
        matches!(err, GitError::UnknownSubmodule(ref s) if s == "svc-nope"),
        "{err}"
    );
    assert!(!spec.path.exists());
}

#[test]
fn rejects_invalid_branch_names_and_existing_folders() {
    let fx = Fixture::new();

    let mut spec = fx.spec(&["svc-payments"]);
    spec.branch = "bad..branch".into();
    let err = create_workspace(&fx.parent(), &spec, &opts(), dl()).unwrap_err();
    assert!(matches!(err, GitError::InvalidBranch(_)), "{err}");

    let spec = fx.spec(&["svc-payments"]);
    std::fs::create_dir_all(&spec.path).unwrap();
    let err = create_workspace(&fx.parent(), &spec, &opts(), dl()).unwrap_err();
    assert!(matches!(err, GitError::WorkspaceExists(_)), "{err}");
}

#[test]
fn archives_a_workspace_with_no_new_work() {
    let fx = Fixture::new();
    let ws = create_workspace(&fx.parent(), &fx.spec(&["svc-payments"]), &opts(), dl())
        .unwrap()
        .0;

    archive_workspace(&fx.parent(), &ws, false, &opts()).unwrap();
    assert!(!ws.path.exists());
    assert_eq!(git(&fx.parent(), &["worktree", "list"]).lines().count(), 1);
}

#[test]
fn refuses_to_archive_while_work_would_be_lost() {
    let fx = Fixture::new();
    let ws = create_workspace(&fx.parent(), &fx.spec(&["svc-payments"]), &opts(), dl())
        .unwrap()
        .0;
    let sub = ws.path.join("svc-payments");

    std::fs::write(sub.join("wip.txt"), "draft\n").unwrap();
    let err = archive_workspace(&fx.parent(), &ws, false, &opts()).unwrap_err();
    assert!(matches!(err, GitError::Dirty { .. }), "{err}");

    std::fs::remove_file(sub.join("wip.txt")).unwrap();
    commit_change(&ws, "svc-payments");
    let err = archive_workspace(&fx.parent(), &ws, false, &opts()).unwrap_err();
    assert!(matches!(err, GitError::Unpushed { count: 1, .. }), "{err}");
    assert!(ws.path.exists());

    git(&sub, &["push", "-q", "origin", BRANCH]);
    archive_workspace(&fx.parent(), &ws, false, &opts()).unwrap();
    assert!(!ws.path.exists());
}

#[test]
fn checking_a_workspace_never_removes_it() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let ws = create_workspace(&repo, &fx.repo_spec(), &opts(), dl())
        .unwrap()
        .0;
    check_workspace(&ws, &opts()).unwrap();

    commit_change(&ws, ".");
    let err = check_workspace(&ws, &opts()).unwrap_err();
    assert!(matches!(err, GitError::Unpushed { count: 1, .. }), "{err}");
    assert!(ws.path.exists());

    // Removing it anyway keeps the branch, with its commit.
    remove_workspace(&repo, &ws, &opts()).unwrap();
    assert!(!ws.path.exists());
    assert!(git(&repo, &["branch", "--list", "feature/SIGNAL-12"]).contains("SIGNAL-12"));
}

#[test]
fn repo_workspace_is_a_worktree_on_its_own_branch() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let ws = create_workspace(&repo, &fx.repo_spec(), &opts(), dl())
        .unwrap()
        .0;

    assert_eq!(
        git(&ws.path, &["branch", "--show-current"]),
        "feature/SIGNAL-12"
    );
    assert!(ws.path.join("src/service.sh").exists());
    assert_eq!(git(&repo, &["branch", "--show-current"]), "main");
    assert_eq!(
        repo_root(&ws.path.join("src")).unwrap(),
        std::fs::canonicalize(&ws.path).unwrap()
    );
}

#[test]
fn repo_projects_take_no_submodules() {
    let fx = Fixture::new();
    let mut spec = fx.repo_spec();
    spec.submodules = vec!["svc-payments".into()];

    let err = create_workspace(&fx.repo(), &spec, &opts(), dl()).unwrap_err();
    assert!(matches!(err, GitError::SubmodulesInRepo), "{err}");
}

#[test]
fn repo_archive_ignores_unpushed_work_that_is_not_the_tasks() {
    let fx = Fixture::new();
    let repo = fx.repo();
    // Local `main` is ahead of origin before the task even starts.
    std::fs::write(repo.join("local.txt"), "local only\n").unwrap();
    git(&repo, &["add", "local.txt"]);
    git(&repo, &["commit", "-m", "Local change"]);

    let ws = create_workspace(&repo, &fx.repo_spec(), &opts(), dl())
        .unwrap()
        .0;
    archive_workspace(&repo, &ws, false, &opts()).unwrap();
    assert!(!ws.path.exists());
    // The branch stays in the repo: it is what the PR is opened from.
    assert!(git(&repo, &["branch", "--list", "feature/SIGNAL-12"]).contains("SIGNAL-12"));
}

#[test]
fn repo_archive_refuses_while_the_tasks_commits_are_not_pushed() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let ws = create_workspace(&repo, &fx.repo_spec(), &opts(), dl())
        .unwrap()
        .0;
    commit_change(&ws, ".");

    let err = archive_workspace(&repo, &ws, false, &opts()).unwrap_err();
    assert!(matches!(err, GitError::Unpushed { count: 1, .. }), "{err}");

    git(&ws.path, &["push", "-q", "origin", "feature/SIGNAL-12"]);
    archive_workspace(&repo, &ws, false, &opts()).unwrap();
}

#[test]
fn lists_what_the_branch_changed_committed_or_not() {
    let fx = Fixture::new();
    let ws = create_workspace(&fx.parent(), &fx.spec(&["svc-payments"]), &opts(), dl())
        .unwrap()
        .0;
    let dir = ws.path.join("svc-payments");
    commit_change(&ws, "svc-payments");
    std::fs::write(dir.join("README.md"), "Payments, now with retries\n").unwrap();
    std::fs::write(dir.join("notes.md"), "one\ntwo\n").unwrap();
    git(&dir, &["mv", "tests/test.sh", "tests/retry_test.sh"]);

    let found = changes(&dir, &opts()).unwrap();
    assert!(found.base.is_some());
    assert_eq!(found.commits, 1);
    let kinds: Vec<_> = found
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.kind))
        .collect();
    assert_eq!(
        kinds,
        [
            ("README.md", ChangeKind::Modified),
            ("change.txt", ChangeKind::Added),
            ("notes.md", ChangeKind::Untracked),
            ("tests/retry_test.sh", ChangeKind::Renamed),
        ]
    );
    let readme = &found.files[0];
    assert_eq!(readme.additions, Some(1));
    assert_eq!(found.files[2].additions, Some(2));
    assert_eq!(found.files[3].old_path.as_deref(), Some("tests/test.sh"));

    let diff = file_diff(&dir, readme, &opts()).unwrap();
    assert!(diff.contains("+Payments, now with retries"), "{diff}");
    let new = file_diff(&dir, &found.files[2], &opts()).unwrap();
    assert!(new.contains("@@ -0,0 +1,2 @@\n+one\n+two"), "{new}");
}

#[test]
fn a_task_can_work_in_the_checkout_and_leave_it_as_it_was() {
    let fx = Fixture::new();
    let parent = fx.parent();
    // The fixture's parent has its submodules initialised, like a dev's checkout.
    git(&parent, &["submodule", "update", "--init", "-q"]);
    let spec = WorkspaceSpec {
        path: parent.clone(),
        branch: Some("feature/PROJ-9".into()),
        kind: ProjectKind::Parent,
        submodules: vec!["svc-payments".into()],
    };
    let pay = parent.join("svc-payments");
    let before = git(&pay, &["rev-parse", "HEAD"]);

    let (ws, previous, _) = use_checkout(&parent, &spec, &opts(), dl()).unwrap();
    assert_eq!(ws.path, parent);
    assert_eq!(git(&pay, &["branch", "--show-current"]), "feature/PROJ-9");
    assert_eq!(previous[0].0, "svc-payments");

    // Committed work stays on the task's branch; files left uncommitted stay in the
    // checkout and do not stop it from going back.
    std::fs::write(pay.join("retry.txt"), "jitter\n").unwrap();
    git(&pay, &["add", "retry.txt"]);
    git(&pay, &["commit", "-qm", "retry"]);
    std::fs::write(pay.join("notes.txt"), "left over\n").unwrap();
    assert!(leave_checkout(&parent, &previous, &opts()).is_empty());
    assert_eq!(git(&pay, &["rev-parse", "HEAD"]), before);
    assert!(git(&pay, &["branch", "--list", "feature/PROJ-9"]).contains("feature/PROJ-9"));
    assert!(pay.join("notes.txt").exists());
    // Leaving again (already back) is a no-op.
    assert!(leave_checkout(&parent, &previous, &opts()).is_empty());
}

#[test]
fn a_forced_archive_drops_work_that_was_not_pushed() {
    let fx = Fixture::new();
    let ws = create_workspace(&fx.parent(), &fx.spec(&["svc-payments"]), &opts(), dl())
        .unwrap()
        .0;
    std::fs::write(ws.path.join("svc-payments/wip.txt"), "draft\n").unwrap();
    commit_change(&ws, "svc-payments");
    archive_workspace(&fx.parent(), &ws, true, &opts()).unwrap();
    assert!(!ws.path.exists());
}

#[test]
fn the_checkout_is_not_used_with_uncommitted_changes() {
    let fx = Fixture::new();
    let repo = fx.repo();
    std::fs::write(repo.join("wip.txt"), "mine\n").unwrap();
    let spec = WorkspaceSpec {
        path: repo.clone(),
        branch: Some("feature/X-1".into()),
        kind: ProjectKind::Repo,
        submodules: vec![],
    };
    assert!(matches!(
        use_checkout(&repo, &spec, &opts(), dl()),
        Err(GitError::Dirty { .. })
    ));
}

// --- PR helpers: commit_subjects, needs_push, push_branch ---

fn run(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn pr_repo() -> (TempDir, PathBuf, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let remote = tmp.path().join("remote.git");
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    run(
        tmp.path(),
        &["init", "--bare", "-b", "main", remote.to_str().unwrap()],
    );
    run(&repo, &["init", "-b", "main"]);
    run(&repo, &["config", "user.name", "T"]);
    run(&repo, &["config", "user.email", "t@example.com"]);
    run(&repo, &["commit", "--allow-empty", "-m", "base"]);
    run(
        &repo,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    run(&repo, &["push", "-u", "origin", "main"]);
    (tmp, repo, remote)
}

#[test]
fn pr_helpers_subjects_and_push() {
    let (_tmp, repo, remote) = pr_repo();
    let opts = GitOptions::default();
    run(&repo, &["checkout", "-b", "feature/x"]);
    assert!(
        commit_subjects(&repo, "main", "feature/x", &opts)
            .unwrap()
            .is_empty()
    );
    run(&repo, &["commit", "--allow-empty", "-m", "first"]);
    run(&repo, &["commit", "--allow-empty", "-m", "second"]);
    assert_eq!(
        commit_subjects(&repo, "main", "feature/x", &opts).unwrap(),
        vec!["first", "second"]
    );
    assert!(needs_push(&repo, "feature/x", &opts).unwrap());
    push_branch(&repo, "feature/x", &opts).unwrap();
    assert!(!needs_push(&repo, "feature/x", &opts).unwrap());
    let heads = Command::new("git")
        .arg("-C")
        .arg(&remote)
        .args(["branch", "--list", "feature/x"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&heads.stdout).contains("feature/x"));
    run(&repo, &["commit", "--allow-empty", "-m", "third"]);
    assert!(needs_push(&repo, "feature/x", &opts).unwrap());
}

#[test]
fn pr_helpers_reject_dash_branch() {
    let (_tmp, repo, _remote) = pr_repo();
    let opts = GitOptions::default();
    assert!(matches!(
        push_branch(&repo, "-x", &opts),
        Err(GitError::InvalidBranch(_))
    ));
    assert!(matches!(
        needs_push(&repo, "-x", &opts),
        Err(GitError::InvalidBranch(_))
    ));
    assert!(matches!(
        commit_subjects(&repo, "main", "-x", &opts),
        Err(GitError::InvalidBranch(_))
    ));
}

// --- merge check and branch deletion ---

fn head(dir: &Path) -> String {
    git(dir, &["rev-parse", "HEAD"])
}

/// A plain repo whose `feature/x` is pushed, and a squash commit lands on the
/// remote's main. Returns the workspace and the PR head.
fn merged_repo() -> (TempDir, Workspace, String, PathBuf) {
    let (tmp, repo, remote) = pr_repo();
    run(&repo, &["checkout", "-b", "feature/x"]);
    run(&repo, &["commit", "--allow-empty", "-m", "work"]);
    run(&repo, &["push", "-u", "origin", "feature/x"]);
    let pr_head = head(&repo);
    run(&repo, &["checkout", "main"]);
    run(&repo, &["commit", "--allow-empty", "-m", "squash of work"]);
    run(&repo, &["push", "origin", "main"]);
    run(&repo, &["checkout", "feature/x"]);
    let ws = Workspace {
        path: repo,
        branch: "feature/x".into(),
        kind: ProjectKind::Repo,
        submodules: vec![],
    };
    (tmp, ws, pr_head, remote)
}

fn heads(pairs: &[(&str, &str)]) -> std::collections::HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn check_merged_passes_a_squash_merged_branch() {
    let (_tmp, ws, pr_head, _remote) = merged_repo();
    let o = GitOptions::default();
    check_merged(&ws, &heads(&[(".", &pr_head)]), &o).unwrap();
}

#[test]
fn check_merged_refuses_local_commits_after_the_pr_head() {
    let (_tmp, ws, pr_head, _remote) = merged_repo();
    run(&ws.path, &["commit", "--allow-empty", "-m", "extra"]);
    let err = check_merged(&ws, &heads(&[(".", &pr_head)]), &GitOptions::default()).unwrap_err();
    assert!(matches!(err, GitError::NotMerged { .. }), "{err}");
    assert!(err.to_string().contains("not in the merged PR"));
}

#[test]
fn check_merged_refuses_uncommitted_files() {
    let (_tmp, ws, pr_head, _remote) = merged_repo();
    std::fs::write(ws.path.join("wip.txt"), "x\n").unwrap();
    let err = check_merged(&ws, &heads(&[(".", &pr_head)]), &GitOptions::default()).unwrap_err();
    assert!(matches!(err, GitError::Dirty { .. }), "{err}");
}

#[test]
fn check_merged_fetches_when_the_pr_head_is_unknown_locally() {
    let (tmp, ws, _pr_head, remote) = merged_repo();
    let other = tmp.path().join("other");
    run(
        tmp.path(),
        &["clone", "-q", remote.to_str().unwrap(), "other"],
    );
    run(&other, &["config", "user.name", "T"]);
    run(&other, &["config", "user.email", "t@example.com"]);
    run(&other, &["checkout", "feature/x"]);
    run(&other, &["commit", "--allow-empty", "-m", "late"]);
    run(&other, &["push", "origin", "feature/x"]);
    let new_head = head(&other);
    let o = GitOptions::default();
    check_merged(&ws, &heads(&[(".", &new_head)]), &o).unwrap();

    let bogus = "0123456789012345678901234567890123456789";
    let err = check_merged(&ws, &heads(&[(".", bogus)]), &o).unwrap_err();
    assert!(matches!(err, GitError::Command { .. }), "{err}");
}

#[test]
fn check_merged_ignores_repos_without_a_head_and_missing_workspaces() {
    let (_tmp, mut ws, _pr_head, _remote) = merged_repo();
    run(&ws.path, &["commit", "--allow-empty", "-m", "extra"]);
    let o = GitOptions::default();
    check_merged(&ws, &heads(&[]), &o).unwrap();
    ws.path = ws.path.join("gone");
    check_merged(&ws, &heads(&[(".", "abc")]), &o).unwrap();
}

#[test]
fn check_merged_in_a_parent_refuses_a_dirty_parent_but_not_a_moved_pointer() {
    let fx = Fixture::new();
    let ws = create_workspace(&fx.parent(), &fx.spec(&["svc-payments"]), &opts(), dl())
        .unwrap()
        .0;
    commit_change(&ws, "svc-payments");
    let sub = ws.path.join("svc-payments");
    git(&sub, &["push", "-q", "origin", BRANCH]);
    let hs = heads(&[("svc-payments", &head(&sub))]);

    // The submodule pointer differs from the parent's commit: not a change.
    check_merged(&ws, &hs, &opts()).unwrap();

    std::fs::write(ws.path.join("notes.txt"), "x\n").unwrap();
    let err = check_merged(&ws, &hs, &opts()).unwrap_err();
    assert!(matches!(err, GitError::Dirty { .. }), "{err}");
    assert!(err.to_string().contains("the parent"), "{err}");
}

#[test]
fn fetch_updates_the_remote_refs() {
    let (tmp, repo, remote) = pr_repo();
    let other = tmp.path().join("other");
    run(
        tmp.path(),
        &["clone", "-q", remote.to_str().unwrap(), "other"],
    );
    run(&other, &["config", "user.name", "T"]);
    run(&other, &["config", "user.email", "t@example.com"]);
    run(&other, &["commit", "--allow-empty", "-m", "more"]);
    run(&other, &["push", "origin", "main"]);
    fetch(&repo, &GitOptions::default(), dl()).unwrap();
    assert_eq!(git(&repo, &["rev-parse", "origin/main"]), head(&other));
}

#[test]
fn delete_branch_removes_it_and_tolerates_missing_or_refuses_current() {
    let (_tmp, repo, _remote) = pr_repo();
    let o = GitOptions::default();
    run(&repo, &["branch", "feature/y"]);
    delete_branch(&repo, "feature/y", None, &o).unwrap();
    assert!(
        git(&repo, &["branch", "--list", "feature/y"]).is_empty(),
        "branch still there"
    );
    delete_branch(&repo, "feature/y", None, &o).unwrap();
    assert!(delete_branch(&repo, "main", None, &o).is_err());
    assert!(matches!(
        delete_branch(&repo, "-x", None, &o),
        Err(GitError::InvalidBranch(_))
    ));
}

#[test]
fn delete_branch_with_an_expected_tip_deletes_only_that_tip() {
    let (_tmp, repo, _remote) = pr_repo();
    let o = GitOptions::default();
    run(&repo, &["branch", "feature/y"]);
    let tip = branch_tip(&repo, "feature/y", &o).unwrap().unwrap();
    assert_eq!(tip, git(&repo, &["rev-parse", "feature/y"]));
    assert_eq!(branch_tip(&repo, "feature/none", &o).unwrap(), None);

    // The branch moved on since the tip was read: it stays.
    run(&repo, &["checkout", "-q", "feature/y"]);
    run(&repo, &["commit", "--allow-empty", "-m", "later"]);
    run(&repo, &["checkout", "-q", "main"]);
    assert!(delete_branch(&repo, "feature/y", Some(&tip), &o).is_err());
    assert!(!git(&repo, &["branch", "--list", "feature/y"]).is_empty());

    let tip = branch_tip(&repo, "feature/y", &o).unwrap().unwrap();
    delete_branch(&repo, "feature/y", Some(&tip), &o).unwrap();
    assert!(git(&repo, &["branch", "--list", "feature/y"]).is_empty());
    // Gone is fine; something that is not a commit id never reaches git.
    delete_branch(&repo, "feature/y", Some(&tip), &o).unwrap();
    run(&repo, &["branch", "feature/z"]);
    assert!(delete_branch(&repo, "feature/z", Some("--stdin"), &o).is_err());
}

#[test]
fn check_repos_checks_only_the_labelled_repos() {
    let f = Fixture::new();
    let o = GitOptions {
        allow_file_protocol: true,
    };
    let ws = create_workspace(
        &f.parent(),
        &f.spec(&["svc-payments", "svc-orders"]),
        &o,
        dl(),
    )
    .unwrap()
    .0;
    std::fs::write(ws.path.join("svc-orders/wip.txt"), "x").unwrap();
    let only = |l: &str| vec![l.to_string()];

    check_repos(&ws, &only("svc-payments"), &o).unwrap();
    let err = check_repos(&ws, &only("svc-orders"), &o).unwrap_err();
    assert!(matches!(err, GitError::Dirty { .. }), "{err}");

    git(&ws.path.join("svc-orders"), &["add", "."]);
    git(&ws.path.join("svc-orders"), &["commit", "-q", "-m", "wip"]);
    let err = check_repos(&ws, &only("svc-orders"), &o).unwrap_err();
    assert!(matches!(err, GitError::Unpushed { .. }), "{err}");
}

#[test]
fn home_of_is_the_repo_under_the_root_and_nothing_outside_it() {
    let f = Fixture::new();
    let root = f.parent();
    let canonical = root.canonicalize().unwrap();
    assert_eq!(Workspace::home_of(&root, "."), Some(canonical.clone()));
    assert_eq!(
        Workspace::home_of(&root, "svc-orders"),
        Some(canonical.join("svc-orders"))
    );
    // Not initialised, missing, or leading out of the root.
    std::fs::create_dir_all(root.join("plain")).unwrap();
    assert_eq!(Workspace::home_of(&root, "plain"), None);
    assert_eq!(Workspace::home_of(&root, "nope"), None);
    assert_eq!(Workspace::home_of(&root, ".."), None);
    assert_eq!(
        Workspace::home_of(&root, "../env/remotes/svc-orders.git"),
        None
    );
}

#[test]
fn remote_helpers_advance_and_upstream() {
    let fx = Fixture::new();
    let remote = fx.root.join("env/remotes/svc-orders.git");
    let before = remote_head(&remote);
    let sha = advance_remote(&remote, "extra.txt", "advance");
    assert_ne!(before, sha);
    assert_eq!(remote_head(&remote), sha);

    let repo = fx.repo();
    assert_eq!(upstream_of(&repo, "main").as_deref(), Some("origin/main"));
    git(&repo, &["checkout", "-q", "-b", "local-only"]);
    assert_eq!(upstream_of(&repo, "local-only"), None);
}

fn dl() -> Deadline {
    Deadline::after(Duration::from_secs(30))
}

#[test]
fn repo_workspace_starts_at_the_fresh_remote_head() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let remote = fx.root.join("env/remotes/svc-orders.git");
    let local_main = git(&repo, &["rev-parse", "main"]);
    let new_head = advance_remote(&remote, "fresh.txt", "Fresh");

    let (ws, bases) = create_workspace(&repo, &fx.repo_spec(), &opts(), dl()).unwrap();

    assert_eq!(git(&ws.path, &["rev-parse", "HEAD"]), new_head);
    assert_eq!(upstream_of(&ws.path, "feature/SIGNAL-12"), None);
    assert_eq!(git(&repo, &["rev-parse", "main"]), local_main);
    assert_eq!(git(&repo, &["branch", "--show-current"]), "main");
    assert_eq!(
        bases,
        vec![RepoBase {
            repo: ".".into(),
            refname: "origin/main".into(),
            sha: Some(new_head),
            fetch: FetchOutcome::Fetched,
        }]
    );
}

#[test]
fn repo_workspace_ignores_the_checked_out_branch() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let remote = fx.root.join("env/remotes/svc-orders.git");
    git(&repo, &["checkout", "-q", "-b", "other", "HEAD"]);
    let new_head = advance_remote(&remote, "fresh.txt", "Fresh");

    let (ws, _) = create_workspace(&repo, &fx.repo_spec(), &opts(), dl()).unwrap();

    assert_eq!(git(&ws.path, &["rev-parse", "HEAD"]), new_head);
    assert_eq!(git(&repo, &["branch", "--show-current"]), "other");
}

#[test]
fn repo_workspace_starts_from_the_old_origin_when_the_fetch_fails() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let old = git(&repo, &["rev-parse", "origin/main"]);
    git(&repo, &["remote", "set-url", "origin", "/nonexistent"]);

    let (ws, bases) = create_workspace(&repo, &fx.repo_spec(), &opts(), dl()).unwrap();

    assert_eq!(git(&ws.path, &["rev-parse", "HEAD"]), old);
    assert_eq!(bases[0].refname, "origin/main");
    assert!(
        matches!(bases[0].fetch, FetchOutcome::Failed(_)),
        "{:?}",
        bases[0]
    );
}

fn checkout_spec(branch: &str) -> WorkspaceSpec {
    WorkspaceSpec {
        path: PathBuf::new(),
        branch: branch.into(),
        kind: ProjectKind::Repo,
        submodules: vec![],
    }
}

#[test]
fn checkout_starts_the_new_branch_at_the_fresh_remote_head() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let remote = fx.root.join("env/remotes/svc-orders.git");
    let local_main = git(&repo, &["rev-parse", "main"]);
    let new_head = advance_remote(&remote, "fresh.txt", "Fresh");
    assert_ne!(new_head, local_main);
    let spec = checkout_spec("feature/X-1");

    let (_, previous, bases) = use_checkout(&repo, &spec, &opts(), dl()).unwrap();

    assert_eq!(git(&repo, &["branch", "--show-current"]), "feature/X-1");
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), new_head);
    assert_eq!(upstream_of(&repo, "feature/X-1"), None);
    assert_eq!(git(&repo, &["rev-parse", "main"]), local_main);
    assert_eq!(previous, [(".".to_string(), "main".to_string())]);
    assert_eq!(bases[0].fetch, FetchOutcome::Fetched);
    assert_eq!(bases[0].sha.as_deref(), Some(new_head.as_str()));
    assert!(leave_checkout(&repo, &previous, &opts()).is_empty());
    assert_eq!(git(&repo, &["branch", "--show-current"]), "main");
}

#[test]
fn checkout_uses_an_existing_branch_as_it_is_without_fetching() {
    let fx = Fixture::new();
    let repo = fx.repo();
    git(&repo, &["checkout", "-q", "-b", "feature/X-2"]);
    std::fs::write(repo.join("mine.txt"), "x\n").unwrap();
    git(&repo, &["add", "mine.txt"]);
    git(&repo, &["commit", "-qm", "mine"]);
    let tip = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["checkout", "-q", "main"]);
    // A fetch would hang: one that ran would show in the time taken.
    git(&repo, &["config", "core.sshCommand", "sleep 30 #"]);
    git(
        &repo,
        &["remote", "set-url", "origin", "ssh://host/repo.git"],
    );

    let started = std::time::Instant::now();
    let (_, previous, bases) =
        use_checkout(&repo, &checkout_spec("feature/X-2"), &opts(), dl()).unwrap();
    assert!(started.elapsed() < Duration::from_secs(10));

    assert_eq!(git(&repo, &["branch", "--show-current"]), "feature/X-2");
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), tip);
    assert_eq!(bases[0].fetch, FetchOutcome::Existing);
    assert_eq!(bases[0].sha.as_deref(), Some(tip.as_str()));
    assert_eq!(previous, [(".".to_string(), "main".to_string())]);
}

#[test]
fn a_repo_workspace_refuses_before_any_fetch() {
    let fx = Fixture::new();
    let repo = fx.repo();
    git(&repo, &["config", "core.sshCommand", "sleep 30 #"]);
    git(
        &repo,
        &["remote", "set-url", "origin", "ssh://host/repo.git"],
    );
    let started = std::time::Instant::now();

    let mut spec = fx.repo_spec();
    spec.branch = "bad..name".into();
    let r = create_workspace(&repo, &spec, &opts(), dl());
    assert!(matches!(r, Err(GitError::InvalidBranch(_))), "{r:?}");

    let spec = fx.repo_spec();
    std::fs::create_dir_all(&spec.path).unwrap();
    let r = create_workspace(&repo, &spec, &opts(), dl());
    assert!(matches!(r, Err(GitError::WorkspaceExists(_))), "{r:?}");

    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn a_dirty_checkout_fails_before_any_fetch() {
    let fx = Fixture::new();
    let repo = fx.repo();
    git(&repo, &["config", "core.sshCommand", "sleep 30 #"]);
    git(
        &repo,
        &["remote", "set-url", "origin", "ssh://host/repo.git"],
    );
    std::fs::write(repo.join("wip.txt"), "mine\n").unwrap();
    git(&repo, &["add", "wip.txt"]);

    let started = std::time::Instant::now();
    let r = use_checkout(&repo, &checkout_spec("feature/X-3"), &opts(), dl());

    assert!(matches!(r, Err(GitError::Dirty { .. })));
    // Well under the 30 s deadline a fetch to that host could take.
    assert!(started.elapsed() < Duration::from_secs(10));
}

// --- parent projects: a fresh base per repo ---

fn base_of<'a>(bases: &'a [RepoBase], repo: &str) -> &'a RepoBase {
    bases.iter().find(|b| b.repo == repo).unwrap()
}

#[test]
fn parent_workspace_starts_the_parent_and_submodules_fresh() {
    let fx = Fixture::new();
    let pay_remote = fx.root.join("env/remotes/svc-payments.git");
    let parent_remote = fx.root.join("env/remotes/parent.git");
    let pay_head = advance_remote(&pay_remote, "fresh.txt", "Fresh");
    let parent_head = advance_remote(&parent_remote, "fresh.txt", "Fresh");

    let (ws, bases) =
        create_workspace(&fx.parent(), &fx.spec(&["svc-payments"]), &opts(), dl()).unwrap();

    assert_eq!(git(&ws.path, &["rev-parse", "HEAD"]), parent_head);
    assert_eq!(git(&ws.path, &["branch", "--show-current"]), "");
    let pay = ws.path.join("svc-payments");
    assert_eq!(git(&pay, &["rev-parse", "HEAD"]), pay_head);
    assert_eq!(git(&pay, &["branch", "--show-current"]), BRANCH);
    assert_eq!(upstream_of(&pay, BRANCH), None);
    assert_eq!(
        std::fs::read_dir(ws.path.join("svc-billing"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(bases.len(), 2);
    assert_eq!(base_of(&bases, ".").fetch, FetchOutcome::Fetched);
    assert_eq!(
        base_of(&bases, ".").sha.as_deref(),
        Some(parent_head.as_str())
    );
    assert_eq!(base_of(&bases, "svc-payments").fetch, FetchOutcome::Fetched);
    assert_eq!(
        base_of(&bases, "svc-payments").sha.as_deref(),
        Some(pay_head.as_str())
    );
}

#[test]
fn parent_workspace_starts_each_submodule_from_its_own_default_branch() {
    let fx = Fixture::new();
    let bill_remote = fx.root.join("env/remotes/svc-billing.git");
    git(&bill_remote, &["branch", "-m", "main", "master"]);
    git(&bill_remote, &["symbolic-ref", "HEAD", "refs/heads/master"]);
    let bill_head = git(&bill_remote, &["rev-parse", "master"]);
    let pay_head = advance_remote(
        &fx.root.join("env/remotes/svc-payments.git"),
        "fresh.txt",
        "Fresh",
    );

    let (ws, bases) = create_workspace(
        &fx.parent(),
        &fx.spec(&["svc-payments", "svc-billing"]),
        &opts(),
        dl(),
    )
    .unwrap();

    let bill = base_of(&bases, "svc-billing");
    assert_eq!(bill.refname, "origin/master", "{bill:?}");
    assert_eq!(bill.sha.as_deref(), Some(bill_head.as_str()));
    assert_eq!(base_of(&bases, "svc-payments").refname, "origin/main");
    assert_eq!(
        git(&ws.path.join("svc-billing"), &["rev-parse", "HEAD"]),
        bill_head
    );
    assert_eq!(
        git(&ws.path.join("svc-payments"), &["rev-parse", "HEAD"]),
        pay_head
    );
}

fn parent_checkout_spec(parent: &Path, subs: &[&str]) -> WorkspaceSpec {
    WorkspaceSpec {
        path: parent.to_path_buf(),
        branch: "feature/PROJ-9".into(),
        kind: ProjectKind::Parent,
        submodules: subs.iter().map(|s| s.to_string()).collect(),
    }
}

#[test]
fn parent_checkout_falls_back_per_submodule_when_one_fetch_fails() {
    let fx = Fixture::new();
    let parent = fx.parent();
    git(&parent, &["submodule", "update", "--init", "-q"]);
    let pay_head = advance_remote(
        &fx.root.join("env/remotes/svc-payments.git"),
        "fresh.txt",
        "Fresh",
    );
    let bill = parent.join("svc-billing");
    let old = git(&bill, &["rev-parse", "origin/main"]);
    git(&bill, &["remote", "set-url", "origin", "/nonexistent"]);

    let spec = parent_checkout_spec(&parent, &["svc-payments", "svc-billing"]);
    let (_, _, bases) = use_checkout(&parent, &spec, &opts(), dl()).unwrap();

    let b = base_of(&bases, "svc-billing");
    assert!(matches!(b.fetch, FetchOutcome::Failed(_)), "{b:?}");
    assert_eq!(git(&bill, &["rev-parse", "HEAD"]), old);
    let p = base_of(&bases, "svc-payments");
    assert_eq!(p.fetch, FetchOutcome::Fetched);
    assert_eq!(
        git(&parent.join("svc-payments"), &["rev-parse", "HEAD"]),
        pay_head
    );
}

#[test]
fn parent_checkout_starts_a_submodule_with_no_origin_from_its_own_main() {
    let fx = Fixture::new();
    let parent = fx.parent();
    git(&parent, &["submodule", "update", "--init", "-q"]);
    let bill = parent.join("svc-billing");
    let local = git(&bill, &["rev-parse", "HEAD"]);
    git(&bill, &["remote", "remove", "origin"]);

    let spec = parent_checkout_spec(&parent, &["svc-payments", "svc-billing"]);
    let (_, _, bases) = use_checkout(&parent, &spec, &opts(), dl()).unwrap();

    let b = base_of(&bases, "svc-billing");
    assert_eq!(b.fetch, FetchOutcome::NoRemote, "{b:?}");
    assert_eq!(git(&bill, &["branch", "--show-current"]), "feature/PROJ-9");
    assert_eq!(git(&bill, &["rev-parse", "HEAD"]), local);
    assert_eq!(base_of(&bases, "svc-payments").fetch, FetchOutcome::Fetched);
}

#[test]
fn a_failed_parent_checkout_leaves_no_branch_behind() {
    let fx = Fixture::new();
    let parent = fx.parent();
    git(&parent, &["submodule", "update", "--init", "-q"]);
    let pay = parent.join("svc-payments");
    let pay_was = git(&pay, &["rev-parse", "HEAD"]);
    // svc-billing can't switch: its index is locked.
    let bill_git = git(
        &parent.join("svc-billing"),
        &["rev-parse", "--absolute-git-dir"],
    );
    std::fs::write(Path::new(&bill_git).join("index.lock"), "").unwrap();

    let spec = parent_checkout_spec(&parent, &["svc-payments", "svc-billing"]);
    assert!(use_checkout(&parent, &spec, &opts(), dl()).is_err());

    assert_eq!(git(&pay, &["rev-parse", "HEAD"]), pay_was);
    assert_eq!(git(&pay, &["branch", "--list", "feature/PROJ-9"]), "");
}

#[test]
fn parent_checkout_gives_the_parent_folder_no_branch_and_no_base() {
    let fx = Fixture::new();
    let parent = fx.parent();
    git(&parent, &["submodule", "update", "--init", "-q"]);
    let pay_head = advance_remote(
        &fx.root.join("env/remotes/svc-payments.git"),
        "fresh.txt",
        "Fresh",
    );
    advance_remote(
        &fx.root.join("env/remotes/parent.git"),
        "fresh.txt",
        "Fresh",
    );
    let parent_before = git(&parent, &["rev-parse", "HEAD"]);

    let spec = parent_checkout_spec(&parent, &["svc-payments"]);
    let (_, previous, bases) = use_checkout(&parent, &spec, &opts(), dl()).unwrap();

    let pay = parent.join("svc-payments");
    assert_eq!(git(&pay, &["branch", "--show-current"]), "feature/PROJ-9");
    assert_eq!(git(&pay, &["rev-parse", "HEAD"]), pay_head);
    assert_eq!(git(&parent, &["rev-parse", "HEAD"]), parent_before);
    assert_eq!(git(&parent, &["branch", "--show-current"]), "main");
    assert!(bases.iter().all(|b| b.repo != "."), "{bases:?}");
    assert_eq!(previous.len(), 1);
    // The parent folder was not fetched either.
    assert_eq!(git(&parent, &["rev-parse", "origin/main"]), parent_before);
}

#[test]
fn parent_checkout_judges_an_existing_branch_per_submodule() {
    let fx = Fixture::new();
    let parent = fx.parent();
    git(&parent, &["submodule", "update", "--init", "-q"]);
    let pay = parent.join("svc-payments");
    git(&pay, &["branch", "feature/PROJ-9"]);
    let pay_tip = git(&pay, &["rev-parse", "feature/PROJ-9"]);
    let ord_head = advance_remote(
        &fx.root.join("env/remotes/svc-orders.git"),
        "fresh.txt",
        "Fresh",
    );
    advance_remote(
        &fx.root.join("env/remotes/svc-payments.git"),
        "fresh.txt",
        "Fresh",
    );

    let spec = parent_checkout_spec(&parent, &["svc-payments", "svc-orders"]);
    let (_, _, bases) = use_checkout(&parent, &spec, &opts(), dl()).unwrap();

    let p = base_of(&bases, "svc-payments");
    assert_eq!(p.fetch, FetchOutcome::Existing);
    assert_eq!(git(&pay, &["rev-parse", "HEAD"]), pay_tip);
    let o = base_of(&bases, "svc-orders");
    assert_eq!(o.fetch, FetchOutcome::Fetched);
    assert_eq!(
        git(&parent.join("svc-orders"), &["rev-parse", "HEAD"]),
        ord_head
    );
}
