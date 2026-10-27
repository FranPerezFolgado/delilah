//! The language Delilah speaks in: the one picked in the app's Settings (`language`
//! preference: "auto", "es" or "en"), "auto" following the Mac's. It covers what the
//! daemon says by itself (notifications, inbox texts, the ticket handed to the agent)
//! and what it writes to Claude, so the agent answers in that language too.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use delilah_core::hierarchy::Refusal;
use delilah_core::workflow::ProjectWorkflow;
use serde_json::{Map, Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Es,
    En,
}

impl Lang {
    /// `es` or `en`, whichever this language is.
    pub fn pick<'a>(self, es: &'a str, en: &'a str) -> &'a str {
        match self {
            Lang::Es => es,
            Lang::En => en,
        }
    }

    /// The language's name in English, for prompts written in English.
    pub fn name(self) -> &'static str {
        self.pick("Spanish", "English")
    }
}

/// The built-in start prompt (a template: `{ticket}`, `{title}`), without slash commands.
pub fn builtin_start(lang: Lang) -> &'static str {
    match lang {
        Lang::Es => "Trabaja en la tarea {ticket}: {title}.",
        Lang::En => "Work on the task {ticket}: {title}.",
    }
}

/// The review step's prompt in the "Spec Kit (`/feature`)" preset (the one place
/// that names `/review`).
pub fn review_text(lang: Lang) -> &'static str {
    lang.pick(
        "/review de la rama de esta tarea. Después aplica los hallazgos confirmados, actualiza la documentación que hayan dejado obsoleta y haz commit. No hagas push ni abras una PR.",
        "/review the branch of this task. Then apply the confirmed findings, update the docs they made stale and commit. Do not push or open a PR.",
    )
}

/// The "Spec Kit (`/feature`)" preset: what projects had before commands were
/// configurable. The review text is [`review_text`], and the
/// JSON in migration 0024 must stay equal to this (a test checks it).
pub fn feature_preset(lang: Lang) -> ProjectWorkflow {
    ProjectWorkflow::Phased {
        plan: "/feature {task}".to_string(),
        implement: "/orchestrate {spec_dir}".to_string(),
        review: review_text(lang).to_string(),
    }
}

/// The Mac's language: Spanish if its first choice is, English otherwise or when it
/// cannot be read. `LANG` is not consulted: the contract's system language on macOS is
/// `AppleLanguages`, and the daemon's `LANG` (often `C` or unset under launchd) says
/// nothing about what the user picked.
pub fn system() -> Lang {
    let out = std::process::Command::new("defaults")
        .args(["read", "-g", "AppleLanguages"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    resolve(None, &out)
}

/// From the `language` preference's value.
pub fn from_setting(value: Option<&str>) -> Lang {
    match value {
        Some("es") => Lang::Es,
        Some("en") => Lang::En,
        _ => system(),
    }
}

/// The first entry of `defaults read -g AppleLanguages` output, e.g. `(\n "es-ES",\n "en-US"\n)`
/// (entries may be unquoted), lowercased; empty when there is none.
fn first_language(apple_languages: &str) -> String {
    let trim = |c: char| c == '(' || c == ')' || c == '"' || c == ',' || c.is_whitespace();
    let inner = apple_languages.trim_start_matches(trim);
    let end = inner.find(|c: char| c == '"' || c == ',' || c == ')' || c.is_whitespace());
    inner[..end.unwrap_or(inner.len())].to_lowercase()
}

/// The language rule (contracts/language.md): "es"/"en" are explicit; anything else
/// follows the first AppleLanguages entry, `es*` being Spanish and anything else
/// (empty, garbage) English.
fn resolve(pref: Option<&str>, apple_languages: &str) -> Lang {
    match pref {
        Some("en") => Lang::En,
        Some("es") => Lang::Es,
        _ if first_language(apple_languages).starts_with("es") => Lang::Es,
        _ => Lang::En,
    }
}

/// The daemon's current language, shared and cheap to read.
#[derive(Clone)]
pub struct Current(Arc<AtomicBool>);

impl Default for Current {
    fn default() -> Self {
        Self(Arc::new(AtomicBool::new(true)))
    }
}

impl Current {
    pub fn get(&self) -> Lang {
        if self.0.load(Ordering::Relaxed) {
            Lang::En
        } else {
            Lang::Es
        }
    }

    pub fn set(&self, lang: Lang) {
        self.0.store(lang == Lang::En, Ordering::Relaxed);
    }
}

/// The reason a refusal goes by in the 409 body (`error`), as the contract names it.
pub fn refusal_reason(r: Refusal) -> &'static str {
    match r {
        Refusal::NotAParentTask => "not-a-parent-task",
        Refusal::ParentTaskArchived => "parent-task-archived",
        Refusal::Nesting => "nesting",
        Refusal::ProjectMismatch => "project-mismatch",
        Refusal::ParentTaskNotStarted => "parent-task-not-started",
        Refusal::ParentTaskBranchMissing => "parent-task-branch-missing",
        Refusal::HasOpenPr => "has-open-pr",
        Refusal::HasSession => "has-session",
        Refusal::InCheckout => "in-checkout",
        Refusal::AlreadyStarted => "already-started",
        Refusal::LiveSubtasks => "live-subtasks",
        Refusal::NoCommits => "no-commits",
        Refusal::EmptyCommitNotAllowed => "empty-commit-not-allowed",
    }
}

/// The localised message of a refusal. `detail` is the object that goes in the body
/// (`key`, `project`, `branch`, `subtasks`); what it names is woven into the text.
pub fn refusal_message(lang: Lang, r: Refusal, detail: &Value) -> String {
    let s = |field: &str| detail.get(field).and_then(Value::as_str).unwrap_or("?").to_string();
    match r {
        Refusal::NotAParentTask => {
            let k = s("key");
            lang.pick(
                &format!("{k} no es una tarea padre."),
                &format!("{k} is not a parent task."),
            )
            .to_string()
        }
        Refusal::ParentTaskArchived => {
            let k = s("key");
            lang.pick(
                &format!("La tarea padre {k} está archivada."),
                &format!("The parent task {k} is archived."),
            )
            .to_string()
        }
        Refusal::Nesting => lang
            .pick(
                "Las subtareas no se anidan: una tarea padre no puede ser subtarea ni al revés.",
                "Subtasks do not nest: a parent task cannot be a subtask, nor the other way round.",
            )
            .to_string(),
        Refusal::ProjectMismatch => {
            let p = s("project");
            lang.pick(
                &format!("El proyecto {p} no es el de la tarea padre."),
                &format!("Project {p} is not the parent task's project."),
            )
            .to_string()
        }
        Refusal::ParentTaskNotStarted => {
            let k = s("key");
            lang.pick(
                &format!("Primero inicia la tarea padre {k}: aún no tiene workspace."),
                &format!("Start the parent task {k} first: it has no workspace yet."),
            )
            .to_string()
        }
        Refusal::ParentTaskBranchMissing => {
            let b = s("branch");
            lang.pick(
                &format!("La rama {b} de la tarea padre no está en el remoto; súbela primero."),
                &format!("The parent task's branch {b} is not on the remote; push it first."),
            )
            .to_string()
        }
        Refusal::HasOpenPr => lang
            .pick(
                "La tarea tiene una PR abierta; ciérrala o fusiónala antes de convertirla en tarea padre.",
                "The task has an open PR; close or merge it before making it a parent task.",
            )
            .to_string(),
        Refusal::HasSession => lang
            .pick(
                "La tarea ya tiene sesiones; solo una tarea sin empezar puede ser tarea padre.",
                "The task already has sessions; only a task never run can become a parent task.",
            )
            .to_string(),
        Refusal::InCheckout => lang
            .pick(
                "La tarea está en tu checkout; devuélvela antes de cambiarla de tipo.",
                "The task is in your checkout; put it back before changing its kind.",
            )
            .to_string(),
        Refusal::AlreadyStarted => lang
            .pick(
                "La tarea ya se ha ejecutado; solo una tarea sin empezar puede adjuntarse a una tarea padre.",
                "The task has already run; only a never-started task can be attached to a parent task.",
            )
            .to_string(),
        Refusal::LiveSubtasks => {
            let list = detail
                .get("subtasks")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", "))
                .unwrap_or_default();
            lang.pick(
                &format!("La tarea padre tiene subtareas vivas: {list}."),
                &format!("The parent task has live subtasks: {list}."),
            )
            .to_string()
        }
        Refusal::NoCommits => lang
            .pick(
                "La rama no tiene commits que proponer.",
                "The branch has no commits to propose.",
            )
            .to_string(),
        Refusal::EmptyCommitNotAllowed => lang
            .pick(
                "Un commit vacío solo se admite en una tarea padre.",
                "An empty commit is only allowed on a parent task.",
            )
            .to_string(),
    }
}

/// The 409 body of a refusal: `{ error, message, ...detail }`.
pub fn refusal_body(lang: Lang, r: Refusal, detail: Value) -> Value {
    let message = refusal_message(lang, r, &detail);
    let mut body = Map::new();
    body.insert("error".into(), json!(refusal_reason(r)));
    body.insert("message".into(), json!(message));
    if let Value::Object(d) = detail {
        for (k, v) in d {
            body.entry(k).or_insert(v);
        }
    }
    Value::Object(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_start_names_the_task_and_has_no_command() {
        for lang in [Lang::Es, Lang::En] {
            let t = builtin_start(lang);
            assert!(t.contains("{ticket}") && t.contains("{title}"));
            assert!(!t.contains('/'));
            assert!(delilah_core::workflow::validate("f", t, false).is_empty());
        }
        assert!(builtin_start(Lang::Es).starts_with("Trabaja"));
        assert!(builtin_start(Lang::En).starts_with("Work"));
    }

    #[test]
    fn feature_preset_is_valid_and_keeps_todays_review_text() {
        for lang in [Lang::Es, Lang::En] {
            let w = feature_preset(lang);
            assert!(delilah_core::workflow::validate_workflow(&w).is_empty());
            let ProjectWorkflow::Phased {
                plan,
                implement,
                review,
            } = w
            else {
                panic!("phased")
            };
            assert_eq!(plan, "/feature {task}");
            assert_eq!(implement, "/orchestrate {spec_dir}");
            // Same text as `review_text`.
            assert!(review.starts_with(lang.pick("/review de la rama", "/review the branch")));
            assert!(review.ends_with(lang.pick("abras una PR.", "open a PR.")));
        }
    }

    /// The app's presets (app/src/workflowPresets.ts) carry their own copy of the
    /// `/feature` preset; this keeps the two from drifting.
    #[test]
    fn the_apps_feature_preset_matches() {
        let ts = include_str!("../../../app/src/workflowPresets.ts");
        for lang in [Lang::Es, Lang::En] {
            assert!(ts.contains(review_text(lang)), "review text ({lang:?})");
        }
        let ProjectWorkflow::Phased {
            plan, implement, ..
        } = feature_preset(Lang::En)
        else {
            unreachable!()
        };
        assert!(ts.contains(&format!("plan: \"{plan}\"")));
        assert!(ts.contains(&format!("implement: \"{implement}\"")));
    }

    #[test]
    fn the_default_is_english() {
        assert_eq!(Current::default().get(), Lang::En);
    }

    // The table of contracts/language.md, mirrored in app/src-tauri/src/daemon.rs and
    // app/src/i18n.test.ts.
    fn apple(first: &str) -> String {
        format!("(\n    \"{first}\",\n    \"en-US\"\n)")
    }

    #[test]
    fn explicit_preference_wins() {
        for langs in [
            apple("es-ES"),
            apple("en-US"),
            String::new(),
            "garbage".into(),
        ] {
            assert_eq!(resolve(Some("en"), &langs), Lang::En);
            assert_eq!(resolve(Some("es"), &langs), Lang::Es);
        }
    }

    #[test]
    fn other_preferences_follow_the_system() {
        for pref in [None, Some("auto"), Some("fr")] {
            for (langs, want) in [
                (apple("es-ES"), Lang::Es),
                (apple("es-419"), Lang::Es),
                (apple("ES"), Lang::Es),
                (apple("en-US"), Lang::En),
                (apple("fr-FR"), Lang::En),
                (apple("ja-JP"), Lang::En),
                ("(\n    es,\n    \"en-US\"\n)".to_string(), Lang::Es),
                ("(\n    en,\n    \"es-ES\"\n)".to_string(), Lang::En),
                ("es-ES".to_string(), Lang::Es),
                (String::new(), Lang::En),
                ("not a plist".to_string(), Lang::En),
            ] {
                assert_eq!(resolve(pref, &langs), want, "{pref:?} {langs:?}");
            }
        }
    }

    #[test]
    fn an_explicit_choice_wins_over_the_system() {
        assert_eq!(from_setting(Some("en")), Lang::En);
        assert_eq!(from_setting(Some("es")), Lang::Es);
        assert_eq!(Lang::En.pick("hola", "hello"), "hello");
    }

    const ALL_REFUSALS: [Refusal; 13] = [
        Refusal::NotAParentTask,
        Refusal::ParentTaskArchived,
        Refusal::Nesting,
        Refusal::ProjectMismatch,
        Refusal::ParentTaskNotStarted,
        Refusal::ParentTaskBranchMissing,
        Refusal::HasOpenPr,
        Refusal::HasSession,
        Refusal::InCheckout,
        Refusal::AlreadyStarted,
        Refusal::LiveSubtasks,
        Refusal::NoCommits,
        Refusal::EmptyCommitNotAllowed,
    ];

    #[test]
    fn every_refusal_has_its_reason_and_both_messages() {
        for r in ALL_REFUSALS {
            // The reason is the serde (kebab-case) name of the variant.
            assert_eq!(json!(refusal_reason(r)), serde_json::to_value(r).unwrap());
            let es = refusal_message(Lang::Es, r, &json!({}));
            let en = refusal_message(Lang::En, r, &json!({}));
            assert!(!es.is_empty() && !en.is_empty() && es != en, "{r:?}");
        }
    }

    #[test]
    fn refusal_body_carries_error_message_and_detail() {
        let body = refusal_body(
            Lang::En,
            Refusal::LiveSubtasks,
            json!({ "subtasks": ["P-2", "P-3"] }),
        );
        assert_eq!(body["error"], "live-subtasks");
        assert_eq!(body["subtasks"], json!(["P-2", "P-3"]));
        assert!(body["message"].as_str().unwrap().contains("P-2, P-3"));
        let body = refusal_body(Lang::Es, Refusal::NotAParentTask, json!({ "key": "P-9" }));
        assert_eq!(body["key"], "P-9");
        assert!(body["message"].as_str().unwrap().contains("P-9"));
        let body = refusal_body(Lang::En, Refusal::Nesting, Value::Null);
        assert_eq!(body.as_object().unwrap().len(), 2);
    }
}
