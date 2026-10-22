//! US2-5, US2-6: the API, the CLI and the event stream agree on a task's state and phase;
//! an ask is answered and the task goes on (FR-010, FR-011, FR-012).

use std::thread;
use std::time::Duration;

use delilah_e2e::api::{items, phase_of};
use delilah_e2e::{Env, EnvOptions};
use serde_json::json;

const T: Duration = Duration::from_secs(20);

fn start(env: &Env, key: &str, marker: &str) {
    env.register_fixtures();
    env.api().start_run(json!({
        "key": key, "title": "States", "project": "signal",
        "prompt": format!("States {marker}"),
    }));
}

/// What `delilah show <key>` prints for `phase:` and `status:`.
fn cli_state(env: &Env, key: &str) -> (String, String) {
    let (status, out, err) = env.cli().run(&["show", key]);
    assert!(status.success(), "delilah show failed: {status}\n{out}\n{err}");
    let field = |name: &str| {
        out.lines()
            .find_map(|l| l.trim().strip_prefix(name))
            .unwrap_or_else(|| panic!("no `{name}` in:\n{out}"))
            .trim()
            .to_string()
    };
    (field("phase:"), field("status:"))
}

/// The API's (phase, status) for a task; `-` when it has no phase, as the CLI prints it.
fn api_state(env: &Env, key: &str) -> (String, String) {
    let t = env.api().task(key);
    let phase = if phase_of(&t).is_empty() { "-" } else { phase_of(&t) };
    (phase.to_string(), t["status"]["type"].as_str().unwrap_or("").to_string())
}

/// Waits until the API reports `(phase, status)` (`None` = any phase), then checks the CLI
/// says the same.
fn agree(env: &Env, key: &str, phase: Option<&str>, status: &str) {
    let want = phase.map(str::to_string);
    let seen = env.wait_for(&format!("{key} at {phase:?}/{status}"), T, || {
        let s = api_state(env, key);
        if s.1 == status && want.as_ref().is_none_or(|p| *p == s.0) { Ok(s) } else { Err(s) }
    });
    assert_eq!(cli_state(env, key), seen, "CLI and API disagree");
}

/// Runs `act` while listening to the event stream; returns the topics heard.
fn events_during(env: &Env, act: impl FnOnce()) -> Vec<String> {
    let api = env.api();
    let listener = thread::spawn(move || api.events(2500));
    thread::sleep(Duration::from_millis(500));
    act();
    listener.join().expect("event listener")
}

#[test]
fn api_cli_and_events_agree_through_a_tasks_life() {
    let env = Env::start(EnvOptions::default());
    start(&env, "E2E-20", "");
    agree(&env, "E2E-20", None, "waiting-for-user");

    let heard = events_during(&env, || {
        env.api().set_phase("E2E-20", "plan");
    });
    assert!(heard.iter().any(|t| t == "tasks"), "no tasks event: {heard:?}");
    agree(&env, "E2E-20", Some("plan"), "waiting-for-user");

    let heard = events_during(&env, || {
        let (status, out, err) = env.cli().run(&["done", "E2E-20"]);
        assert!(status.success(), "delilah done failed: {status}\n{out}\n{err}");
    });
    assert!(heard.iter().any(|t| t == "tasks"), "no tasks event: {heard:?}");
    agree(&env, "E2E-20", Some("plan"), "done");
    let (_, out, _) = env.cli().run(&["ls"]);
    assert!(out.contains("E2E-20"), "{out}");
}

#[test]
fn an_answered_ask_leaves_the_inbox_and_the_task_goes_on() {
    let env = Env::start(EnvOptions::default());
    env.scenario(
        "ask",
        &json!({"steps": [{"ask": {"question": "Which colour?", "header": "Colour",
            "options": ["red", "blue"]}}, {"sleep_ms": 2500}, {"stop": "done"}]}),
    );
    start(&env, "E2E-21", "[fake:ask]");
    let ask = env.wait_for("the ask in the inbox", T, || {
        let inbox = env.api().inbox();
        match items(&inbox).iter().find(|i| i["task"] == "E2E-21" && i["ask"].is_object()) {
            Some(i) => Ok(i["ask"].clone()),
            None => Err(inbox),
        }
    });
    agree(&env, "E2E-21", None, "waiting-for-user");

    let id = ask["id"].as_str().unwrap().to_string();
    let heard = events_during(&env, || {
        env.api().answer_ask(&id, json!({"Which colour?": "blue"}));
    });
    assert!(heard.iter().any(|t| t == "inbox"), "no inbox event: {heard:?}");

    agree(&env, "E2E-21", None, "running");
    env.wait_for("the ask left the inbox", T, || {
        let inbox = env.api().inbox();
        if items(&inbox).iter().any(|i| i["ask"]["id"] == id.as_str()) { Err(inbox) } else { Ok(()) }
    });
    let rec = env.wait_for("the answer in the record", T, || match env.record_for("E2E-21") {
        Some(r) if !r.answers.is_empty() => Ok(r),
        other => Err(other.map(|r| r.answers)),
    });
    assert!(rec.answers[0].to_string().contains("blue"), "{:?}", rec.answers);
    // Past the ask the scenario stops: the task is waiting for the user again.
    env.wait_for("the stop after the ask", T, || match env.record_for("E2E-21") {
        Some(r) if r.exited.is_some() || r.steps_done >= 3 => Ok(()),
        other => Err(other.map(|r| (r.steps_done, r.exited))),
    });
    agree(&env, "E2E-21", None, "waiting-for-user");
}
