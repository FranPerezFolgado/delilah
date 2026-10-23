//! Questions asked while autopilot runs: answered with the recommended option.

use delilah_core::{Event, TaskKey};
use serde_json::{Map, Value};

use super::{is_running, is_task, pause};
use crate::state::AppState;

/// Whether an option's label is marked "(Recommended)" / "(Recomendado)" (any case).
fn is_marked(label: &str) -> bool {
    let l = label.trim().to_lowercase();
    l.ends_with("(recommended)") || l.ends_with("(recomendado)")
}

/// The labels of a question's options marked as recommended.
fn marked(question: &Value) -> impl Iterator<Item = &str> {
    question["options"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|o| o["label"].as_str())
        .filter(|l| is_marked(l))
}

/// The answers autopilot gives to an AskUserQuestion: for every question, its one
/// option marked "(Recommended)" / "(Recomendado)" (any case), by the label the agent
/// wrote. Several or no marked options in any question: `None`, autopilot cannot choose.
pub(crate) fn recommended(questions: &Value) -> Option<Map<String, Value>> {
    let mut answers = Map::new();
    for q in questions.as_array().filter(|qs| !qs.is_empty())? {
        let text = q["question"].as_str()?;
        let mut marked = marked(q);
        let label = marked.next().filter(|_| marked.next().is_none())?;
        answers.insert(text.to_string(), Value::String(label.to_string()));
    }
    Some(answers)
}

/// The text of the first question autopilot cannot choose for (none or several
/// recommended options): the reason it pauses.
fn unanswerable(questions: &Value) -> &str {
    questions
        .as_array()
        .into_iter()
        .flatten()
        .find(|q| marked(q).count() != 1)
        .or_else(|| questions.get(0))
        .and_then(|q| q["question"].as_str())
        .unwrap_or_default()
}

/// A question of the task's primary session while autopilot runs: the answers when
/// every question has a recommended option (each one goes to the history), else the
/// autopilot pauses with the first unanswerable question as the reason and `None` lets
/// it be asked. A project's owner has no autopilot: `None`.
pub(crate) async fn answer_question(
    app: &AppState,
    key: &TaskKey,
    session: &str,
    questions: &Value,
) -> Option<Map<String, Value>> {
    if !is_task(key) {
        return None;
    }
    let task = app.store.get_task(key).await.ok()??;
    if !is_running(&task) {
        return None;
    }
    let primary = crate::handoff::primary_session(app, key).await?;
    if primary.id != session {
        return None;
    }
    let Some(answers) = recommended(questions) else {
        pause(app, key, unanswerable(questions)).await;
        return None;
    };
    for (question, answer) in &answers {
        let event = Event::AutopilotAnswered {
            question: question.clone(),
            answer: answer.as_str().unwrap_or_default().to_string(),
        };
        if let Err(e) = app.transition(key, event).await {
            tracing::warn!(task = %key, "cannot record the autopilot's answer: {e}");
        }
    }
    Some(answers)
}
