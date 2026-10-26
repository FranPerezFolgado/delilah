//! Per-project workflow commands: what the first message of a task is, and the
//! plan/implement/review commands of a phased workflow. Pure logic, no I/O.

use serde::{Deserialize, Serialize};

use crate::project::{FieldError, ProjectSettings};

/// How a project's tasks run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ProjectWorkflow {
    /// One message to start with (or none).
    Single { start: Start },
    /// Plan, implement and review, each a command template.
    Phased {
        plan: String,
        implement: String,
        review: String,
    },
}

impl Default for ProjectWorkflow {
    fn default() -> Self {
        Self::Single {
            start: Start::Builtin,
        }
    }
}

/// The first message of a single-start workflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Start {
    /// Delilah's own start prompt.
    Builtin,
    /// A template of the user's.
    Message { template: String },
    /// No first message.
    Nothing,
}

/// A task's own choice of workflow (the `tasks.workflow` column).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskWorkflow {
    /// Follow the project's workflow.
    Project,
    /// Start with the message stored in `start_message`.
    Prompt,
    /// Start with nothing.
    None,
}

impl TaskWorkflow {
    /// `feature` is the old name of `project`; anything unknown is an error.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "project" | "feature" => Ok(Self::Project),
            "prompt" => Ok(Self::Prompt),
            "none" => Ok(Self::None),
            other => Err(format!("unknown workflow `{other}`")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Prompt => "prompt",
            Self::None => "none",
        }
    }
}

/// Values a template can use. `task` is the ticket for a Jira key and the title
/// for a free key; the caller works that out.
#[derive(Debug, Clone, Copy)]
pub struct Vars<'a> {
    pub ticket: &'a str,
    pub title: &'a str,
    pub task: &'a str,
    pub spec_dir: Option<&'a str>,
}

/// Expands `{ticket}` `{title}` `{task}` `{spec_dir}`; `{{` and `}}` are literal
/// braces. Assumes the template was validated: anything else is kept as written.
pub fn render(template: &str, vars: &Vars) -> String {
    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                out.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                out.push('}');
            }
            '{' => {
                let mut name = String::new();
                let mut closed = false;
                for n in chars.by_ref() {
                    if n == '}' {
                        closed = true;
                        break;
                    }
                    name.push(n);
                }
                let value = match name.as_str() {
                    "ticket" => Some(vars.ticket),
                    "title" => Some(vars.title),
                    "task" => Some(vars.task),
                    "spec_dir" => vars.spec_dir,
                    _ => None,
                };
                match value {
                    Some(v) if closed => out.push_str(v),
                    _ => {
                        out.push('{');
                        out.push_str(&name);
                        if closed {
                            out.push('}');
                        }
                    }
                }
            }
            c => out.push(c),
        }
    }
    out
}

fn err(field: &str, code: &str, message: String) -> FieldError {
    FieldError {
        field: field.to_string(),
        code: code.to_string(),
        message,
    }
}

/// Checks one template: not blank, balanced braces, known placeholders
/// (`{spec_dir}` only where `allow_spec_dir`).
pub fn validate(field: &str, template: &str, allow_spec_dir: bool) -> Vec<FieldError> {
    if template.trim().is_empty() {
        return vec![err(
            field,
            "workflow.blank",
            "The command is empty.".to_string(),
        )];
    }
    let mut errors = Vec::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
            }
            '}' => errors.push(err(
                field,
                "workflow.unbalanced_brace",
                "Unmatched `}`; write `}}` for a literal brace.".to_string(),
            )),
            '{' => {
                let mut name = String::new();
                let mut closed = false;
                for n in chars.by_ref() {
                    if n == '}' {
                        closed = true;
                        break;
                    }
                    name.push(n);
                }
                if !closed || name.contains('{') {
                    errors.push(err(
                        field,
                        "workflow.unbalanced_brace",
                        "Unclosed `{`; write `{{` for a literal brace.".to_string(),
                    ));
                    break;
                }
                match name.as_str() {
                    "ticket" | "title" | "task" => {}
                    "spec_dir" if allow_spec_dir => {}
                    "spec_dir" => errors.push(err(
                        field,
                        "workflow.placeholder_not_allowed",
                        "`{spec_dir}` can only be used in the implement command.".to_string(),
                    )),
                    other => errors.push(err(
                        field,
                        "workflow.unknown_placeholder",
                        format!(
                            "Unknown placeholder `{{{other}}}`; use `{{ticket}}`, `{{title}}` or `{{task}}`."
                        ),
                    )),
                }
            }
            _ => {}
        }
    }
    errors
}

/// Validates every template of a project workflow.
pub fn validate_workflow(workflow: &ProjectWorkflow) -> Vec<FieldError> {
    match workflow {
        ProjectWorkflow::Single {
            start: Start::Message { template },
        } => validate("workflow.start.template", template, false),
        ProjectWorkflow::Single { .. } => Vec::new(),
        ProjectWorkflow::Phased {
            plan,
            implement,
            review,
        } => {
            let mut errors = validate("workflow.plan", plan, false);
            errors.extend(validate("workflow.implement", implement, true));
            errors.extend(validate("workflow.review", review, false));
            errors
        }
    }
}

/// Only tasks that follow a phased project workflow are phased.
pub fn is_phased(task: TaskWorkflow, project: &ProjectSettings) -> bool {
    task == TaskWorkflow::Project && matches!(project.workflow(), ProjectWorkflow::Phased { .. })
}

/// What a start request says about the first message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartRequest {
    /// Not said: use the task's stored choice.
    Inherit,
    /// Follow the project's workflow.
    Project,
    /// This message (a template).
    Prompt(String),
    /// No first message.
    Nothing,
}

/// The first message to send and what to store on the task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedStart {
    /// `None` = send nothing.
    pub prompt: Option<String>,
    /// Saving mode, autopilot and hand-offs apply only if true.
    pub phased: bool,
    /// Value for `tasks.workflow`.
    pub workflow: TaskWorkflow,
    /// Value for `tasks.start_message`.
    pub start_message: Option<String>,
}

impl ResolvedStart {
    fn nothing() -> Self {
        Self {
            prompt: None,
            phased: false,
            workflow: TaskWorkflow::None,
            start_message: None,
        }
    }

    fn message(template: &str, vars: &Vars) -> Self {
        Self {
            prompt: Some(render(template, vars)),
            phased: false,
            workflow: TaskWorkflow::Prompt,
            start_message: Some(template.to_string()),
        }
    }

    fn project(project: Option<&ProjectSettings>, vars: &Vars, builtin_text: &str) -> Self {
        let (prompt, phased) = match project.map(ProjectSettings::workflow) {
            None
            | Some(ProjectWorkflow::Single {
                start: Start::Builtin,
            }) => (Some(builtin_text.to_string()), false),
            Some(ProjectWorkflow::Single {
                start: Start::Message { template },
            }) => (Some(render(&template, vars)), false),
            Some(ProjectWorkflow::Single {
                start: Start::Nothing,
            }) => (None, false),
            Some(ProjectWorkflow::Phased { plan, .. }) => (Some(render(&plan, vars)), true),
        };
        Self {
            prompt,
            phased,
            workflow: TaskWorkflow::Project,
            start_message: None,
        }
    }
}

/// Decides the first message: the start request, then the task's stored choice,
/// then the project's workflow, then the built-in prompt. `stored_workflow` and
/// `stored_message` are the task's `workflow` and `start_message` columns (an
/// unparseable workflow starts with nothing; the caller may want to log it).
pub fn resolve(
    req: &StartRequest,
    stored_workflow: &str,
    stored_message: Option<&str>,
    project: Option<&ProjectSettings>,
    vars: &Vars,
    builtin_text: &str,
) -> ResolvedStart {
    match req {
        StartRequest::Prompt(p) => ResolvedStart::message(p, vars),
        StartRequest::Nothing => ResolvedStart::nothing(),
        StartRequest::Project => ResolvedStart::project(project, vars, builtin_text),
        StartRequest::Inherit => match TaskWorkflow::parse(stored_workflow) {
            Ok(TaskWorkflow::Project) => ResolvedStart::project(project, vars, builtin_text),
            Ok(TaskWorkflow::Prompt) => match stored_message {
                Some(m) => ResolvedStart::message(m, vars),
                // Legacy: a prompt task that never stored its message.
                None => ResolvedStart::nothing(),
            },
            Ok(TaskWorkflow::None) | Err(_) => ResolvedStart::nothing(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> Vars<'static> {
        Vars {
            ticket: "PROJ-1",
            title: "Fix it",
            task: "PROJ-1",
            spec_dir: Some("specs/001-x"),
        }
    }

    fn codes(e: &[FieldError]) -> Vec<(&str, &str)> {
        e.iter()
            .map(|e| (e.field.as_str(), e.code.as_str()))
            .collect()
    }

    #[test]
    fn project_workflow_json() {
        let cases = [
            (
                ProjectWorkflow::Single {
                    start: Start::Builtin,
                },
                r#"{"kind":"single","start":{"type":"builtin"}}"#,
            ),
            (
                ProjectWorkflow::Single {
                    start: Start::Message {
                        template: "/fix {ticket}".into(),
                    },
                },
                r#"{"kind":"single","start":{"type":"message","template":"/fix {ticket}"}}"#,
            ),
            (
                ProjectWorkflow::Single {
                    start: Start::Nothing,
                },
                r#"{"kind":"single","start":{"type":"nothing"}}"#,
            ),
            (
                ProjectWorkflow::Phased {
                    plan: "/feature {task}".into(),
                    implement: "/orchestrate {spec_dir}".into(),
                    review: "/review".into(),
                },
                r#"{"kind":"phased","plan":"/feature {task}","implement":"/orchestrate {spec_dir}","review":"/review"}"#,
            ),
        ];
        for (w, json) in cases {
            assert_eq!(serde_json::to_string(&w).unwrap(), json);
            assert_eq!(serde_json::from_str::<ProjectWorkflow>(json).unwrap(), w);
        }
    }

    #[test]
    fn task_workflow_parse() {
        assert_eq!(TaskWorkflow::parse("project"), Ok(TaskWorkflow::Project));
        assert_eq!(TaskWorkflow::parse("feature"), Ok(TaskWorkflow::Project));
        assert_eq!(TaskWorkflow::parse("prompt"), Ok(TaskWorkflow::Prompt));
        assert_eq!(TaskWorkflow::parse("none"), Ok(TaskWorkflow::None));
        assert!(TaskWorkflow::parse("bugfix").is_err());
        assert_eq!(TaskWorkflow::Project.as_str(), "project");
        assert_eq!(TaskWorkflow::Prompt.as_str(), "prompt");
        assert_eq!(TaskWorkflow::None.as_str(), "none");
    }

    #[test]
    fn render_placeholders_and_literals() {
        assert_eq!(
            render("/x {ticket} {title} {task} {spec_dir}", &vars()),
            "/x PROJ-1 Fix it PROJ-1 specs/001-x"
        );
        assert_eq!(render("{{a}} {{{ticket}}}", &vars()), "{a} {PROJ-1}");
        let free = Vars {
            ticket: "LIBRE-1",
            title: "Fix the bug",
            task: "Fix the bug",
            spec_dir: None,
        };
        assert_eq!(render("/f {task}", &free), "/f Fix the bug");
    }

    #[test]
    fn validate_rules() {
        assert_eq!(
            codes(&validate("workflow.plan", "  ", false)),
            [("workflow.plan", "workflow.blank")]
        );
        assert!(validate("f", "/a {ticket} {title} {task} {{ok}}", false).is_empty());
        assert_eq!(
            codes(&validate("f", "{nope}", false)),
            [("f", "workflow.unknown_placeholder")]
        );
        assert_eq!(
            codes(&validate("f", "{spec_dir}", false)),
            [("f", "workflow.placeholder_not_allowed")]
        );
        assert!(validate("f", "{spec_dir}", true).is_empty());
        for bad in ["{ticket", "ticket}", "a } b", "{ {ticket}"] {
            assert_eq!(
                codes(&validate("f", bad, true)),
                [("f", "workflow.unbalanced_brace")],
                "{bad}"
            );
        }
    }

    #[test]
    fn validate_workflow_field_names() {
        let w = ProjectWorkflow::Single {
            start: Start::Message {
                template: "".into(),
            },
        };
        assert_eq!(
            codes(&validate_workflow(&w)),
            [("workflow.start.template", "workflow.blank")]
        );
        let w = ProjectWorkflow::Phased {
            plan: "{spec_dir}".into(),
            implement: "".into(),
            review: "{x}".into(),
        };
        assert_eq!(
            codes(&validate_workflow(&w)),
            [
                ("workflow.plan", "workflow.placeholder_not_allowed"),
                ("workflow.implement", "workflow.blank"),
                ("workflow.review", "workflow.unknown_placeholder"),
            ]
        );
    }

    fn settings(w: Option<ProjectWorkflow>) -> ProjectSettings {
        ProjectSettings {
            workflow: w,
            ..Default::default()
        }
    }

    fn phased() -> ProjectWorkflow {
        ProjectWorkflow::Phased {
            plan: "/feature {task}".into(),
            implement: "/orch {spec_dir}".into(),
            review: "/review".into(),
        }
    }

    fn single(start: Start) -> ProjectWorkflow {
        ProjectWorkflow::Single { start }
    }

    #[test]
    fn is_phased_only_for_project_tasks_of_phased_projects() {
        let p = settings(Some(phased()));
        assert!(is_phased(TaskWorkflow::Project, &p));
        assert!(!is_phased(TaskWorkflow::Prompt, &p));
        assert!(!is_phased(TaskWorkflow::None, &p));
        assert!(!is_phased(TaskWorkflow::Project, &settings(None)));
        let m = settings(Some(single(Start::Nothing)));
        assert!(!is_phased(TaskWorkflow::Project, &m));
    }

    fn task<'a>(workflow: &'a str, msg: Option<&'a str>) -> (&'a str, Option<&'a str>) {
        (workflow, msg)
    }

    fn run(
        req: StartRequest,
        t: &(&str, Option<&str>),
        p: Option<&ProjectSettings>,
    ) -> ResolvedStart {
        resolve(&req, t.0, t.1, p, &vars(), "BUILTIN")
    }

    fn check(r: ResolvedStart, prompt: Option<&str>, phased: bool, wf: &str, msg: Option<&str>) {
        assert_eq!(r.prompt.as_deref(), prompt);
        assert_eq!(r.phased, phased);
        assert_eq!(r.workflow.as_str(), wf);
        assert_eq!(r.start_message.as_deref(), msg);
    }

    #[test]
    fn resolve_request_wins() {
        let ph = settings(Some(phased()));
        let t = task("none", None);
        check(
            run(StartRequest::Prompt("do {ticket}".into()), &t, Some(&ph)),
            Some("do PROJ-1"),
            false,
            "prompt",
            Some("do {ticket}"),
        );
        check(
            run(StartRequest::Nothing, &task("project", None), Some(&ph)),
            None,
            false,
            "none",
            None,
        );
        check(
            run(StartRequest::Project, &t, Some(&ph)),
            Some("/feature PROJ-1"),
            true,
            "project",
            None,
        );
    }

    #[test]
    fn resolve_stored_choice() {
        let ph = settings(Some(phased()));
        check(
            run(
                StartRequest::Inherit,
                &task("prompt", Some("go {title}")),
                Some(&ph),
            ),
            Some("go Fix it"),
            false,
            "prompt",
            Some("go {title}"),
        );
        check(
            run(StartRequest::Inherit, &task("prompt", None), Some(&ph)),
            None,
            false,
            "none",
            None,
        );
        check(
            run(StartRequest::Inherit, &task("none", None), Some(&ph)),
            None,
            false,
            "none",
            None,
        );
        check(
            run(StartRequest::Inherit, &task("feature", None), Some(&ph)),
            Some("/feature PROJ-1"),
            true,
            "project",
            None,
        );
    }

    #[test]
    fn resolve_project_workflow() {
        let t = task("project", None);
        let go = |p: Option<&ProjectSettings>| run(StartRequest::Inherit, &t, p);
        check(go(None), Some("BUILTIN"), false, "project", None);
        check(
            go(Some(&settings(None))),
            Some("BUILTIN"),
            false,
            "project",
            None,
        );
        check(
            go(Some(&settings(Some(single(Start::Builtin))))),
            Some("BUILTIN"),
            false,
            "project",
            None,
        );
        check(
            go(Some(&settings(Some(single(Start::Message {
                template: "/fix {ticket}".into(),
            }))))),
            Some("/fix PROJ-1"),
            false,
            "project",
            None,
        );
        check(
            go(Some(&settings(Some(single(Start::Nothing))))),
            None,
            false,
            "project",
            None,
        );
        check(
            go(Some(&settings(Some(phased())))),
            Some("/feature PROJ-1"),
            true,
            "project",
            None,
        );
    }
}
