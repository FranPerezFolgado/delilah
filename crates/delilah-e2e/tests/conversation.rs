//! US2: an idea conversation ends with the proposal card; confirming it creates a
//! backlog task in the project with the brief in its notes (FR-010, FR-012).

use std::time::Duration;

use delilah_e2e::api::items;
use delilah_e2e::{Env, EnvOptions};
use serde_json::json;

const T: Duration = Duration::from_secs(20);

#[test]
fn confirming_the_proposal_card_creates_a_backlog_task_with_the_brief() {
    let env = Env::start(EnvOptions::default());
    env.register_fixtures();
    let title = "Dashboard de uso";
    let brief = "Build a user dashboard with usage charts.";
    env.scenario(
        "@signal",
        &json!({"steps": [
            {"ask": {"question": title, "header": "Nueva tarea",
                "options": [{"label": "Crear", "preview": brief}, "Cambiar algo", "Descartar"]}},
            {"stop": "ok"},
        ]}),
    );

    let api = env.api();
    let session = api.open_project_session("signal", true);
    let id = session["id"].as_str().expect("session id").to_string();
    env.wait_for("the session to wait for a message", T, || {
        let all = api.sessions();
        match items(&all).iter().find(|s| s["id"] == id.as_str()) {
            Some(s) if s["live"] == true => Ok(()),
            other => Err(other.cloned()),
        }
    });
    api.send_message(&id, "I want a user dashboard");

    let card = env.wait_for("the proposal card", T, || {
        let conv = api.conversation(&id);
        match items(&conv["entries"]).iter().find(|e| e["kind"] == "card" && e["outcome"] == "pending") {
            Some(c) => Ok(c.clone()),
            None => Err(conv),
        }
    });
    assert_eq!(card["proposal"]["title"], title, "{card}");
    assert_eq!(card["proposal"]["brief"], brief, "{card}");

    api.answer_ask(card["ask"].as_str().unwrap(), json!({title: "Crear"}));

    let done = env.wait_for("the card answered with a task", T, || {
        let conv = api.conversation(&id);
        match items(&conv["entries"]).iter().find(|e| e["kind"] == "card" && e["task"].is_string()) {
            Some(c) => Ok(c.clone()),
            None => Err(conv),
        }
    });
    let task = api.task(done["task"].as_str().unwrap());
    assert_eq!(task["status"]["type"], "queued", "{task}");
    assert_eq!(task["title"], title, "{task}");
    assert_eq!(task["project"], "signal", "{task}");
    assert!(task["notes"].as_str().unwrap_or("").contains(brief), "{task}");
}
