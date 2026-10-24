//! A quick, read-only Claude pass that guesses which submodules a ticket touches,
//! so the user starts from a suggestion instead of a blank list.

use serde::{Deserialize, Serialize};

/// A small model is plenty to map a ticket onto a handful of folders.
pub const MODEL: &str = "haiku";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suggestion {
    pub path: String,
    pub reason: String,
}

pub struct Ticket<'a> {
    pub key: &'a str,
    pub title: &'a str,
    pub components: &'a [String],
    pub description: &'a str,
}

/// `language`: the language of the reasons, in English ("Spanish").
pub fn prompt(ticket: &Ticket, submodules: &[String], language: &str) -> String {
    let list: String = submodules.iter().map(|s| format!("- {s}\n")).collect();
    let components = if ticket.components.is_empty() {
        "(none)".into()
    } else {
        ticket.components.join(", ")
    };
    let description = if ticket.description.trim().is_empty() {
        "(no description)"
    } else {
        ticket.description
    };
    format!(
        "You are planning a coding task in a parent repository whose modules are git submodules.\n\
         Decide which submodules will most likely need changes for this ticket. Look at the code \
         (read-only) only as much as you need, and be quick.\n\n\
         Submodules (paths):\n{list}\n\
         Ticket {key}: {title}\n\
         Jira components: {components}\n\
         Description:\n{description}\n\n\
         Answer with only a JSON object and no other text:\n\
         {{\"submodules\": [{{\"path\": \"<one of the paths above>\", \"reason\": \"<one short sentence in {language}>\"}}]}}\n\
         List only the submodules that need changes, most important first.",
        key = ticket.key,
        title = ticket.title,
    )
}

/// Arguments for a headless, read-only run that leaves no saved session behind.
pub fn args(prompt: &str) -> Vec<String> {
    [
        "-p",
        prompt,
        "--model",
        MODEL,
        "--output-format",
        "json",
        "--no-session-persistence",
        "--strict-mcp-config",
        "--tools",
        "Read",
        "Grep",
        "Glob",
        // `--tools` takes several values: end the list with another option.
        "--permission-mode",
        "default",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Reads Claude's `--output-format json` answer, keeping only real submodules, once each.
pub fn parse(output: &str, known: &[String]) -> Result<Vec<Suggestion>, String> {
    let answer = crate::oneshot::answer_json(output)?;
    let mut out: Vec<Suggestion> = Vec::new();
    for item in answer["submodules"].as_array().into_iter().flatten() {
        let Some(path) = item["path"].as_str() else {
            continue;
        };
        if known.iter().any(|k| k == path) && !out.iter().any(|s| s.path == path) {
            out.push(Suggestion {
                path: path.to_string(),
                reason: item["reason"].as_str().unwrap_or("").to_string(),
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn known() -> Vec<String> {
        ["svc-payments", "svc-orders", "svc-billing", "common-lib"]
            .map(String::from)
            .to_vec()
    }

    #[test]
    fn the_prompt_lists_the_submodules_and_the_ticket() {
        let comps = vec!["svc-payments".to_string()];
        let p = prompt(
            &Ticket {
                key: "PROJ-1",
                title: "Cobros duplicados",
                components: &comps,
                description: "Dos cargos.",
            },
            &known(),
            "Spanish",
        );
        assert!(p.contains("- svc-orders\n"));
        assert!(p.contains("Ticket PROJ-1: Cobros duplicados"));
        assert!(p.contains("Jira components: svc-payments"));
        assert!(p.contains("{\"submodules\""));
        assert!(p.contains("one short sentence in Spanish"));
    }

    #[test]
    fn the_run_is_read_only_and_leaves_no_session() {
        let a = args("hola");
        assert!(
            a.windows(4)
                .any(|w| w == ["--tools", "Read", "Grep", "Glob"])
        );
        assert!(a.contains(&"--no-session-persistence".to_string()));
        // The tool list must be closed by another option, not by the end of the line.
        let tools = a.iter().position(|x| x == "--tools").unwrap();
        assert!(a[tools + 4].starts_with("--"));
    }

    #[test]
    fn parses_the_answer_and_drops_unknown_or_repeated_paths() {
        let answer = r#"Aquí va: {"submodules": [
            {"path": "svc-payments", "reason": "Crea los cargos."},
            {"path": "svc-nope", "reason": "?"},
            {"path": "svc-orders", "reason": "Estado del pedido."},
            {"path": "svc-payments", "reason": "otra vez"}]}"#;
        let output = json!({ "type": "result", "is_error": false, "result": answer }).to_string();
        let got = parse(&output, &known()).unwrap();
        assert_eq!(
            got.iter().map(|s| s.path.as_str()).collect::<Vec<_>>(),
            ["svc-payments", "svc-orders"]
        );
        assert_eq!(got[0].reason, "Crea los cargos.");
    }

    #[test]
    fn reports_failures_and_non_json_answers() {
        let failed = json!({ "is_error": true, "result": "usage limit reached" }).to_string();
        assert!(
            parse(&failed, &known())
                .unwrap_err()
                .contains("usage limit")
        );
        let prose = json!({ "is_error": false, "result": "No lo sé" }).to_string();
        assert!(parse(&prose, &known()).unwrap_err().contains("JSON"));
    }
}
