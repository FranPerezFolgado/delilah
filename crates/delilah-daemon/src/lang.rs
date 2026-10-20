//! The language Delilah speaks in: the one picked in the app's Settings (`language`
//! preference: "auto", "es" or "en"), "auto" following the Mac's. It covers what the
//! daemon says by itself (notifications, inbox texts, the ticket handed to the agent)
//! and what it writes to Claude, so the agent answers in that language too.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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

/// The Mac's language: Spanish if its first choice is, English otherwise.
pub fn system() -> Lang {
    let preferred = std::process::Command::new("defaults")
        .args(["read", "-g", "AppleLanguages"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase())
        .and_then(|out| out.split('"').nth(1).map(str::to_string))
        .or_else(|| std::env::var("LANG").ok().map(|l| l.to_lowercase()));
    match preferred {
        Some(code) if code.starts_with("es") => Lang::Es,
        Some(_) => Lang::En,
        None => Lang::Es,
    }
}

/// From the `language` preference's value.
pub fn from_setting(value: Option<&str>) -> Lang {
    match value {
        Some("es") => Lang::Es,
        Some("en") => Lang::En,
        _ => system(),
    }
}

/// The daemon's current language, shared and cheap to read.
#[derive(Clone)]
pub struct Current(Arc<AtomicBool>);

impl Default for Current {
    fn default() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
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
    fn an_explicit_choice_wins_over_the_system() {
        assert_eq!(from_setting(Some("en")), Lang::En);
        assert_eq!(from_setting(Some("es")), Lang::Es);
        assert_eq!(Lang::En.pick("hola", "hello"), "hello");
    }
}
