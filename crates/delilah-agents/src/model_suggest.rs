//! A model suggestion for a new task: a local heuristic over the ticket, then a quick,
//! tool-less Haiku pass that may refine it. The user confirms what comes out.

use delilah_core::AgentModel;

/// A small model is plenty to size a ticket.
pub const MODEL: &str = "haiku";

/// Description beyond this many characters is cut: the size is what matters.
const MAX_DESCRIPTION_CHARS: usize = 3000;

pub struct Ticket<'a> {
    pub title: &'a str,
    pub description: &'a str,
    /// Jira issue type (`Bug`, `Story`…); empty when unknown.
    pub issue_type: &'a str,
    pub labels: &'a [String],
    pub workflow: &'a str,
}

/// Words in the type or labels that say how big the thinking is.
const SMALL: &[&str] = &[
    "chore",
    "docs",
    "doc",
    "typo",
    "copy",
    "trivial",
    "i18n",
    "translation",
];
const MEDIUM: &[&str] = &["bug", "fix", "bugfix", "story", "feature", "task"];
const LARGE: &[&str] = &[
    "epic",
    "spike",
    "research",
    "architecture",
    "design",
    "refactor",
    "migration",
    "security",
    "performance",
];

/// The local guess: the ticket's type and labels, its description's length and its
/// workflow, scored into a model with a one-line reason (in English; the caller may
/// translate it). Small tasks go to Haiku, ordinary ones to Sonnet, big ones to
/// `opusplan` (Opus thinks, Sonnet types), and only very big ones to Opus.
pub fn heuristic(ticket: &Ticket) -> (AgentModel, String) {
    let words: Vec<String> = std::iter::once(ticket.issue_type)
        .chain(ticket.labels.iter().map(String::as_str))
        .map(|w| w.trim().to_lowercase())
        .filter(|w| !w.is_empty())
        .collect();
    let has = |set: &[&str]| words.iter().any(|w| set.contains(&w.as_str()));
    let mut score = 0i32;
    let mut why = Vec::new();
    if has(LARGE) {
        score += 3;
        why.push("its type or labels call for design work");
    } else if has(SMALL) {
        score -= 2;
        why.push("its type or labels say it is small");
    } else if has(MEDIUM) {
        score += 1;
    }
    let length = ticket.description.trim().chars().count();
    match length {
        0..200 => {
            score -= 1;
            why.push("its description is short");
        }
        200..1500 => {}
        1500..4000 => {
            score += 1;
            why.push("its description is long");
        }
        _ => {
            score += 2;
            why.push("its description is very long");
        }
    }
    if matches!(ticket.workflow, "project" | "feature") {
        score += 1;
        why.push("it goes through the spec workflow");
    }
    let model = match score {
        ..=-2 => AgentModel::Haiku,
        -1..=1 => AgentModel::Sonnet,
        2..=3 => AgentModel::Opusplan,
        _ => AgentModel::Opus,
    };
    let reason = match why.is_empty() {
        true => "an ordinary task".to_string(),
        false => why.join(", "),
    };
    (model, reason)
}

/// `guess`: the heuristic's answer, as a starting point. `language`: of the reason
/// ("Spanish").
pub fn prompt(ticket: &Ticket, guess: AgentModel, language: &str) -> String {
    let description: String = ticket
        .description
        .trim()
        .chars()
        .take(MAX_DESCRIPTION_CHARS)
        .collect();
    let description = match description.is_empty() {
        true => "(no description)".to_string(),
        false => description,
    };
    let labels = match ticket.labels.is_empty() {
        true => "(none)".to_string(),
        false => ticket.labels.join(", "),
    };
    format!(
        "Pick the Claude model a coding agent should use for this task. Options:\n\
         - haiku: trivial changes (typos, copy, small config).\n\
         - sonnet: ordinary features and bug fixes.\n\
         - opusplan: needs real design first, then routine implementation.\n\
         - opus: hard, open-ended or risky work all the way through.\n\
         Prefer the cheapest model that will do the job well. A quick heuristic guessed \
         `{guess}`.\n\n\
         Title: {title}\n\
         Type: {issue_type}\n\
         Labels: {labels}\n\
         Workflow: {workflow}\n\
         Description:\n{description}\n\n\
         Answer with only a JSON object and no other text:\n\
         {{\"model\": \"<haiku|sonnet|opusplan|opus>\", \"reason\": \"<one short sentence in {language}>\"}}",
        guess = guess.name(),
        title = ticket.title,
        issue_type = if ticket.issue_type.is_empty() {
            "(unknown)"
        } else {
            ticket.issue_type
        },
        workflow = ticket.workflow,
    )
}

/// Arguments for a headless run with no tools that leaves no saved session behind.
pub fn args(prompt: &str) -> Vec<String> {
    crate::branch_name::args(prompt)
}

/// Reads Claude's `--output-format json` answer. `default` is not a suggestion.
pub fn parse(output: &str) -> Result<(AgentModel, String), String> {
    let answer = crate::oneshot::answer_json(output)?;
    let model = answer["model"]
        .as_str()
        .and_then(AgentModel::parse)
        .filter(|m| *m != AgentModel::Default)
        .ok_or_else(|| format!("claude suggested no known model: {}", answer["model"]))?;
    let reason = answer["reason"].as_str().unwrap_or("").trim().to_string();
    Ok((model, reason))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn ticket<'a>(issue_type: &'a str, labels: &'a [String], description: &'a str) -> Ticket<'a> {
        Ticket {
            title: "Algo",
            description,
            issue_type,
            labels,
            workflow: "plain",
        }
    }

    #[test]
    fn small_tickets_go_to_haiku_and_big_ones_to_opus() {
        let none: Vec<String> = Vec::new();
        assert_eq!(
            heuristic(&ticket("Chore", &none, "typo")).0,
            AgentModel::Haiku
        );
        assert_eq!(
            heuristic(&ticket("Bug", &none, &"x".repeat(500))).0,
            AgentModel::Sonnet
        );
        let spike = vec!["spike".to_string()];
        assert_eq!(
            heuristic(&ticket("", &spike, &"x".repeat(500))).0,
            AgentModel::Opusplan
        );
        let long = "x".repeat(5000);
        let mut big = ticket("Epic", &none, &long);
        big.workflow = "feature";
        let (model, reason) = heuristic(&big);
        assert_eq!(model, AgentModel::Opus);
        assert!(reason.contains("spec workflow"), "{reason}");
        // Nothing to go on: an ordinary task.
        assert_eq!(
            heuristic(&ticket("", &none, &"x".repeat(500))).0,
            AgentModel::Sonnet
        );
    }

    #[test]
    fn the_prompt_has_the_ticket_and_the_guess() {
        let labels = vec!["backend".to_string()];
        let p = prompt(
            &ticket("Story", &labels, "Hacer X"),
            AgentModel::Sonnet,
            "Spanish",
        );
        assert!(p.contains("Type: Story"));
        assert!(p.contains("Labels: backend"));
        assert!(p.contains("guessed `sonnet`"));
        assert!(p.contains("one short sentence in Spanish"));
        assert!(args(&p).windows(2).any(|w| w == ["--model", MODEL]));
    }

    #[test]
    fn parses_the_answer_and_refuses_unknown_models() {
        let ok =
            json!({"is_error": false, "result": r#"{"model": "opusplan", "reason": "Diseño."}"#});
        assert_eq!(
            parse(&ok.to_string()).unwrap(),
            (AgentModel::Opusplan, "Diseño.".to_string())
        );
        for bad in ["gpt-5", "default"] {
            let answer = json!({"is_error": false, "result": format!(r#"{{"model": "{bad}"}}"#)});
            assert!(parse(&answer.to_string()).is_err(), "{bad}");
        }
    }
}
