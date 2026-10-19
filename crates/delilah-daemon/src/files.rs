//! The task's workspace as files, for the app's editor: list a folder, read a file,
//! save it. Every path is relative to the workspace and must stay inside it, also
//! through symlinks. Saving checks the file's version first, so an edit made in the
//! editor never silently overwrites what the agent wrote meanwhile.

use std::path::{Component, Path as FsPath, PathBuf};
use std::time::UNIX_EPOCH;

use axum::Json;
use axum::extract::{Path, Query, State};
use delilah_core::TaskKey;
use serde::{Deserialize, Serialize};

use crate::api::ApiError;
use crate::state::AppState;

/// Enough for any repo worth opening here; past this the list is cut.
const MAX_LISTED: usize = 100_000;

/// Bigger files are shown as not editable: an editor in a panel is not the tool.
pub const MAX_EDIT_BYTES: u64 = 1 << 20;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub dir: bool,
}

#[derive(Debug, Serialize)]
pub struct FileContent {
    pub path: String,
    /// `None` when the file is binary or too big to edit.
    pub text: Option<String>,
    /// Changes whenever the file does; sent back when saving.
    pub version: String,
    pub size: u64,
}

#[derive(Debug, Deserialize)]
pub struct PathQuery {
    #[serde(default)]
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct Save {
    pub text: String,
    /// The version the edit started from. A different one on disk means someone
    /// else (the agent) changed the file: the save is refused.
    pub version: String,
}

/// `root` joined with `rel`, refused if it leaves `root`: no absolute paths, no
/// `..`, and no symlink that points outside. `rel` may be empty (the root itself).
pub fn resolve(root: &FsPath, rel: &str) -> Result<PathBuf, ApiError> {
    let bad = || ApiError::bad_request(format!("`{rel}` is not a path inside the workspace"));
    let rel_path = FsPath::new(rel);
    if rel_path
        .components()
        .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(bad());
    }
    let root = root
        .canonicalize()
        .map_err(|e| ApiError::not_found(format!("workspace not found: {e}")))?;
    let full = root.join(rel_path);
    let real = full
        .canonicalize()
        .map_err(|_| ApiError::not_found(format!("`{rel}` does not exist")))?;
    if !real.starts_with(&root) {
        return Err(bad());
    }
    Ok(real)
}

pub fn list_dir(dir: &FsPath) -> std::io::Result<Vec<Entry>> {
    let mut entries: Vec<Entry> = std::fs::read_dir(dir)?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            // Git's own data is noise here; a submodule's `.git` is a file, same thing.
            if name == ".git" || name == ".DS_Store" {
                return None;
            }
            // Follows symlinks, so a linked folder opens like one.
            let dir = e.path().is_dir();
            Some(Entry { name, dir })
        })
        .collect();
    entries.sort_by(|a, b| {
        b.dir
            .cmp(&a.dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

pub fn version(path: &FsPath) -> std::io::Result<String> {
    let meta = std::fs::metadata(path)?;
    let modified = meta
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    Ok(format!("{modified}-{}", meta.len()))
}

pub fn read(path: &FsPath, rel: &str) -> std::io::Result<FileContent> {
    let size = std::fs::metadata(path)?.len();
    let text = if size > MAX_EDIT_BYTES {
        None
    } else {
        let bytes = std::fs::read(path)?;
        // A NUL byte early on is the usual sign of a binary file.
        let binary = bytes.iter().take(8000).any(|&b| b == 0);
        if binary {
            None
        } else {
            String::from_utf8(bytes).ok()
        }
    };
    Ok(FileContent {
        path: rel.to_string(),
        text,
        version: version(path)?,
        size,
    })
}

async fn root(app: &AppState, key: &TaskKey) -> Result<PathBuf, ApiError> {
    let ws = app.store.workspace(key).await?.map(|(_, ws)| ws);
    ws.filter(|ws| ws.path.is_dir())
        .map(|ws| ws.path)
        .ok_or_else(|| ApiError::not_found(format!("{key} has no workspace")))
}

fn io_error(e: std::io::Error) -> ApiError {
    ApiError::internal(e.to_string())
}

/// `GET /api/v1/tasks/{key}/tree?path=…`: one folder of the workspace.
pub async fn tree(
    State(app): State<AppState>,
    Path(key): Path<String>,
    Query(q): Query<PathQuery>,
) -> Result<Json<Vec<Entry>>, ApiError> {
    let dir = resolve(&root(&app, &TaskKey::new(key)).await?, &q.path)?;
    if !dir.is_dir() {
        return Err(ApiError::bad_request(format!(
            "`{}` is not a folder",
            q.path
        )));
    }
    Ok(Json(list_dir(&dir).map_err(io_error)?))
}

/// `GET /api/v1/tasks/{key}/files`: every file of the workspace, as git sees it (no
/// ignored files), with each initialised submodule's files under its folder. For
/// finding a file by name.
pub async fn all(
    State(app): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Vec<String>>, ApiError> {
    let key = TaskKey::new(key);
    let ws = app
        .store
        .workspace(&key)
        .await?
        .map(|(_, ws)| ws)
        .filter(|ws| ws.path.is_dir())
        .ok_or_else(|| ApiError::not_found(format!("{key} has no workspace")))?;
    let opts = delilah_git::GitOptions {
        allow_file_protocol: app.config.allow_file_protocol,
    };
    let files = tokio::task::spawn_blocking(move || {
        let mut files = delilah_git::repo_files(&ws.path, &opts).unwrap_or_default();
        // A parent lists its submodules as folders: put their files in their place.
        let subs: Vec<String> = ws
            .repos()
            .into_iter()
            .map(|(sub, _)| sub)
            .filter(|s| s != ".")
            .collect();
        // Submodules left out of this workspace are empty folders: not files either.
        files.retain(|f| !subs.contains(f) && !ws.path.join(f).is_dir());
        for sub in subs {
            let dir = ws.path.join(&sub);
            if let Ok(inner) = delilah_git::repo_files(&dir, &opts) {
                files.extend(inner.into_iter().map(|f| format!("{sub}/{f}")));
            }
        }
        files.sort();
        files.truncate(MAX_LISTED);
        files
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(files))
}

/// `GET /api/v1/tasks/{key}/file?path=…`
pub async fn get(
    State(app): State<AppState>,
    Path(key): Path<String>,
    Query(q): Query<PathQuery>,
) -> Result<Json<FileContent>, ApiError> {
    let file = resolve(&root(&app, &TaskKey::new(key)).await?, &q.path)?;
    if !file.is_file() {
        return Err(ApiError::bad_request(format!("`{}` is not a file", q.path)));
    }
    Ok(Json(read(&file, &q.path).map_err(io_error)?))
}

/// `PUT /api/v1/tasks/{key}/file?path=…`: saves an existing file. 409 with the file
/// as it is now if it changed since `version`.
pub async fn put(
    State(app): State<AppState>,
    Path(key): Path<String>,
    Query(q): Query<PathQuery>,
    Json(save): Json<Save>,
) -> Result<Json<FileContent>, ApiError> {
    let file = resolve(&root(&app, &TaskKey::new(key)).await?, &q.path)?;
    if !file.is_file() {
        return Err(ApiError::bad_request(format!("`{}` is not a file", q.path)));
    }
    if version(&file).map_err(io_error)? != save.version {
        return Err(ApiError::conflict(format!(
            "`{}` changed on disk since you opened it",
            q.path
        )));
    }
    std::fs::write(&file, save.text).map_err(io_error)?;
    Ok(Json(read(&file, &q.path).map_err(io_error)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("delilah-files-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/main.rs"), "fn main() {}\n").unwrap();
        dir
    }

    #[test]
    fn paths_cannot_leave_the_workspace() {
        let ws = workspace();
        assert!(resolve(&ws, "src/main.rs").is_ok());
        assert!(resolve(&ws, "").is_ok());
        for bad in ["../outside", "src/../../outside", "/etc/passwd"] {
            assert!(resolve(&ws, bad).is_err(), "{bad} was accepted");
        }
        // A symlink pointing out is refused too.
        std::os::unix::fs::symlink("/etc", ws.join("etc")).unwrap();
        assert!(resolve(&ws, "etc/hosts").is_err());
        std::fs::remove_dir_all(ws).unwrap();
    }

    #[test]
    fn lists_folders_first_without_git_data() {
        let ws = workspace();
        std::fs::write(ws.join(".git"), "gitdir: elsewhere").unwrap();
        std::fs::write(ws.join("README.md"), "hi").unwrap();
        let names: Vec<_> = list_dir(&ws).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["src", "README.md"]);
        std::fs::remove_dir_all(ws).unwrap();
    }

    #[test]
    fn binary_files_are_not_editable_and_versions_follow_changes() {
        let ws = workspace();
        std::fs::write(ws.join("logo.png"), [0x89, b'P', b'N', b'G', 0, 0, 1]).unwrap();
        assert_eq!(read(&ws.join("logo.png"), "logo.png").unwrap().text, None);
        let main = ws.join("src/main.rs");
        let before = read(&main, "src/main.rs").unwrap();
        assert_eq!(before.text.as_deref(), Some("fn main() {}\n"));
        std::fs::write(&main, "fn main() { run() }\n").unwrap();
        assert_ne!(version(&main).unwrap(), before.version);
        std::fs::remove_dir_all(ws).unwrap();
    }
}
