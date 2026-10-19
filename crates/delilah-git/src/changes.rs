//! What a task's branch changed: the files, and the diff of each, against the commit
//! the branch started from. Uncommitted and untracked work counts too: reviewing a
//! task means reviewing what is in the workspace now.

use std::path::Path;

use serde::Serialize;

use crate::command::{GitError, GitOptions, git};

/// Where the branch could have started from, best first.
const BASE_REFS: &[&str] = &[
    "origin/HEAD",
    "origin/main",
    "origin/master",
    "main",
    "master",
];

/// Diffs larger than this are cut: nobody reviews them line by line in a panel.
const MAX_DIFF_BYTES: usize = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    /// New and not yet added to git.
    Untracked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileChange {
    pub path: String,
    /// The previous path of a renamed file.
    pub old_path: Option<String>,
    pub kind: ChangeKind,
    /// Line counts; `None` for binary files.
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Changes {
    /// The commit the branch is compared against; `None` if no base could be found,
    /// in which case only uncommitted work shows.
    pub base: Option<String>,
    /// Commits on the branch since `base`.
    pub commits: u32,
    pub files: Vec<FileChange>,
}

/// The commit the branch in `dir` started from: where it meets the main line.
pub fn base_commit(dir: &Path, opts: &GitOptions) -> Option<String> {
    BASE_REFS.iter().find_map(|r| {
        let target = format!("{r}^{{commit}}");
        git(dir, &["rev-parse", "--verify", "--quiet", &target], opts).ok()?;
        git(dir, &["merge-base", "HEAD", r], opts).ok()
    })
}

pub fn changes(dir: &Path, opts: &GitOptions) -> Result<Changes, GitError> {
    let base = base_commit(dir, opts);
    let against = base.as_deref().unwrap_or("HEAD");
    let commits = match &base {
        Some(base) => git(
            dir,
            &["rev-list", "--count", &format!("{base}..HEAD")],
            opts,
        )?
        .parse()
        .unwrap_or(0),
        None => 0,
    };

    let mut files = name_status(&git(
        dir,
        &["diff", "-M", "--name-status", "-z", against],
        opts,
    )?);
    let counts = numstat(&git(
        dir,
        &["diff", "-M", "--numstat", "-z", against],
        opts,
    )?);
    for file in &mut files {
        if let Some((add, del)) = counts
            .iter()
            .find(|(p, _)| p == &file.path)
            .map(|(_, c)| *c)
        {
            file.additions = add;
            file.deletions = del;
        }
    }
    let untracked = git(
        dir,
        &["ls-files", "--others", "--exclude-standard", "-z"],
        opts,
    )?;
    for path in untracked.split('\0').filter(|p| !p.is_empty()) {
        let lines = std::fs::read_to_string(dir.join(path))
            .ok()
            .map(|text| text.lines().count() as u32);
        files.push(FileChange {
            path: path.to_string(),
            old_path: None,
            kind: ChangeKind::Untracked,
            additions: lines,
            deletions: lines.map(|_| 0),
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Changes {
        base,
        commits,
        files,
    })
}

/// The unified diff of one changed file, as `git diff` prints it.
pub fn file_diff(dir: &Path, file: &FileChange, opts: &GitOptions) -> Result<String, GitError> {
    let mut diff = if file.kind == ChangeKind::Untracked {
        untracked_diff(dir, &file.path)
    } else {
        let base = base_commit(dir, opts);
        let against = base.as_deref().unwrap_or("HEAD");
        let mut args = vec!["diff", "-M", against, "--"];
        if let Some(old) = &file.old_path {
            args.push(old);
        }
        args.push(&file.path);
        git(dir, &args, opts)?
    };
    if diff.len() > MAX_DIFF_BYTES {
        let mut cut = MAX_DIFF_BYTES;
        while !diff.is_char_boundary(cut) {
            cut -= 1;
        }
        diff.truncate(cut);
        diff.push_str("\n\\ Diff cortado: es demasiado grande para mostrarlo entero\n");
    }
    Ok(diff)
}

/// `git diff` ignores untracked files, so their diff is built by hand: all lines new.
fn untracked_diff(dir: &Path, path: &str) -> String {
    let header = format!("diff --git a/{path} b/{path}\nnew file\n--- /dev/null\n+++ b/{path}\n");
    match std::fs::read_to_string(dir.join(path)) {
        Ok(text) if !text.is_empty() => {
            let lines: Vec<&str> = text.lines().collect();
            let body: String = lines.iter().map(|l| format!("+{l}\n")).collect();
            format!("{header}@@ -0,0 +1,{} @@\n{body}", lines.len())
        }
        Ok(_) => header,
        Err(_) => format!("{header}Binary file\n"),
    }
}

/// Every file of the repo in `dir` that git would show: tracked, plus new files not
/// ignored. Submodules appear as their folder, not their files.
pub fn repo_files(dir: &Path, opts: &GitOptions) -> Result<Vec<String>, GitError> {
    let out = git(
        dir,
        &[
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ],
        opts,
    )?;
    Ok(out
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect())
}

/// `--name-status -z`: `M\0path\0`, or `R100\0old\0new\0` for a rename.
fn name_status(out: &str) -> Vec<FileChange> {
    let mut fields = out.split('\0').filter(|f| !f.is_empty());
    let mut files = Vec::new();
    while let Some(status) = fields.next() {
        let (kind, old_path) = match status.chars().next() {
            Some('A') => (ChangeKind::Added, None),
            Some('D') => (ChangeKind::Deleted, None),
            Some('R') | Some('C') => (ChangeKind::Renamed, fields.next().map(str::to_string)),
            _ => (ChangeKind::Modified, None),
        };
        let Some(path) = fields.next() else { break };
        files.push(FileChange {
            path: path.to_string(),
            old_path,
            kind,
            additions: None,
            deletions: None,
        });
    }
    files
}

type Counts = (Option<u32>, Option<u32>);

/// `--numstat -z`: `add\tdel\tpath\0`, or `add\tdel\t\0old\0new\0` for a rename;
/// `-\t-` for binary files.
fn numstat(out: &str) -> Vec<(String, Counts)> {
    let mut fields = out.split('\0');
    let mut counts = Vec::new();
    while let Some(entry) = fields.next() {
        let mut parts = entry.splitn(3, '\t');
        let (Some(add), Some(del), Some(path)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let path = if path.is_empty() {
            // A rename: the old path, then the new one.
            fields.next();
            fields.next().unwrap_or_default()
        } else {
            path
        };
        counts.push((path.to_string(), (add.parse().ok(), del.parse().ok())));
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_renames_in_both_listings() {
        let files = name_status("M\0src/a.rs\0R092\0old.rs\0new.rs\0A\0b.rs\0");
        assert_eq!(files.len(), 3);
        assert_eq!(files[1].kind, ChangeKind::Renamed);
        assert_eq!(files[1].old_path.as_deref(), Some("old.rs"));
        assert_eq!(files[1].path, "new.rs");

        let counts = numstat("3\t1\tsrc/a.rs\x002\t0\t\0old.rs\0new.rs\0-\t-\timg.png\0");
        assert_eq!(counts[0], ("src/a.rs".into(), (Some(3), Some(1))));
        assert_eq!(counts[1], ("new.rs".into(), (Some(2), Some(0))));
        assert_eq!(counts[2], ("img.png".into(), (None, None)));
    }
}
