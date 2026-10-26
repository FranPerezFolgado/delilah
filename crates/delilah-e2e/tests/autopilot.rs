//! Autopilot against the fake Claude: waiting on background work is not a stall.

use std::time::Duration;

use delilah_e2e::api::items;
use delilah_e2e::{Env, EnvOptions};
use serde_json::{Value, json};

const T: Duration = Duration::from_secs(60);
const KEY: &str = "E2E-80";

fn running(agent: &str, n: usize) -> Vec<Value> {
    (0..n)
        .map(|i| {
            json!({"id": format!("b{i}"), "type": "subagent", "agent_type": agent,
            "description": format!("job {i}"), "status": "running"})
        })
        .collect()
}

fn event_names(events: &Value) -> Vec<String> {
    items(events)
        .iter()
        .map(|e| e["event"]["type"].as_str().unwrap_or_default().to_string())
        .collect()
}

fn step_started(events: &Value) -> Vec<String> {
    items(events)
        .iter()
        .filter(|e| e["event"]["type"] == "step-started")
        .map(|e| e["event"]["step"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[test]
fn autopilot_waits_on_background_work() {
    let env = Env::start(EnvOptions::default());
    env.register_fixtures();
    let dir = format!("specs/{}-x", KEY.to_lowercase());
    env.scenario(
        &format!("{KEY}.feature"),
        &json!({"steps": [
            {"write": {"path": format!("{dir}/spec.md"), "content": "# Spec\n"}},
            {"write": {"path": format!("{dir}/plan.md"), "content": "# Plan\n"}},
            {"stop": {"message": "waiting", "background": running("architecture-reviewer", 1)}},
            {"sleep_ms": 600},
            {"stop": {"message": "still waiting", "background": running("architecture-reviewer", 1)}},
            {"write": {"path": format!("{dir}/tasks.md"),
                "content": "- [ ] T001 One\n- [ ] T002 Two\n- [ ] T003 Three\n"}},
            {"stop": {"message": "planned", "background": []}},
        ]}),
    );
    env.scenario(
        &format!("{KEY}.orchestrate"),
        &json!({"steps": [
            {"stop": {"message": "working", "background": running("implementer", 2)}},
            {"sleep_ms": 600},
            {"stop": {"message": "still working", "background": running("implementer", 2)}},
            {"write": {"path": format!("{dir}/tasks.md"),
                "content": "- [x] T001 One\n- [x] T002 Two\n- [x] T003 Three\n"}},
            {"stop": {"message": "done", "background": []}},
        ]}),
    );
    // The review session just waits: its turn never ends.
    env.scenario(KEY, &json!({"steps": [{"wait_input": true}]}));
    let api = env.api();
    // Without the save-notes turn, the next step starts at once.
    api.set_preference("save-notes", json!(false));
    api.start_run(
        json!({"key": KEY, "title": "Background", "project": "signal", "autopilot": true}),
    );

    let mut paused_seen = false;
    let events = env.wait_for("the review step starts", T, || {
        let task = api.task(KEY);
        paused_seen |= task["autopilot"]["state"] == "paused";
        let events = api.task_events(KEY);
        if step_started(&events).iter().any(|s| s == "review") {
            Ok(events)
        } else {
            Err((task["autopilot"].clone(), event_names(&events)))
        }
    });

    assert!(!paused_seen, "the autopilot paused while waiting");
    let names = event_names(&events);
    assert!(
        !names
            .iter()
            .any(|n| n == "autopilot-nudged" || n == "autopilot-paused"),
        "{names:?}"
    );
    assert_eq!(step_started(&events), ["implement", "review"], "{names:?}");
    assert_ne!(api.task(KEY)["autopilot"]["state"], "paused");
}
