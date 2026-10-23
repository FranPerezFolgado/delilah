//! The review's outcome file, written by an agent and so read with care.

use std::path::Path;

use crate::lang::Lang;

/// What the review reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    Ready {
        pr_title: Option<String>,
        pr_body: Option<String>,
    },
    Blocked {
        reason: String,
    },
}

#[derive(serde::Deserialize)]
struct OutcomeFile {
    outcome: String,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    pr_title: Option<String>,
    #[serde(default)]
    pr_body: Option<String>,
}

/// The biggest outcome file read, and the longest title and body of a PR taken from it
/// (GitHub's body limit is 65536 characters).
const MAX_OUTCOME_BYTES: u64 = 256 * 1024;
const MAX_TITLE_CHARS: usize = 256;
const MAX_BODY_CHARS: usize = 59_000;

/// A title on one line: no control characters, cut to `MAX_TITLE_CHARS`.
fn clean_title(title: String) -> Option<String> {
    let line: String = title
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_TITLE_CHARS)
        .collect();
    non_empty(Some(line.trim().to_string()))
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|s| !s.trim().is_empty())
}

/// The outcome in `path`. A missing or invalid file is `Blocked`.
pub(crate) fn read_outcome(lang: Lang, path: &Path) -> Outcome {
    let missing = || Outcome::Blocked {
        reason: lang
            .pick(
                "la revisión no informó de un resultado",
                "the review did not report an outcome",
            )
            .to_string(),
    };
    // Written by an agent: only a regular file of a sane size is read (not a symlink
    // to somewhere else, not a device or a huge file).
    let regular = std::fs::symlink_metadata(path)
        .is_ok_and(|m| m.file_type().is_file() && m.len() <= MAX_OUTCOME_BYTES);
    let Some(file) = regular
        .then(|| std::fs::read_to_string(path).ok())
        .flatten()
        .and_then(|text| serde_json::from_str::<OutcomeFile>(&text).ok())
    else {
        return missing();
    };
    match file.outcome.as_str() {
        "ready" => Outcome::Ready {
            pr_title: non_empty(file.pr_title).and_then(clean_title),
            pr_body: non_empty(file.pr_body).map(|b| b.chars().take(MAX_BODY_CHARS).collect()),
        },
        "blocked" => Outcome::Blocked {
            reason: non_empty(file.reason).unwrap_or_else(|| {
                lang.pick(
                    "la revisión quedó bloqueada",
                    "the review reported it is blocked",
                )
                .to_string()
            }),
        },
        _ => missing(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::lang::Lang;

    use super::*;

    fn file_with(content: Option<&str>) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("delilah-outcome-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("review-s1.json");
        if let Some(content) = content {
            std::fs::write(&path, content).unwrap();
        }
        (dir, path)
    }

    #[test]
    fn the_outcome_is_read_with_optional_fields() {
        let (_d, p) = file_with(Some(
            r#"{"outcome":"ready","pr_title":"feat: x","pr_body":"why"}"#,
        ));
        assert_eq!(
            read_outcome(Lang::En, &p),
            Outcome::Ready {
                pr_title: Some("feat: x".into()),
                pr_body: Some("why".into())
            }
        );
        let (_d, p) = file_with(Some(r#"{"outcome":"ready"}"#));
        assert_eq!(
            read_outcome(Lang::En, &p),
            Outcome::Ready {
                pr_title: None,
                pr_body: None
            }
        );
        let (_d, p) = file_with(Some(r#"{"outcome":"blocked","reason":"tests fail"}"#));
        assert_eq!(
            read_outcome(Lang::En, &p),
            Outcome::Blocked {
                reason: "tests fail".into()
            }
        );
    }

    #[test]
    fn a_missing_or_invalid_outcome_is_blocked_in_the_app_language() {
        for content in [None, Some("not json"), Some(r#"{"outcome":"maybe"}"#)] {
            let (_d, p) = file_with(content);
            assert_eq!(
                read_outcome(Lang::En, &p),
                Outcome::Blocked {
                    reason: "the review did not report an outcome".into()
                }
            );
            let Outcome::Blocked { reason } = read_outcome(Lang::Es, &p) else {
                panic!("not blocked");
            };
            assert!(reason.contains("la revisión no informó"));
        }
    }

    #[test]
    fn an_outcome_that_is_not_a_regular_file_or_is_too_big_is_blocked() {
        let blocked = Outcome::Blocked {
            reason: "the review did not report an outcome".into(),
        };
        // A symlink, even to a good file.
        let (d, good) = file_with(Some(r#"{"outcome":"ready"}"#));
        let link = d.join("link.json");
        std::os::unix::fs::symlink(&good, &link).unwrap();
        assert_eq!(read_outcome(Lang::En, &link), blocked);
        // A directory.
        assert_eq!(read_outcome(Lang::En, &d), blocked);
        // Too big, though valid JSON.
        let big = format!(
            r#"{{"outcome":"ready","pr_body":"{}"}}"#,
            "x".repeat(MAX_OUTCOME_BYTES as usize)
        );
        let (_d, p) = file_with(Some(&big));
        assert_eq!(read_outcome(Lang::En, &p), blocked);
    }

    #[test]
    fn the_title_is_one_short_line_and_the_body_is_capped() {
        let long = "t".repeat(1000);
        let json = serde_json::json!({
            "outcome": "ready",
            "pr_title": format!("feat:\n x\r\u{7}y\t{long}"),
            "pr_body": "b".repeat(70_000),
        });
        let (_d, p) = file_with(Some(&json.to_string()));
        let Outcome::Ready { pr_title, pr_body } = read_outcome(Lang::En, &p) else {
            panic!("not ready");
        };
        let title = pr_title.unwrap();
        assert!(title.starts_with("feat: xy"), "{title}");
        assert!(!title.chars().any(char::is_control));
        assert_eq!(title.chars().count(), MAX_TITLE_CHARS);
        assert_eq!(pr_body.unwrap().chars().count(), MAX_BODY_CHARS);
        // A title of only control characters is no title.
        let (_d, p) = file_with(Some(r#"{"outcome":"ready","pr_title":"\n\t"}"#));
        assert_eq!(
            read_outcome(Lang::En, &p),
            Outcome::Ready {
                pr_title: None,
                pr_body: None
            }
        );
    }
}
