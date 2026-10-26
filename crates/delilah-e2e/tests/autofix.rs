//! US1: a failing head reaches the idle agent once; the new green head closes the loop.

use std::process::Command;
use std::time::Duration;

use delilah_e2e::api::items;
use delilah_e2e::fixtures::Checks;
use delilah_e2e::{Env, EnvOptions};
use serde_json::{Value, json};

const T: Duration = Duration::from_secs(30);
const KEY: &str = "E2E-90";

fn autofix_inputs(env: &Env) -> Vec<String> {
    env.record_for(KEY)
        .map(|r| {
            r.inputs
                .into_iter()
                .filter(|i| i.contains("attempt 1 of 3"))
                .collect()
        })
        .unwrap_or_default()
}

fn autofix_of(env: &Env) -> Value {
    let (_, prs) = env.api().raw(
        reqwest::Method::GET,
        &format!("/api/v1/tasks/{KEY}/prs"),
        None,
    );
    prs["prs"]
        .as_array()
        .and_then(|a| a.first())
        .map(|p| p["autofix"].clone())
        .unwrap_or(Value::Null)
}

fn event_names(env: &Env) -> Vec<String> {
    items(&env.api().task_events(KEY))
        .iter()
        .map(|e| e["event"]["type"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[test]
fn autofix_types_one_prompt_then_sees_the_fix() {
    let env = Env::start(EnvOptions::default());
    env.register_fixtures();
    // The agent ends its turn (idle), takes the autofix prompt, and ends that turn too.
    env.scenario(
        KEY,
        &json!({"steps": [
            {"stop": "ready"},
            {"wait_input": true},
            {"stop": "fixed and pushed"},
            {"wait_input": true},
        ]}),
    );
    let api = env.api();
    api.set_preference("language", json!("en"));
    // `/runs` turns autofix on for this task, as the dialog's switch does.
    api.start_run(json!({
        "key": KEY, "title": "Autofix", "project": "signal", "prompt": "Autofix",
        "ci_autofix": true,
    }));
    assert_eq!(api.task(KEY)["ci_autofix"], true);
    env.wait_for("the agent idle after its first turn", T, || {
        match env.record_for(KEY) {
            Some(r) if r.hooks.iter().any(|h| h.event == "Stop") => Ok(()),
            other => Err(other.map(|r| r.hooks)),
        }
    });
    let branch = env.wait_for("a branch", T, || {
        let t = api.task(KEY);
        t["branch"].as_str().map(str::to_string).ok_or(t)
    });
    api.set_phase(KEY, "review");
    env.gh.open_pr(1, &branch);
    let tip = Command::new("git")
        .arg("-C")
        .arg(&env.fixtures.repo)
        .args(["rev-parse", &branch])
        .output()
        .expect("git rev-parse");
    env.gh
        .set_head(1, String::from_utf8_lossy(&tip.stdout).trim());
    env.gh.set_checks(1, Checks::Failing);

    env.wait_for("the autofix prompt reaches the agent", T, || {
        let n = autofix_inputs(&env);
        if n.is_empty() {
            Err((autofix_of(&env), event_names(&env)))
        } else {
            Ok(n)
        }
    });
    let fixing = env.wait_for("the PR is fixing", T, || {
        let a = autofix_of(&env);
        if a["state"] == "fixing" {
            Ok(a)
        } else {
            Err(a)
        }
    });
    assert_eq!(fixing["attempts"], 1, "{fixing}");
    assert!(
        fixing["sha"].is_string() && fixing["sent_at"].is_string(),
        "{fixing}"
    );
    let notices: Vec<_> = items(&api.inbox())
        .iter()
        .filter(|i| i["task"] == KEY && i["kind"] == "ci-failed")
        .cloned()
        .collect();
    assert!(
        notices.is_empty(),
        "ci-failed should be resolved: {notices:?}"
    );

    // Several poll cycles (1s each): still one prompt for the same head.
    std::thread::sleep(Duration::from_secs(4));
    assert_eq!(autofix_inputs(&env).len(), 1, "{:?}", autofix_inputs(&env));

    // The agent pushed: a new head, green.
    env.gh.set_head(1, "sha-1-fixed");
    env.gh.set_checks(1, Checks::Passing);
    let watching = env.wait_for("the PR back to watching", T, || {
        let a = autofix_of(&env);
        if a["state"] == "watching" {
            Ok(a)
        } else {
            Err(a)
        }
    });
    assert_eq!(watching["attempts"], 0, "{watching}");
    let names = event_names(&env);
    let pos = |n: &str| names.iter().position(|e| e == n);
    let (sent, fixed) = (pos("autofix-sent"), pos("autofix-fixed"));
    assert!(
        sent.is_some() && fixed.is_some() && sent < fixed,
        "{names:?}"
    );
    assert_eq!(autofix_inputs(&env).len(), 1);
}
