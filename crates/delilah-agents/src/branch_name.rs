//! A quick, tool-less Claude pass that names a task's git branch from its title
//! (and notes), so the branch says what the work is instead of just a ticket key.

/// A small model is plenty to summarise a task in a few words.
pub const MODEL: &str = "haiku";

/// Notes beyond this many characters are cut: the name only needs the gist.
const MAX_NOTES_CHARS: usize = 1000;

pub fn prompt(title: &str, notes: Option<&str>) -> String {
    let notes = notes
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(|n| {
            format!(
                "Notes:\n{}\n\n",
                n.chars().take(MAX_NOTES_CHARS).collect::<String>()
            )
        })
        .unwrap_or_default();
    format!(
        "Name the git branch for this task: 2 to 4 lowercase English words describing what it does. \
         Translate the title if it is in another language. Do not include a ticket key or a \
         prefix such as `feature/`.\n\n\
         Title: {title}\n\n\
         {notes}\
         Answer with only a JSON object and no other text:\n\
         {{\"name\": \"<2-4 English words>\"}}"
    )
}

/// Arguments for a headless run with no tools that leaves no saved session behind.
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
        // No tools at all: an empty list, closed by another option.
        "--tools",
        "",
        "--setting-sources",
        "user",
        // User-level hooks would still run on every naming call.
        "--settings",
        "{\"disableAllHooks\":true}",
        "--disable-slash-commands",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Reads Claude's `--output-format json` answer and returns the trimmed `name`; the caller
/// normalises it into a branch name.
pub fn parse(output: &str) -> Result<String, String> {
    let answer = crate::oneshot::answer_json(output)?;
    let name = answer["name"].as_str().map(str::trim).unwrap_or("");
    if name.is_empty() {
        return Err("claude gave no branch name".into());
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn envelope(result: &str) -> String {
        json!({ "type": "result", "is_error": false, "result": result }).to_string()
    }

    #[test]
    fn the_prompt_has_the_title_and_the_notes_when_given() {
        let p = prompt("Cobros duplicados", Some("Dos cargos al reintentar."));
        assert!(p.contains("Cobros duplicados"));
        assert!(p.contains("Dos cargos al reintentar."));
        assert!(p.contains("{\"name\""));
        assert!(p.contains("2 to 4"));
        assert!(p.contains("English"));
        let without = prompt("Cobros duplicados", Some("  "));
        assert!(!without.contains("Notes:"));
        assert!(!prompt("x", None).contains("Notes:"));
    }

    #[test]
    fn the_run_has_no_tools_and_leaves_no_session() {
        let a = args("hola");
        for flag in [
            "-p",
            "--no-session-persistence",
            "--strict-mcp-config",
            "--setting-sources",
            "--disable-slash-commands",
        ] {
            assert!(a.contains(&flag.to_string()), "{flag}");
        }
        assert!(a.windows(2).any(|w| w == ["--model", "haiku"]));
        assert!(a.windows(2).any(|w| w == ["--output-format", "json"]));
        assert!(a.windows(2).any(|w| w == ["--setting-sources", "user"]));
        assert!(
            a.windows(2)
                .any(|w| w == ["--settings", "{\"disableAllHooks\":true}"])
        );
        let tools = a.iter().position(|x| x == "--tools").unwrap();
        assert_eq!(a[tools + 1], "");
        assert!(a[tools + 2].starts_with("--"));
    }

    #[test]
    fn parses_the_name_from_json_inside_prose() {
        let out = envelope("Claro: {\"name\": \" fix duplicate charges \"} listo");
        assert_eq!(parse(&out).unwrap(), "fix duplicate charges");
    }

    #[test]
    fn reports_failures_prose_and_empty_names() {
        let failed = json!({ "is_error": true, "result": "usage limit reached" }).to_string();
        assert!(parse(&failed).unwrap_err().contains("usage limit"));
        assert!(parse(&envelope("No lo sé")).unwrap_err().contains("JSON"));
        assert!(parse(&envelope("{\"name\": \"  \"}")).is_err());
        assert!(parse(&envelope("{\"other\": 1}")).is_err());
    }
}
