//! Desktop notifications. The daemon sends them itself, so they arrive with every
//! client closed.

use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub title: String,
    pub body: String,
}

#[derive(Clone)]
pub enum Notifier {
    /// macOS Notification Center, through `osascript`.
    MacOs,
    /// Keeps them in memory instead (tests, or `DELILAH_NOTIFY=0`).
    Recorder(Arc<Mutex<Vec<Notification>>>),
}

impl Notifier {
    pub fn send(&self, title: impl Into<String>, body: impl Into<String>) {
        let n = Notification {
            title: title.into(),
            body: body.into(),
        };
        match self {
            Self::MacOs => {
                let script = format!(
                    "display notification \"{}\" with title \"Delilah\" subtitle \"{}\"",
                    applescript_escape(&n.body),
                    applescript_escape(&n.title)
                );
                // Fire and forget: a notification that fails must not affect the task.
                if let Err(e) = tokio::process::Command::new("osascript")
                    .args(["-e", &script])
                    .spawn()
                {
                    tracing::warn!("cannot show notification: {e}");
                }
            }
            Self::Recorder(sent) => sent.lock().unwrap().push(n),
        }
    }

    pub fn recorded(&self) -> Vec<Notification> {
        match self {
            Self::MacOs => Vec::new(),
            Self::Recorder(sent) => sent.lock().unwrap().clone(),
        }
    }
}

fn applescript_escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_quotes_for_applescript() {
        assert_eq!(
            applescript_escape(r#"say "hola" \ adios"#),
            r#"say \"hola\" \\ adios"#
        );
    }
}
