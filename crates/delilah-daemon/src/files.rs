//! The task's workspace as files, for the app's editor: list a folder, read a file,
//! save it, serve an image for a preview. Every path is relative to the workspace and
//! must stay inside it, also through symlinks. Saving checks the file's version first,
//! so an edit made in the editor never silently overwrites what the agent wrote
//! meanwhile.

use std::path::{Component, Path as FsPath, PathBuf};
use std::time::UNIX_EPOCH;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use delilah_core::TaskKey;
use serde::{Deserialize, Serialize};

use crate::api::ApiError;
use crate::state::AppState;

/// Enough for any repo worth opening here; past this the list is cut.
const MAX_LISTED: usize = 100_000;

/// Bigger files are shown as not editable: an editor in a panel is not the tool.
pub const MAX_EDIT_BYTES: u64 = 1 << 20;

/// Bigger images are not shown in a preview.
const MAX_IMAGE_BYTES: u64 = 10 << 20;

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

/// The `Content-Type` of an image, by its extension. `None` for anything else.
fn image_type(path: &FsPath) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "avif" => "image/avif",
        _ => return None,
    })
}

/// The image's `Content-Type` if `rel` can be served as one at `size` bytes.
fn check_image(rel: &str, size: u64) -> Result<&'static str, ApiError> {
    let kind = image_type(FsPath::new(rel))
        .ok_or_else(|| ApiError::unsupported_type(format!("`{rel}` is not an image")))?;
    if size > MAX_IMAGE_BYTES {
        return Err(ApiError::too_large(format!("`{rel}` is too big to show")));
    }
    Ok(kind)
}

async fn root(app: &AppState, key: &TaskKey) -> Result<PathBuf, ApiError> {
    Ok(crate::owners::workspace(app, key.as_str()).await?.path)
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
    let ws = crate::owners::workspace(&app, &key).await?;
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

/// `GET /api/v1/tasks/{key}/image?path=…`: an image's bytes, for a Markdown preview.
pub async fn image(
    State(app): State<AppState>,
    Path(key): Path<String>,
    Query(q): Query<PathQuery>,
) -> Result<Response, ApiError> {
    let file = resolve(&root(&app, &TaskKey::new(key)).await?, &q.path)?;
    if !file.is_file() {
        return Err(ApiError::bad_request(format!("`{}` is not a file", q.path)));
    }
    let size = std::fs::metadata(&file).map_err(io_error)?.len();
    // Refuses a huge file before reading any of it.
    let kind = check_image(&q.path, size)?;
    let bytes = tokio::task::spawn_blocking(move || read_capped(&file))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .map_err(io_error)?;
    // The file may have grown since its size was checked.
    let bytes =
        bytes.ok_or_else(|| ApiError::too_large(format!("`{}` is too big to show", q.path)))?;
    Ok(image_response(kind, bytes))
}

/// The file's bytes, or `None` if it holds more than [`MAX_IMAGE_BYTES`]. Reads at
/// most one byte past the limit, whatever the file's size says.
fn read_capped(path: &FsPath) -> std::io::Result<Option<Vec<u8>>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    Ok((bytes.len() as u64 <= MAX_IMAGE_BYTES).then_some(bytes))
}

fn image_response(kind: &'static str, bytes: Vec<u8>) -> Response {
    // An SVG opened straight from this URL would run its scripts as the daemon's
    // origin: the sandbox keeps it an inert image.
    (
        [
            (header::CONTENT_TYPE, kind),
            (header::CACHE_CONTROL, "no-cache"),
            (
                header::CONTENT_SECURITY_POLICY,
                "sandbox; default-src 'none'; style-src 'unsafe-inline'",
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            // No other site can probe with an `<img>` whether a workspace image
            // exists; the app's CORS fetch is not affected.
            (
                header::HeaderName::from_static("cross-origin-resource-policy"),
                "same-origin",
            ),
        ],
        bytes,
    )
        .into_response()
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

    #[test]
    fn images_are_typed_by_extension_whatever_its_case() {
        assert_eq!(image_type(FsPath::new("docs/logo.png")), Some("image/png"));
        assert_eq!(image_type(FsPath::new("a.jpeg")), Some("image/jpeg"));
        assert_eq!(image_type(FsPath::new("a.svg")), Some("image/svg+xml"));
        assert_eq!(image_type(FsPath::new("Shot.JPG")), Some("image/jpeg"));
        assert_eq!(image_type(FsPath::new("a.AvIf")), Some("image/avif"));
        for other in ["README.md", "a.html", "png", "a.png.exe"] {
            assert_eq!(image_type(FsPath::new(other)), None, "{other} was typed");
        }
    }

    #[test]
    fn only_images_small_enough_are_served() {
        assert_eq!(
            check_image("logo.png", MAX_IMAGE_BYTES).ok(),
            Some("image/png")
        );
        let status = |r: Result<&str, ApiError>| r.err().unwrap().into_response().status();
        assert_eq!(
            status(check_image("logo.png", MAX_IMAGE_BYTES + 1)),
            axum::http::StatusCode::PAYLOAD_TOO_LARGE
        );
        assert_eq!(
            status(check_image("notes.md", 10)),
            axum::http::StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
    }

    #[test]
    fn a_capped_read_refuses_one_byte_too_many() {
        let ws = workspace();
        let file = ws.join("big.png");
        std::fs::write(&file, vec![0u8; MAX_IMAGE_BYTES as usize]).unwrap();
        assert_eq!(
            read_capped(&file).unwrap().map(|b| b.len() as u64),
            Some(MAX_IMAGE_BYTES)
        );
        std::fs::write(&file, vec![0u8; MAX_IMAGE_BYTES as usize + 1]).unwrap();
        assert_eq!(read_capped(&file).unwrap(), None);
        std::fs::remove_dir_all(ws).unwrap();
    }

    /// A task whose workspace is a folder with a PNG and a Markdown file in it.
    async fn task_with_images() -> (AppState, PathBuf) {
        let ws = workspace();
        std::fs::write(ws.join("logo.png"), [0x89, b'P', b'N', b'G']).unwrap();
        std::fs::write(ws.join("notes.md"), "# hi\n").unwrap();
        let config = crate::config::Config {
            home: ws.join("home"),
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            agent_program: None,
            ..crate::config::Config::test_defaults()
        };
        let app = AppState::new(crate::store::Store::in_memory().await.unwrap(), config);
        let key = TaskKey::new("PROJ-1");
        app.store
            .create_task(&delilah_core::Task::new(key.clone(), "t", "feature"))
            .await
            .unwrap();
        let workspace = delilah_git::Workspace {
            path: ws.clone(),
            branch: "feature/PROJ-1".into(),
            kind: delilah_core::ProjectKind::Repo,
            submodules: vec![],
        };
        app.store
            .save_workspace(&key, &ws, &workspace)
            .await
            .unwrap();
        (app, ws)
    }

    #[tokio::test]
    async fn the_image_route_serves_only_images_inside_the_workspace() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt;

        let (app, ws) = task_with_images().await;
        let get = |path: &str| {
            let req = Request::get(format!("/api/v1/tasks/PROJ-1/image?path={path}"))
                .body(Body::empty())
                .unwrap();
            crate::api::router(app.clone()).oneshot(req)
        };

        let res = get("logo.png").await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let h = res.headers();
        assert_eq!(h[header::CONTENT_TYPE], "image/png");
        assert!(
            h[header::CONTENT_SECURITY_POLICY]
                .to_str()
                .unwrap()
                .starts_with("sandbox")
        );
        assert_eq!(h["cross-origin-resource-policy"], "same-origin");
        assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff");

        let res = get("notes.md").await.unwrap();
        assert_eq!(res.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        let res = get("..%2Foutside.png").await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        std::fs::remove_dir_all(ws).unwrap();
    }
}
