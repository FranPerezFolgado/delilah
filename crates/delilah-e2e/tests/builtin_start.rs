//! A project with no workflow chosen starts its tasks with Delilah's built-in prompt:
//! no slash command, one main session, no saving mode.

use std::time::Duration;

use delilah_e2e::{Env, EnvOptions};
use serde_json::json;

const T: Duration = Duration::from_secs(30);

#[test]
fn builtin_start_for_a_fresh_project_free_task_with_notes() {
    let env = Env::start(EnvOptions::default());
    env.register_fixtures();
    let api = env.api();
    // Even with the saving-mode preference on, a task that is not phased runs without it.
    api.set_preference("saving-mode", json!(true));
    let task = api.start_run(json!({
        "key": "", "free": true, "title": "Arreglar el login", "project": "signal",
        "notes": "mira el token", "in_checkout": false,
    }));
    let key = task["key"].as_str().expect("a key").to_string();

    let record = env.wait_for("the agent's record", T, || {
        env.record_for(&key).ok_or("no record yet")
    });
    let prompt = record.prompt.clone().expect("a first prompt");
    assert!(prompt.contains(&key), "{prompt}");
    assert!(prompt.contains("Arreglar el login"), "{prompt}");
    // The notes pointer carries a path; everything before it is the built-in text.
    let (text, pointer) = prompt.split_once(" (").expect("a notes pointer");
    assert!(pointer.contains("notes.md"), "{prompt}");
    assert!(!text.contains('/'), "{text}");

    let task = api.task(&key);
    assert_eq!(task["saving_mode"], false, "{task}");
    assert!(task["autopilot"].is_null(), "{task}");
    let sessions = api.sessions();
    let main: Vec<_> = delilah_e2e::api::items(&sessions)
        .iter()
        .filter(|s| s["task"] == key.as_str() && s["primary"] == true)
        .collect();
    assert_eq!(main.len(), 1, "{sessions}");
}
