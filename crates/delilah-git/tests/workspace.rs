use std::path::{Path, PathBuf};
use std::process::Command;

use delilah_core::ProjectKind;
use delilah_git::{
    ChangeKind, GitError, GitOptions, Workspace, WorkspaceSpec, archive_workspace, branch_tip,
    changes, check_merged, check_repos, check_workspace, commit_subjects, create_workspace,
    current_branch, delete_branch, fetch, file_diff, leave_checkout, list_submodules, needs_push,
    push_branch, remove_workspace, repo_root, spec_dirs, use_checkout,
};
use tempfile::TempDir;

const BRANCH: &str = "feature/PROJ-1234-card-retries";

struct Fixture {
    _dir: TempDir,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        let script =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/make-fixture-parent.sh");
        let out = Command::new("bash")
            .arg(script)
            .arg(root.join("env"))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "fixture failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        Self { _dir: dir, root }
    }

    fn parent(&self) -> PathBuf {
        self.root.join("env/parent")
    }

    fn spec(&self, submodules: &[&str]) -> WorkspaceSpec {
        WorkspaceSpec {
            path: self.root.join("workspaces/PROJ-1234"),
            branch: BRANCH.into(),
            kind: ProjectKind::Parent,
            submodules: submodules.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// A plain repo with an `origin`, like a personal project: one of the fixture's
    /// modules cloned on its own.
    fn repo(&self) -> PathBuf {
        let repo = self.root.join("repo");
        if !repo.exists() {
            let remote = self.root.join("env/remotes/svc-orders.git");
            git(
                &self.root,
                &["clone", "-q", &remote.to_string_lossy(), "repo"],
            );
        }
        repo
    }

    fn repo_spec(&self) -> WorkspaceSpec {
        WorkspaceSpec {
            path: self.root.join("workspaces/SIGNAL-12"),
            branch: "feature/SIGNAL-12".into(),
            kind: ProjectKind::Repo,
            submodules: vec![],
        }
    }
}

fn opts() -> GitOptions {
    GitOptions {
        allow_file_protocol: true,
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@delilah.invalid",
        ])
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
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

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
    )
    .unwrap();

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

    let err = create_workspace(&fx.parent(), &spec, &opts()).unwrap_err();
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
    let err = create_workspace(&fx.parent(), &spec, &opts()).unwrap_err();
    assert!(matches!(err, GitError::InvalidBranch(_)), "{err}");

    let spec = fx.spec(&["svc-payments"]);
    std::fs::create_dir_all(&spec.path).unwrap();
    let err = create_workspace(&fx.parent(), &spec, &opts()).unwrap_err();
    assert!(matches!(err, GitError::WorkspaceExists(_)), "{err}");
}

#[test]
fn archives_a_workspace_with_no_new_work() {
    let fx = Fixture::new();
    let ws = create_workspace(&fx.parent(), &fx.spec(&["svc-payments"]), &opts()).unwrap();

    archive_workspace(&fx.parent(), &ws, false, &opts()).unwrap();
    assert!(!ws.path.exists());
    assert_eq!(git(&fx.parent(), &["worktree", "list"]).lines().count(), 1);
}

#[test]
fn refuses_to_archive_while_work_would_be_lost() {
    let fx = Fixture::new();
    let ws = create_workspace(&fx.parent(), &fx.spec(&["svc-payments"]), &opts()).unwrap();
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
    let ws = create_workspace(&repo, &fx.repo_spec(), &opts()).unwrap();
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
    let ws = create_workspace(&repo, &fx.repo_spec(), &opts()).unwrap();

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

    let err = create_workspace(&fx.repo(), &spec, &opts()).unwrap_err();
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

    let ws = create_workspace(&repo, &fx.repo_spec(), &opts()).unwrap();
    archive_workspace(&repo, &ws, false, &opts()).unwrap();
    assert!(!ws.path.exists());
    // The branch stays in the repo: it is what the PR is opened from.
    assert!(git(&repo, &["branch", "--list", "feature/SIGNAL-12"]).contains("SIGNAL-12"));
}

#[test]
fn repo_archive_refuses_while_the_tasks_commits_are_not_pushed() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let ws = create_workspace(&repo, &fx.repo_spec(), &opts()).unwrap();
    commit_change(&ws, ".");

    let err = archive_workspace(&repo, &ws, false, &opts()).unwrap_err();
    assert!(matches!(err, GitError::Unpushed { count: 1, .. }), "{err}");

    git(&ws.path, &["push", "-q", "origin", "feature/SIGNAL-12"]);
    archive_workspace(&repo, &ws, false, &opts()).unwrap();
}

#[test]
fn lists_what_the_branch_changed_committed_or_not() {
    let fx = Fixture::new();
    let ws = create_workspace(&fx.parent(), &fx.spec(&["svc-payments"]), &opts()).unwrap();
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
        branch: "feature/PROJ-9".into(),
        kind: ProjectKind::Parent,
        submodules: vec!["svc-payments".into()],
    };
    let pay = parent.join("svc-payments");
    let before = git(&pay, &["rev-parse", "HEAD"]);

    let (ws, previous) = use_checkout(&parent, &spec, &opts()).unwrap();
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
    let ws = create_workspace(&fx.parent(), &fx.spec(&["svc-payments"]), &opts()).unwrap();
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
        branch: "feature/X-1".into(),
        kind: ProjectKind::Repo,
        submodules: vec![],
    };
    assert!(matches!(
        use_checkout(&repo, &spec, &opts()),
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
    let ws = create_workspace(&fx.parent(), &fx.spec(&["svc-payments"]), &opts()).unwrap();
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
    fetch(&repo, &GitOptions::default()).unwrap();
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
    let ws = create_workspace(&f.parent(), &f.spec(&["svc-payments", "svc-orders"]), &o).unwrap();
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
fn has_branch_is_false_for_an_empty_name() {
    let f = Fixture::new();
    let mut spec = f.repo_spec();
    let ws = Workspace {
        path: spec.path.clone(),
        branch: String::new(),
        kind: spec.kind,
        submodules: vec![],
    };
    assert!(!ws.has_branch());
    spec.branch = "feature/x".into();
    let ws = Workspace {
        branch: spec.branch,
        ..ws
    };
    assert!(ws.has_branch());
}

#[test]
fn spec_dirs_lists_sorted_folders_with_a_spec() {
    let dir = TempDir::new().unwrap();
    assert!(spec_dirs(dir.path()).is_empty());
    let specs = dir.path().join("specs");
    for (name, has_spec) in [("002-b", true), ("001-a", true), ("003-empty", false)] {
        std::fs::create_dir_all(specs.join(name)).unwrap();
        if has_spec {
            std::fs::write(specs.join(name).join("spec.md"), "# s\n").unwrap();
        }
    }
    std::fs::write(specs.join("stray.md"), "x").unwrap();
    assert_eq!(spec_dirs(dir.path()), vec!["001-a", "002-b"]);
}

#[test]
fn current_branch_is_none_on_a_detached_head() {
    let f = Fixture::new();
    let repo = f.repo();
    let branch = current_branch(&repo, &opts()).unwrap();
    assert!(branch.is_some_and(|b| !b.is_empty()));
    git(&repo, &["checkout", "-q", "--detach"]);
    assert_eq!(current_branch(&repo, &opts()).unwrap(), None);
}
