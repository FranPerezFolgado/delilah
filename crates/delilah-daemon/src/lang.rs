//! The language Delilah speaks in: the one picked in the app's Settings (`language`
//! preference: "auto", "es" or "en"), "auto" following the Mac's. It covers what the
//! daemon says by itself (notifications, inbox texts, the ticket handed to the agent)
//! and what it writes to Claude, so the agent answers in that language too.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use delilah_core::workflow::ProjectWorkflow;

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

/// The "Spec Kit (`/feature`)" preset: what projects had before commands were
/// configurable. The review text is the one of `handoff::step_prompt`, and the
/// JSON in migration 0022 must stay equal to this (a test checks it).
pub fn feature_preset(lang: Lang) -> ProjectWorkflow {
    ProjectWorkflow::Phased {
        plan: "/feature {task}".to_string(),
        implement: "/orchestrate {spec_dir}".to_string(),
        review: lang
            .pick(
                "/review de la rama de esta tarea. Después aplica los hallazgos confirmados, actualiza la documentación que hayan dejado obsoleta y haz commit. No hagas push ni abras una PR.",
                "/review the branch of this task. Then apply the confirmed findings, update the docs they made stale and commit. Do not push or open a PR.",
            )
            .to_string(),
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
            // Same text as handoff::step_prompt's Review arm.
            assert!(review.starts_with(lang.pick("/review de la rama", "/review the branch")));
            assert!(review.ends_with(lang.pick("abras una PR.", "open a PR.")));
        }
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
}
