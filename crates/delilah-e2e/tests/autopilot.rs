//! Autopilot against the fake Claude: waiting on background work is not a stall.

use std::time::Duration;

use delilah_e2e::api::items;
use delilah_e2e::{Env, EnvOptions};
use serde_json::{Value, json};

const T: Duration = Duration::from_secs(60);
const KEY: &str = "E2E-80";

/// Puts the `/feature` preset (the phased workflow) into the `signal` project: a fresh
/// project has none, and autopilot needs a phased one.
fn use_feature_workflow(env: &Env) {
    let api = env.api();
    let projects = api.projects();
    let mut settings = delilah_e2e::api::items(&projects)
        .iter()
        .find(|p| p["name"] == "signal")
        .expect("the signal project")["settings"]
        .clone();
    settings["workflow"] = json!({
        "kind": "phased",
        "plan": "/feature {task}",
        "implement": "/orchestrate {spec_dir}",
        "review": "/review the branch of this task. Then apply the confirmed findings, update the docs they made stale and commit. Do not push or open a PR.",
    });
    let (status, body) = api.raw(
        reqwest::Method::PUT,
        "/api/v1/projects/signal/settings",
        Some(settings),
    );
    assert_eq!(status, 200, "{body}");
}

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
    use_feature_workflow(&env);
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

#[test]
fn autopilot_still_catches_a_real_stall() {
    let key = "E2E-81";
    let env = Env::start(EnvOptions::default());
    env.register_fixtures();
    use_feature_workflow(&env);
    let dir = format!("specs/{}-x", key.to_lowercase());
    env.scenario(
        &format!("{key}.feature"),
        &json!({"steps": [
            {"write": {"path": format!("{dir}/spec.md"), "content": "# Spec\n"}},
            {"write": {"path": format!("{dir}/plan.md"), "content": "# Plan\n"}},
            {"write": {"path": format!("{dir}/tasks.md"),
                "content": "- [ ] T001 One\n- [ ] T002 Two\n- [ ] T003 Three\n"}},
            {"stop": {"message": "planned", "background": []}},
        ]}),
    );
    // The implement session ends its turn with nothing running and the tasks unchecked.
    env.scenario(
        &format!("{key}.orchestrate"),
        &json!({"steps": [
            {"stop": {"message": "idle", "background": []}},
            {"wait_input": true},
            {"stop": {"message": "idle again", "background": []}},
        ]}),
    );
    env.scenario(key, &json!({"steps": [{"wait_input": true}]}));
    let api = env.api();
    api.set_preference("save-notes", json!(false));
    api.set_preference("language", json!("en"));
    api.start_run(json!({"key": key, "title": "Stall", "project": "signal", "autopilot": true}));

    let (events, task) = env.wait_for("the autopilot pauses", T, || {
        let task = api.task(key);
        let events = api.task_events(key);
        if task["autopilot"]["state"] == "paused" {
            Ok((events, task))
        } else {
            Err((task["autopilot"].clone(), event_names(&events)))
        }
    });

    let names = event_names(&events);
    let nudges = names.iter().filter(|n| *n == "autopilot-nudged").count();
    assert_eq!(nudges, 1, "{names:?}");
    let nudged = names.iter().position(|n| n == "autopilot-nudged").unwrap();
    let paused = names.iter().position(|n| n == "autopilot-paused").unwrap();
    assert!(nudged < paused, "{names:?}");
    assert_eq!(
        names.iter().filter(|n| *n == "autopilot-paused").count(),
        1,
        "{names:?}"
    );
    let nudge = &items(&events)[nudged]["event"];
    assert_eq!(nudge["step"], "implement", "{nudge}");
    assert_eq!(
        task["autopilot"]["reason"], "3 tasks unchecked",
        "{}",
        task["autopilot"]
    );

    let rec = env.wait_for("the nudge reaches the fake", T, || {
        match env.record_for(key) {
            Some(r) if r.inputs.iter().any(|i| i.contains("Autopilot:")) => Ok(r),
            other => Err(other.map(|r| r.inputs)),
        }
    });
    assert!(rec.inputs.iter().any(|i| i.contains("Autopilot:")));
}
