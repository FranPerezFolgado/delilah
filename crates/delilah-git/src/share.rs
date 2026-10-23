//! Shared entries: files or folders of the checkout copied or linked into a new worktree.

use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use delilah_core::{ShareMode, SharedEntry};

/// What happened to one shared entry; the daemon turns each into a log line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShareNote {
    Copied {
        path: String,
    },
    Linked {
        path: String,
    },
    /// The worktree already has it (git tracks it, or it was placed earlier): left as it is.
    AlreadyThere {
        path: String,
    },
    /// The checkout has no such entry.
    Missing {
        path: String,
    },
    /// Skipped on purpose (unsafe path, symlink escaping the checkout or the worktree).
    Refused {
        path: String,
        reason: String,
    },
    /// The copy or link failed.
    Failed {
        path: String,
        reason: String,
    },
}

impl fmt::Display for ShareNote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Copied { path } => write!(f, "shared {path}: copied"),
            Self::Linked { path } => write!(f, "shared {path}: linked"),
            Self::AlreadyThere { path } => {
                write!(f, "shared {path}: already in the worktree, left as it is")
            }
            Self::Missing { path } => write!(f, "shared {path}: not in the checkout, skipped"),
            Self::Refused { path, reason } => write!(f, "shared {path}: skipped ({reason})"),
            Self::Failed { path, reason } => write!(f, "shared {path}: failed ({reason})"),
        }
    }
}

/// Copies (`cp -cR`) or links (absolute symlink) each entry from `checkout` into `worktree`.
/// Only reads from the checkout. One entry failing never stops the others.
pub fn share_entries(checkout: &Path, worktree: &Path, entries: &[SharedEntry]) -> Vec<ShareNote> {
    let root = fs::canonicalize(checkout).ok();
    entries
        .iter()
        .map(|e| share_one(root.as_deref(), worktree, e))
        .collect()
}

fn share_one(root: Option<&Path>, worktree: &Path, entry: &SharedEntry) -> ShareNote {
    let path = entry.path.clone();
    let refused = |reason: &str| ShareNote::Refused {
        path: entry.path.clone(),
        reason: reason.to_string(),
    };
    let failed = |reason: String| ShareNote::Failed {
        path: entry.path.clone(),
        reason,
    };

    // Defensive re-check of what validation already guarantees.
    let mut rel = PathBuf::new();
    for c in Path::new(&entry.path).components() {
        match c {
            Component::Normal(s) if s == ".git" => return refused("inside .git"),
            Component::Normal(s) => rel.push(s),
            Component::CurDir => {}
            _ => return refused("path must be relative to the project root"),
        }
    }
    if rel.as_os_str().is_empty() {
        return refused("empty path");
    }

    let Some(root) = root else {
        return failed("checkout not readable".into());
    };
    let source = root.join(&rel);
    if fs::symlink_metadata(&source).is_err() {
        return ShareNote::Missing { path };
    }
    let source = match fs::canonicalize(&source) {
        Ok(s) => s,
        Err(_) => return ShareNote::Missing { path }, // dangling symlink
    };
    if !source.starts_with(root) {
        return refused("resolves outside the checkout");
    }

    // Walk the destination under the worktree without following symlinks.
    let dest = worktree.join(&rel);
    let mut cur = worktree.to_path_buf();
    let comps: Vec<_> = rel.components().collect();
    for (i, c) in comps.iter().enumerate() {
        cur.push(c);
        match fs::symlink_metadata(&cur) {
            Ok(m) if i + 1 == comps.len() => {
                let _ = m;
                return ShareNote::AlreadyThere { path };
            }
            Ok(m) if m.file_type().is_symlink() => {
                return refused("a parent in the worktree is a symlink");
            }
            Ok(m) if !m.is_dir() => return refused("a parent in the worktree is not a folder"),
            Ok(_) => {}
            Err(_) => break, // the rest does not exist yet
        }
    }

    if let Some(parent) = dest.parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        return failed(format!("cannot create {}: {e}", parent.display()));
    }

    let is_dir = source.is_dir();
    let link = match entry.mode {
        Some(ShareMode::Link) => true,
        Some(ShareMode::Copy) => false,
        None => is_dir,
    };
    if link {
        match std::os::unix::fs::symlink(&source, &dest) {
            Ok(()) => ShareNote::Linked { path },
            Err(e) => failed(e.to_string()),
        }
    } else {
        match Command::new("cp")
            .arg("-cR")
            .arg(&source)
            .arg(&dest)
            .output()
        {
            Ok(o) if o.status.success() => ShareNote::Copied { path },
            Ok(o) => failed(String::from_utf8_lossy(&o.stderr).trim().to_string()),
            Err(e) => failed(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use tempfile::TempDir;

    struct Fx {
        _dir: TempDir,
        checkout: PathBuf,
        worktree: PathBuf,
    }

    fn fx() -> Fx {
        let dir = TempDir::new().unwrap();
        let checkout = dir.path().join("checkout");
        let worktree = dir.path().join("worktree");
        fs::create_dir_all(&checkout).unwrap();
        fs::create_dir_all(&worktree).unwrap();
        Fx {
            _dir: dir,
            checkout,
            worktree,
        }
    }

    fn entry(path: &str, mode: Option<ShareMode>) -> SharedEntry {
        SharedEntry {
            path: path.into(),
            mode,
        }
    }

    #[test]
    fn copies_a_file_creating_parents() {
        let f = fx();
        fs::create_dir_all(f.checkout.join("config")).unwrap();
        fs::write(f.checkout.join("config/local.env"), "A=1").unwrap();
        let n = share_entries(
            &f.checkout,
            &f.worktree,
            &[entry("config/local.env", Some(ShareMode::Copy))],
        );
        assert_eq!(
            n,
            vec![ShareNote::Copied {
                path: "config/local.env".into()
            }]
        );
        let d = f.worktree.join("config/local.env");
        assert!(!fs::symlink_metadata(&d).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(d).unwrap(), "A=1");
    }

    #[test]
    fn links_a_folder_with_an_absolute_symlink() {
        let f = fx();
        fs::create_dir_all(f.checkout.join("node_modules")).unwrap();
        let n = share_entries(
            &f.checkout,
            &f.worktree,
            &[entry("node_modules", Some(ShareMode::Link))],
        );
        assert_eq!(
            n,
            vec![ShareNote::Linked {
                path: "node_modules".into()
            }]
        );
        let t = fs::read_link(f.worktree.join("node_modules")).unwrap();
        assert!(t.is_absolute());
        assert_eq!(
            t,
            fs::canonicalize(f.checkout.join("node_modules")).unwrap()
        );
    }

    #[test]
    fn auto_mode_links_folders_and_copies_files() {
        let f = fx();
        fs::create_dir_all(f.checkout.join("dir")).unwrap();
        fs::write(f.checkout.join("file"), "x").unwrap();
        let n = share_entries(
            &f.checkout,
            &f.worktree,
            &[entry("dir", None), entry("file", None)],
        );
        assert_eq!(
            n,
            vec![
                ShareNote::Linked { path: "dir".into() },
                ShareNote::Copied {
                    path: "file".into()
                }
            ]
        );
    }

    #[test]
    fn existing_entry_is_left_alone() {
        let f = fx();
        fs::write(f.checkout.join("a"), "new").unwrap();
        fs::write(f.worktree.join("a"), "tracked").unwrap();
        let n = share_entries(&f.checkout, &f.worktree, &[entry("a", None)]);
        assert_eq!(n, vec![ShareNote::AlreadyThere { path: "a".into() }]);
        assert_eq!(fs::read_to_string(f.worktree.join("a")).unwrap(), "tracked");
    }

    #[test]
    fn missing_source_is_reported_and_others_continue() {
        let f = fx();
        fs::write(f.checkout.join("b"), "x").unwrap();
        let n = share_entries(
            &f.checkout,
            &f.worktree,
            &[entry("nope", None), entry("b", None)],
        );
        assert_eq!(
            n,
            vec![
                ShareNote::Missing {
                    path: "nope".into()
                },
                ShareNote::Copied { path: "b".into() }
            ]
        );
    }

    #[test]
    fn source_symlink_escaping_the_checkout_is_refused() {
        let f = fx();
        let outside = f._dir.path().join("secret");
        fs::write(&outside, "s").unwrap();
        symlink(&outside, f.checkout.join("leak")).unwrap();
        let n = share_entries(&f.checkout, &f.worktree, &[entry("leak", None)]);
        assert!(matches!(n[0], ShareNote::Refused { .. }), "{n:?}");
        assert!(fs::symlink_metadata(f.worktree.join("leak")).is_err());
    }

    #[test]
    fn symlinked_parent_in_the_worktree_is_not_written_through() {
        let f = fx();
        fs::create_dir_all(f.checkout.join("cfg")).unwrap();
        fs::write(f.checkout.join("cfg/x"), "x").unwrap();
        let elsewhere = f._dir.path().join("elsewhere");
        fs::create_dir_all(&elsewhere).unwrap();
        symlink(&elsewhere, f.worktree.join("cfg")).unwrap();
        let n = share_entries(&f.checkout, &f.worktree, &[entry("cfg/x", None)]);
        assert!(matches!(n[0], ShareNote::Refused { .. }), "{n:?}");
        assert!(fs::read_dir(&elsewhere).unwrap().next().is_none());
    }

    #[test]
    fn git_and_unsafe_paths_are_refused() {
        let f = fx();
        fs::create_dir_all(f.checkout.join(".git/hooks")).unwrap();
        let n = share_entries(
            &f.checkout,
            &f.worktree,
            &[
                entry(".git/hooks", None),
                entry("../x", None),
                entry("/etc/passwd", None),
            ],
        );
        assert!(
            n.iter().all(|x| matches!(x, ShareNote::Refused { .. })),
            "{n:?}"
        );
        assert!(fs::symlink_metadata(f.worktree.join(".git")).is_err());
    }

    #[test]
    fn display_is_a_short_line() {
        let s = ShareNote::Missing { path: "a".into() }.to_string();
        assert!(s.contains('a') && !s.contains('\n'));
    }
}
