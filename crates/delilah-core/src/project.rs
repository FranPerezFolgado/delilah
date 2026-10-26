use serde::{Deserialize, Serialize};

use crate::workflow::{ProjectWorkflow, validate_workflow};

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
    /// Per-project settings kept by Delilah (never written into the repo).
    #[serde(default)]
    pub settings: ProjectSettings,
    /// Command that starts the app for a preview (run in the task's workspace).
    #[serde(default)]
    pub preview_command: Option<String>,
}

/// How a task's workspace is made for a plain-repo project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceMode {
    /// Work in the main checkout.
    #[default]
    Checkout,
    /// Work in a git worktree of its own.
    Worktree,
}

/// How a shared path gets into a workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShareMode {
    Copy,
    Link,
}

/// A path of the main checkout made available inside each workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedEntry {
    /// Relative to the project root; no `..`.
    pub path: String,
    /// `None` lets the project kind decide at use.
    #[serde(default)]
    pub mode: Option<ShareMode>,
}

/// Which submodules a parent project's tasks always or never include.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SubmoduleRules {
    pub always: Vec<String>,
    pub never: Vec<String>,
}

/// Per-project settings. The default is how tasks started before they existed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectSettings {
    /// Where the project's tasks work when neither the task nor the request says:
    /// `None` (the default, JSON `null`) is a worktree, as before settings existed.
    pub workspace: Option<WorkspaceMode>,
    /// Branch template; `{ticket}` is required, `{slug}` optional. `None` (the
    /// default) leaves the name to Delilah's proposal (`feature/<descriptive>`).
    pub branch: Option<String>,
    pub shared: Vec<SharedEntry>,
    /// Command run in a new workspace; `None` (or blank) means none.
    pub setup: Option<String>,
    pub submodules: SubmoduleRules,
    /// How tasks start; `None` (the default) is a single start with the built-in prompt.
    pub workflow: Option<ProjectWorkflow>,
}

impl ProjectSettings {
    /// The workflow in force: the setting, or a single built-in start.
    pub fn workflow(&self) -> ProjectWorkflow {
        self.workflow.clone().unwrap_or_default()
    }
}

/// One problem found by [`validate`], tied to the field that has it.
///
/// `code` is stable (e.g. `branch.missing_ticket`) so the daemon can localise
/// `message`, which is English.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldError {
    /// e.g. `branch`, `shared[1].path`, `submodules.never[0]`.
    pub field: String,
    pub code: String,
    pub message: String,
}

fn err(field: impl Into<String>, code: &str, message: impl Into<String>) -> FieldError {
    FieldError {
        field: field.into(),
        code: code.to_string(),
        message: message.into(),
    }
}

/// Check settings before saving. Whether submodules exist in the parent needs
/// git, so it is not checked here.
pub fn validate(settings: &ProjectSettings, kind: ProjectKind) -> Vec<FieldError> {
    let mut errors = Vec::new();

    if let Some(branch) = settings.branch.as_deref().map(str::trim) {
        if branch.is_empty() {
            errors.push(err(
                "branch",
                "branch.empty",
                "The branch template is empty.",
            ));
        } else {
            let mut has_ticket = false;
            let mut rest = branch;
            while let Some(open) = rest.find('{') {
                let after = &rest[open + 1..];
                let Some(close) = after.find('}') else {
                    errors.push(err(
                        "branch",
                        "branch.unknown_placeholder",
                        "Unclosed `{` in the branch template.",
                    ));
                    break;
                };
                match &after[..close] {
                    "ticket" => has_ticket = true,
                    "slug" => {}
                    other => errors.push(err(
                        "branch",
                        "branch.unknown_placeholder",
                        format!(
                            "Unknown placeholder `{{{other}}}`; use `{{ticket}}` or `{{slug}}`."
                        ),
                    )),
                }
                rest = &after[close + 1..];
            }
            if !has_ticket {
                errors.push(err(
                    "branch",
                    "branch.missing_ticket",
                    "The branch template must contain `{ticket}`.",
                ));
            }
        }
    }

    for (i, entry) in settings.shared.iter().enumerate() {
        let field = format!("shared[{i}].path");
        let path = entry.path.trim();
        let p = std::path::Path::new(path);
        if path.is_empty() {
            errors.push(err(&field, "shared.path_empty", "The path is empty."));
        } else if p.is_absolute() || path.starts_with('/') {
            errors.push(err(
                &field,
                "shared.path_absolute",
                "The path must be relative to the project.",
            ));
        } else if p
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            errors.push(err(
                &field,
                "shared.path_parent",
                "The path must not contain `..`.",
            ));
        } else if settings.shared[..i].iter().any(|e| e.path.trim() == path) {
            errors.push(err(
                &field,
                "shared.path_duplicate",
                "The path is already listed.",
            ));
        }
    }

    if kind == ProjectKind::Repo {
        for (list, name) in [
            (&settings.submodules.always, "always"),
            (&settings.submodules.never, "never"),
        ] {
            for i in 0..list.len() {
                errors.push(err(
                    format!("submodules.{name}[{i}]"),
                    "submodules.not_parent",
                    "Submodule rules only apply to parent projects.",
                ));
            }
        }
    } else {
        for (i, m) in settings.submodules.never.iter().enumerate() {
            if settings.submodules.always.contains(m) {
                errors.push(err(
                    format!("submodules.never[{i}]"),
                    "submodules.conflict",
                    format!("`{m}` is in both the always and never lists."),
                ));
            }
        }
    }

    if let Some(w) = &settings.workflow {
        errors.extend(validate_workflow(w));
    }

    errors
}

/// Maximum length of the `{slug}` part of a branch name.
const SLUG_MAX: usize = 40;

/// Lower-case, hyphen-separated form of a task title for branch names: accents
/// stripped, only `a-z`, `0-9` and `-`, at most 40 characters, cut at the last
/// hyphen when it has to be shortened, never starting or ending with `-`.
pub fn slug(title: &str) -> String {
    use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};
    let mut out = String::new();
    for c in title.nfkd().filter(|c| !is_combining_mark(*c)) {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let mut out = out.trim_matches('-').to_string();
    if out.len() > SLUG_MAX {
        // All ASCII here, so byte indexes are char indexes.
        let cut = if out.as_bytes()[SLUG_MAX] == b'-' {
            SLUG_MAX
        } else {
            out[..SLUG_MAX].rfind('-').unwrap_or(SLUG_MAX)
        };
        out.truncate(cut);
        out = out.trim_matches('-').to_string();
    }
    out
}

/// Expands a branch template: `{ticket}` is the task key verbatim, `{slug}` the
/// slug of the title. With an empty slug the `-`, `_`, `.` or `/` run just before
/// `{slug}` goes too (or the one just after, if `{slug}` opens the template), so
/// no separator is left dangling.
pub fn branch_name(template: &str, key: &str, title: &str) -> String {
    let slug = slug(title);
    let is_sep = |c: char| matches!(c, '-' | '_' | '.' | '/');
    let mut t = template.to_string();
    if slug.is_empty() {
        while let Some(i) = t.find("{slug}") {
            let before = t[..i].trim_end_matches(is_sep).len();
            let mut after = i + "{slug}".len();
            if before == 0 {
                after = t.len() - t[after..].trim_start_matches(is_sep).len();
            }
            t.replace_range(before..after, "");
        }
    } else {
        t = t.replace("{slug}", &slug);
    }
    t.replace("{ticket}", key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> ProjectSettings {
        ProjectSettings::default()
    }

    fn shared(path: &str) -> SharedEntry {
        SharedEntry {
            path: path.into(),
            mode: None,
        }
    }

    fn fields(errors: &[FieldError]) -> Vec<(&str, &str)> {
        errors
            .iter()
            .map(|e| (e.field.as_str(), e.code.as_str()))
            .collect()
    }

    #[test]
    fn default_is_todays_behaviour() {
        let s = ProjectSettings::default();
        assert_eq!(s.workspace, None);
        assert_eq!(s.branch, None);
        assert!(s.shared.is_empty());
        assert_eq!(s.setup, None);
        assert!(s.submodules.always.is_empty() && s.submodules.never.is_empty());
        assert!(validate(&s, ProjectKind::Repo).is_empty());
    }

    #[test]
    fn workspace_is_null_when_unset_and_always_serialised() {
        let s: ProjectSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(s.workspace, None);
        let json = serde_json::to_value(&s).unwrap();
        assert!(json["workspace"].is_null());
        let n: ProjectSettings = serde_json::from_str(r#"{"workspace":null}"#).unwrap();
        assert_eq!(n.workspace, None);
        let c: ProjectSettings = serde_json::from_str(r#"{"workspace":"checkout"}"#).unwrap();
        assert_eq!(c.workspace, Some(WorkspaceMode::Checkout));
    }

    #[test]
    fn empty_object_gives_defaults() {
        let s: ProjectSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, ProjectSettings::default());
    }

    #[test]
    fn full_settings_round_trip() {
        let json = r#"{"workspace":"worktree","branch":"fix/{ticket}-{slug}","shared":[{"path":".env","mode":"copy"},{"path":"node_modules","mode":null}],"setup":"npm ci","submodules":{"always":["a"],"never":["b"]},"workflow":null}"#;
        let s: ProjectSettings = serde_json::from_str(json).unwrap();
        assert_eq!(s.workspace, Some(WorkspaceMode::Worktree));
        assert_eq!(s.shared[0].mode, Some(ShareMode::Copy));
        assert_eq!(s.shared[1].mode, None);
        assert_eq!(serde_json::to_string(&s).unwrap(), json);
    }

    #[test]
    fn old_settings_without_a_branch_read_as_none() {
        let s: ProjectSettings = serde_json::from_str(r#"{"workspace":"worktree"}"#).unwrap();
        assert_eq!(s.branch, None);
    }

    #[test]
    fn serialises_every_field() {
        let json = serde_json::to_string(&ProjectSettings::default()).unwrap();
        assert_eq!(
            json,
            r#"{"workspace":null,"branch":null,"shared":[],"setup":null,"submodules":{"always":[],"never":[]},"workflow":null}"#
        );
    }

    #[test]
    fn unknown_keys_are_refused() {
        assert!(serde_json::from_str::<ProjectSettings>(r#"{"nope":1}"#).is_err());
        assert!(
            serde_json::from_str::<ProjectSettings>(r#"{"shared":[{"path":"a","x":1}]}"#).is_err()
        );
        assert!(serde_json::from_str::<ProjectSettings>(r#"{"submodules":{"x":[]}}"#).is_err());
    }

    #[test]
    fn branch_rules() {
        let mut s = settings();
        s.branch = Some("feature/x".into());
        assert_eq!(
            fields(&validate(&s, ProjectKind::Repo)),
            [("branch", "branch.missing_ticket")]
        );
        s.branch = Some("{ticket}/{foo}".into());
        assert_eq!(
            fields(&validate(&s, ProjectKind::Repo)),
            [("branch", "branch.unknown_placeholder")]
        );
        s.branch = Some("  ".into());
        assert_eq!(
            fields(&validate(&s, ProjectKind::Repo)),
            [("branch", "branch.empty")]
        );
        s.branch = Some("{ticket}-{slug}".into());
        assert!(validate(&s, ProjectKind::Repo).is_empty());
        s.branch = None;
        assert!(validate(&s, ProjectKind::Repo).is_empty());
    }

    #[test]
    fn shared_path_rules() {
        let mut s = settings();
        s.shared = vec![
            shared(".env"),
            shared("/etc"),
            shared("../x"),
            shared(""),
            shared(".env"),
            shared("a/../b"),
        ];
        assert_eq!(
            fields(&validate(&s, ProjectKind::Repo)),
            [
                ("shared[1].path", "shared.path_absolute"),
                ("shared[2].path", "shared.path_parent"),
                ("shared[3].path", "shared.path_empty"),
                ("shared[4].path", "shared.path_duplicate"),
                ("shared[5].path", "shared.path_parent"),
            ]
        );
    }

    #[test]
    fn submodule_rules() {
        let mut s = settings();
        s.submodules.always = vec!["a".into()];
        s.submodules.never = vec!["b".into(), "a".into()];
        assert_eq!(
            fields(&validate(&s, ProjectKind::Parent)),
            [("submodules.never[1]", "submodules.conflict")]
        );
        assert_eq!(
            fields(&validate(&s, ProjectKind::Repo)),
            [
                ("submodules.always[0]", "submodules.not_parent"),
                ("submodules.never[0]", "submodules.not_parent"),
                ("submodules.never[1]", "submodules.not_parent"),
            ]
        );
    }

    #[test]
    fn branch_names_match_shared_cases() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../tests/branch_names.json")).unwrap();
        assert!(cases.len() >= 12);
        for c in cases {
            let f = |k: &str| c[k].as_str().unwrap().to_string();
            assert_eq!(
                branch_name(&f("template"), &f("key"), &f("title")),
                f("branch"),
                "case {c}"
            );
        }
    }

    #[test]
    fn workflow_defaults_to_single_builtin() {
        let s: ProjectSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(s.workflow, None);
        assert_eq!(
            s.workflow(),
            ProjectWorkflow::Single {
                start: crate::workflow::Start::Builtin
            }
        );
        let s: ProjectSettings = serde_json::from_str(
            r#"{"workflow":{"kind":"phased","plan":"a","implement":"b","review":"c"}}"#,
        )
        .unwrap();
        assert!(matches!(s.workflow(), ProjectWorkflow::Phased { .. }));
    }

    #[test]
    fn workflow_errors_come_with_the_others() {
        let mut s = settings();
        s.branch = Some("x".into());
        s.workflow = Some(ProjectWorkflow::Phased {
            plan: " ".into(),
            implement: "{spec_dir} {bad}".into(),
            review: "{spec_dir}".into(),
        });
        assert_eq!(
            fields(&validate(&s, ProjectKind::Repo)),
            [
                ("branch", "branch.missing_ticket"),
                ("workflow.plan", "workflow.blank"),
                ("workflow.implement", "workflow.unknown_placeholder"),
                ("workflow.review", "workflow.placeholder_not_allowed"),
            ]
        );
    }

    #[test]
    fn old_projects_load_without_a_preview_command() {
        let json = r#"{"name":"p","path":"/tmp/p","kind":"repo","workflow":"feature"}"#;
        let project: Project = serde_json::from_str(json).unwrap();
        assert_eq!(project.preview_command, None);
    }
}
