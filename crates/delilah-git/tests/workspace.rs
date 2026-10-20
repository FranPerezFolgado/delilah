use std::path::{Path, PathBuf};
use std::process::Command;

use delilah_core::ProjectKind;
use delilah_git::{
    ChangeKind, GitError, GitOptions, Workspace, WorkspaceSpec, archive_workspace, changes,
    create_workspace, file_diff, leave_checkout, list_submodules, repo_root, use_checkout,
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
