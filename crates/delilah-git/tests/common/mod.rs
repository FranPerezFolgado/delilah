#![allow(dead_code)]
//! Shared fixtures for the integration tests; each test binary uses only part of it.

use std::path::{Path, PathBuf};
use std::process::Command;

use delilah_core::ProjectKind;
use delilah_git::{GitOptions, WorkspaceSpec};
use tempfile::TempDir;

pub const BRANCH: &str = "feature/PROJ-1234-card-retries";

pub struct Fixture {
    pub _dir: TempDir,
    pub root: PathBuf,
}

impl Fixture {
    pub fn new() -> Self {
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

    pub fn parent(&self) -> PathBuf {
        self.root.join("env/parent")
    }

    pub fn spec(&self, submodules: &[&str]) -> WorkspaceSpec {
        WorkspaceSpec {
            path: self.root.join("workspaces/PROJ-1234"),
            branch: Some(BRANCH.into()),
            kind: ProjectKind::Parent,
            submodules: submodules.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// A plain repo with an `origin`, like a personal project: one of the fixture's
    /// modules cloned on its own.
    pub fn repo(&self) -> PathBuf {
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

    pub fn repo_spec(&self) -> WorkspaceSpec {
        WorkspaceSpec {
            path: self.root.join("workspaces/SIGNAL-12"),
            branch: Some("feature/SIGNAL-12".into()),
            kind: ProjectKind::Repo,
            submodules: vec![],
        }
    }
}

pub fn opts() -> GitOptions {
    GitOptions {
        allow_file_protocol: true,
    }
}

pub fn git(dir: &Path, args: &[&str]) -> String {
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

/// Commit `file` in a fresh clone of the bare `remote`, push `main`, return the new sha.
pub fn advance_remote(remote: &Path, file: &str, msg: &str) -> String {
    let tmp = TempDir::new().unwrap();
    let clone = tmp.path().join("clone");
    let out = Command::new("git")
        .args(["-c", "protocol.file.allow=always", "clone", "-q"])
        .arg(remote)
        .arg(&clone)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "clone: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::write(clone.join(file), format!("{msg}\n")).unwrap();
    git(&clone, &["add", file]);
    git(&clone, &["commit", "-q", "-m", msg]);
    git(&clone, &["push", "-q", "origin", "main"]);
    git(&clone, &["rev-parse", "HEAD"])
}

pub fn remote_head(remote: &Path) -> String {
    git(remote, &["rev-parse", "main"])
}

/// The upstream of `branch` (e.g. `origin/main`), or `None` when it has none.
pub fn upstream_of(dir: &Path, branch: &str) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--abbrev-ref", &format!("{branch}@{{u}}")])
        .output()
        .unwrap();
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}
