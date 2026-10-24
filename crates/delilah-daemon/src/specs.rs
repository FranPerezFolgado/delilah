//! Follows each task's SpecKit spec in its workspace (`specs/<feature>/`): the phase
//! it is in and its `tasks.md` checklist. The repo stays the source of truth; Delilah
//! only reads it. A watcher keeps a summary of every task for the board and records
//! phase changes in the task's history.

use std::collections::HashMap;
use std::path::{Path as FsPath, PathBuf};
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use delilah_core::{ProjectKind, SpecGroup, SpecPhase, TaskKey, parse_tasks};
use delilah_git::{GitOptions, Workspace, changed_paths};
use serde::Serialize;

use crate::api::ApiError;
use crate::state::AppState;

pub const TICK: Duration = Duration::from_secs(5);
const SPECS: &str = "specs";

/// What the board shows of a task's spec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpecSummary {
    pub phase: SpecPhase,
    /// The spec folder, relative to the workspace; `None` until the agent creates it.
    pub dir: Option<String>,
    pub done: usize,
    pub total: usize,
    /// The first item left to do, e.g. "T010 Contract test for …".
    pub current: Option<String>,
}

/// The whole spec, for the task's own view.
#[derive(Debug, Clone, Serialize)]
pub struct SpecProgress {
    #[serde(flatten)]
    pub summary: SpecSummary,
    /// The Markdown files in the folder (spec.md, plan.md, research.md…).
    pub files: Vec<String>,
    pub groups: Vec<SpecGroup>,
}

/// Reads the spec of a task from its workspace. Blocking: files and git.
pub fn read_spec(ws: &Workspace, key: &str, opts: &GitOptions) -> SpecProgress {
    let Some(dir) = find_spec_dir(ws, key, opts) else {
        return SpecProgress {
            summary: SpecSummary {
                phase: SpecPhase::Specify,
                dir: None,
                done: 0,
                total: 0,
                current: None,
            },
            files: Vec::new(),
            groups: Vec::new(),
        };
    };
    let mut files: Vec<String> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".md"))
        .collect();
    files.sort_by_key(|name| (file_order(name), name.clone()));
    let tasks = std::fs::read_to_string(dir.join("tasks.md"))
        .ok()
        .map(|md| parse_tasks(&md));
    let groups = tasks.clone().unwrap_or_default();
    let items = || groups.iter().flat_map(|g| &g.items);
    let current = items().find(|i| !i.done).map(|i| match &i.id {
        Some(id) => format!("{id} {}", i.text),
        None => i.text.clone(),
    });
    SpecProgress {
        summary: SpecSummary {
            phase: SpecPhase::of(dir.join("plan.md").is_file(), tasks.as_deref()),
            dir: dir
                .strip_prefix(&ws.path)
                .ok()
                .map(|d| d.display().to_string()),
            done: items().filter(|i| i.done).count(),
            total: items().count(),
            current,
        },
        files,
        groups,
    }
}

/// SpecKit's documents in the order they are written.
fn file_order(name: &str) -> usize {
    [
        "spec.md",
        "plan.md",
        "research.md",
        "data-model.md",
        "quickstart.md",
        "tasks.md",
    ]
    .iter()
    .position(|f| *f == name)
    .unwrap_or(usize::MAX)
}

/// The task's spec folder. `specs/` may also hold the features of other tickets,
/// merged long ago, so it is the folder named after the ticket (or the branch) or,
/// failing that, the one the task's branch has been writing to.
fn find_spec_dir(ws: &Workspace, key: &str, opts: &GitOptions) -> Option<PathBuf> {
    let roots: Vec<PathBuf> = match ws.kind {
        ProjectKind::Repo => vec![ws.path.clone()],
        // The parent's own specs first, then each submodule's.
        ProjectKind::Parent => std::iter::once(ws.path.clone())
            .chain(ws.submodules.iter().map(|s| ws.path.join(s)))
            .collect(),
    };
    let key = key.to_lowercase();
    let branch = ws
        .branch
        .rsplit('/')
        .next()
        .unwrap_or(&ws.branch)
        .to_lowercase();
    for root in &roots {
        let named = spec_dirs(root).into_iter().find(|dir| {
            let name = dir
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            name.contains(&key) || (ws.has_branch() && name == branch)
        });
        if named.is_some() {
            return named;
        }
    }
    // Without a branch there is nothing it has written to.
    if !ws.has_branch() {
        return None;
    }
    roots.iter().find_map(|root| {
        let touched = changed_paths(root, SPECS, &ws.branch, opts).ok()?;
        touched.iter().find_map(|path| {
            let feature = FsPath::new(path)
                .strip_prefix(SPECS)
                .ok()?
                .components()
                .next()?;
            let dir = root.join(SPECS).join(feature);
            dir.is_dir().then_some(dir)
        })
    })
}

fn spec_dirs(root: &FsPath) -> Vec<PathBuf> {
    std::fs::read_dir(root.join(SPECS))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect()
}

fn git_options(app: &AppState) -> GitOptions {
    GitOptions {
        allow_file_protocol: app.config.allow_file_protocol,
    }
}

async fn workspace_of(app: &AppState, key: &TaskKey) -> Result<Option<Workspace>, ApiError> {
    let ws = app.store.workspace(key).await?.map(|(_, ws)| ws);
    Ok(ws.filter(|ws| ws.path.is_dir()))
}

async fn read_spec_of(app: &AppState, key: &TaskKey) -> Result<Option<SpecProgress>, ApiError> {
    let Some(ws) = workspace_of(app, key).await? else {
        return Ok(None);
    };
    let opts = git_options(app);
    let key = key.to_string();
    let spec = tokio::task::spawn_blocking(move || read_spec(&ws, &key, &opts))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Some(spec))
}

/// `GET /api/v1/specs`: the board's summary of every task with a workspace.
pub async fn list(State(app): State<AppState>) -> Json<HashMap<String, SpecSummary>> {
    Json(app.specs.lock().unwrap().clone())
}

/// `GET /api/v1/tasks/{key}/spec`: the whole spec, read now; `null` without a workspace.
pub async fn of_task(
    State(app): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Option<SpecProgress>>, ApiError> {
    let key = TaskKey::new(key);
    let spec = read_spec_of(&app, &key).await?;
    if let Some(spec) = &spec {
        record(&app, &key, &spec.summary).await;
    }
    Ok(Json(spec))
}

/// `GET /api/v1/tasks/{key}/spec/files/{name}`: one of the spec's Markdown files.
pub async fn file(
    State(app): State<AppState>,
    Path((key, name)): Path<(String, String)>,
) -> Result<String, ApiError> {
    let key = TaskKey::new(key);
    let missing = || ApiError::not_found(format!("{key} has no spec file `{name}`"));
    let spec = read_spec_of(&app, &key).await?.ok_or_else(missing)?;
    // Only a file the listing offered: no paths, no way out of the folder.
    if !spec.files.contains(&name) {
        return Err(missing());
    }
    let ws = workspace_of(&app, &key).await?.ok_or_else(missing)?;
    let dir = spec.summary.dir.as_deref().ok_or_else(missing)?;
    // `specs/` may be a symlink in the repo: the file must still be inside the workspace.
    let path = crate::files::resolve(&ws.path, &format!("{dir}/{name}"))?;
    std::fs::read_to_string(path).map_err(|_| missing())
}

/// Keeps the board's summary and, when the phase moves, the task's history.
async fn record(app: &AppState, key: &TaskKey, summary: &SpecSummary) {
    let before = app
        .specs
        .lock()
        .unwrap()
        .insert(key.to_string(), summary.clone());
    if before.as_ref() != Some(summary) {
        app.emit(crate::events::Topic::Specs);
        // Reaching review puts the task in the inbox.
        app.emit(crate::events::Topic::Inbox);
    }
    if summary.dir.is_none() {
        return;
    }
    // The files only push forward: they never undo a phase the user chose.
    crate::phases::advance(app, key, summary.phase).await;
}

pub fn spawn(app: AppState) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(TICK);
        loop {
            ticker.tick().await;
            tick(&app).await;
        }
    });
}

pub async fn tick(app: &AppState) {
    let tasks = match app.store.list_tasks().await {
        Ok(tasks) => tasks,
        Err(e) => return tracing::warn!("specs: {e}"),
    };
    let mut seen = Vec::new();
    for task in tasks {
        match read_spec_of(app, &task.key).await {
            Ok(Some(spec)) => {
                record(app, &task.key, &spec.summary).await;
                seen.push(task.key.to_string());
            }
            Ok(None) => {}
            Err(e) => tracing::warn!(key = %task.key, "specs: {}", e.message()),
        }
    }
    // Archived workspaces leave the board's summary.
    app.specs
        .lock()
        .unwrap()
        .retain(|key, _| seen.contains(key));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(dir: &FsPath) -> Workspace {
        Workspace {
            path: dir.to_path_buf(),
            branch: "proj-7-retries".into(),
            kind: ProjectKind::Repo,
            submodules: Vec::new(),
        }
    }

    fn write(path: PathBuf, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn finds_the_folder_named_after_the_ticket_among_older_ones() {
        let dir = std::env::temp_dir().join(format!("delilah-spec-{}", uuid::Uuid::new_v4()));
        write(
            dir.join("specs/001-old-feature/tasks.md"),
            "- [x] T001 old\n",
        );
        write(dir.join("specs/PROJ-7-card-retries/spec.md"), "# Spec\n");
        write(dir.join("specs/PROJ-7-card-retries/plan.md"), "# Plan\n");
        write(
            dir.join("specs/PROJ-7-card-retries/tasks.md"),
            "## Setup\n- [x] T001 Module\n- [ ] T002 [P] Policy\n",
        );

        let spec = read_spec(&workspace(&dir), "PROJ-7", &GitOptions::default());
        assert_eq!(
            spec.summary.dir.as_deref(),
            Some("specs/PROJ-7-card-retries")
        );
        assert_eq!(spec.summary.phase, SpecPhase::Implement);
        assert_eq!((spec.summary.done, spec.summary.total), (1, 2));
        assert_eq!(spec.summary.current.as_deref(), Some("T002 Policy"));
        assert_eq!(spec.files, ["spec.md", "plan.md", "tasks.md"]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn otherwise_finds_the_folder_the_branch_has_been_writing_to() {
        let dir = std::env::temp_dir().join(format!("delilah-spec-{}", uuid::Uuid::new_v4()));
        let git = |args: &[&str]| {
            let ok = std::process::Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(["-c", "user.name=t", "-c", "user.email=t@t"])
                .args(args)
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?}");
        };
        std::fs::create_dir_all(&dir).unwrap();
        git(&["init", "-q", "-b", "main"]);
        write(
            dir.join("specs/001-old-feature/tasks.md"),
            "- [x] T001 old\n",
        );
        git(&["add", "."]);
        git(&["commit", "-qm", "old feature"]);
        git(&["switch", "-qc", "proj-7-retries"]);
        write(dir.join("specs/002-card-retries/spec.md"), "# Spec\n");

        let spec = read_spec(&workspace(&dir), "PROJ-7", &GitOptions::default());
        assert_eq!(spec.summary.dir.as_deref(), Some("specs/002-card-retries"));
        assert_eq!(spec.summary.phase, SpecPhase::Specify);

        // Still found once committed on the branch.
        git(&["add", "."]);
        git(&["commit", "-qm", "spec"]);
        let spec = read_spec(&workspace(&dir), "PROJ-7", &GitOptions::default());
        assert_eq!(spec.summary.dir.as_deref(), Some("specs/002-card-retries"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_branchless_task_finds_only_the_folder_named_after_the_ticket() {
        let dir = std::env::temp_dir().join(format!("delilah-spec-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let ok = std::process::Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(["init", "-q", "-b", "main"])
            .status()
            .unwrap()
            .success();
        assert!(ok);
        write(dir.join("specs/002-card-retries/spec.md"), "# Spec\n");
        let mut ws = workspace(&dir);
        ws.branch = String::new();
        let spec = read_spec(&ws, "PROJ-7", &GitOptions::default());
        assert_eq!(spec.summary.dir, None);
        assert_eq!(spec.summary.phase, SpecPhase::Specify);

        write(dir.join("specs/PROJ-7-card-retries/spec.md"), "# Spec\n");
        let spec = read_spec(&ws, "PROJ-7", &GitOptions::default());
        assert_eq!(
            spec.summary.dir.as_deref(),
            Some("specs/PROJ-7-card-retries")
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn finds_a_spec_folder_kept_out_of_git() {
        // The `feature` workflow adds `specs/` to .gitignore and names the folder
        // after the feature, not the ticket.
        let dir = std::env::temp_dir().join(format!("delilah-spec-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let ok = std::process::Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(["init", "-q", "-b", "main"])
            .status()
            .unwrap()
            .success();
        assert!(ok);
        write(dir.join(".gitignore"), ".specify/\nspecs/\n");
        write(dir.join("specs/003-card-retries/spec.md"), "# Spec\n");
        write(dir.join("specs/003-card-retries/plan.md"), "# Plan\n");

        let spec = read_spec(&workspace(&dir), "PROJ-7", &GitOptions::default());
        assert_eq!(spec.summary.dir.as_deref(), Some("specs/003-card-retries"));
        assert_eq!(spec.summary.phase, SpecPhase::Plan);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn without_a_spec_folder_it_is_still_being_specified() {
        let dir = std::env::temp_dir().join(format!("delilah-spec-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let spec = read_spec(&workspace(&dir), "PROJ-7", &GitOptions::default());
        assert_eq!(spec.summary.phase, SpecPhase::Specify);
        assert_eq!(spec.summary.dir, None);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
