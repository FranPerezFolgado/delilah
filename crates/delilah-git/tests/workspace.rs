use std::path::{Path, PathBuf};
use std::process::Command;

use delilah_core::ProjectKind;
use delilah_git::{
    GitError, GitOptions, Workspace, WorkspaceSpec, archive_workspace, create_workspace,
    list_submodules, repo_root,
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

    archive_workspace(&fx.parent(), &ws, &opts()).unwrap();
    assert!(!ws.path.exists());
    assert_eq!(git(&fx.parent(), &["worktree", "list"]).lines().count(), 1);
}

#[test]
fn refuses_to_archive_while_work_would_be_lost() {
    let fx = Fixture::new();
    let ws = create_workspace(&fx.parent(), &fx.spec(&["svc-payments"]), &opts()).unwrap();
    let sub = ws.path.join("svc-payments");

    std::fs::write(sub.join("wip.txt"), "draft\n").unwrap();
    let err = archive_workspace(&fx.parent(), &ws, &opts()).unwrap_err();
    assert!(matches!(err, GitError::Dirty { .. }), "{err}");

    std::fs::remove_file(sub.join("wip.txt")).unwrap();
    commit_change(&ws, "svc-payments");
    let err = archive_workspace(&fx.parent(), &ws, &opts()).unwrap_err();
    assert!(matches!(err, GitError::Unpushed { count: 1, .. }), "{err}");
    assert!(ws.path.exists());

    git(&sub, &["push", "-q", "origin", BRANCH]);
    archive_workspace(&fx.parent(), &ws, &opts()).unwrap();
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
    archive_workspace(&repo, &ws, &opts()).unwrap();
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

    let err = archive_workspace(&repo, &ws, &opts()).unwrap_err();
    assert!(matches!(err, GitError::Unpushed { count: 1, .. }), "{err}");

    git(&ws.path, &["push", "-q", "origin", "feature/SIGNAL-12"]);
    archive_workspace(&repo, &ws, &opts()).unwrap();
}
