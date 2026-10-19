use serde::{Deserialize, Serialize};

/// How a project's repos are laid out, which decides how its workspaces are built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectKind {
    /// A parent repo whose modules are git submodules: a task touches some of them
    /// and opens one branch (and PR) in each.
    Parent,
    /// A plain repo: a task gets a worktree on its own branch, as in Orca.
    Repo,
}

impl ProjectKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Parent => "parent",
            Self::Repo => "repo",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "parent" => Some(Self::Parent),
            "repo" => Some(Self::Repo),
            _ => None,
        }
    }
}

/// A registered codebase that tasks run against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    /// Main checkout of the repo (the parent, for `parent` projects).
    pub path: std::path::PathBuf,
    pub kind: ProjectKind,
    /// Workflow used when a run does not name one, e.g. `feature`.
    pub workflow: String,
}
