//! US1: a failing head reaches the idle agent once; the new green head closes the loop.

use std::process::Command;
use std::time::Duration;

use delilah_e2e::api::items;
use delilah_e2e::fixtures::Checks;
use delilah_e2e::{Env, EnvOptions};
use serde_json::{Value, json};

const T: Duration = Duration::from_secs(30);
const KEY: &str = "E2E-90";

/// The autofix prompt is pasted as seven lines (blanks included) and the fake reads one
/// per `wait_input`; lines left over would reach its echo loop, which ends a turn for
/// each (a no-push).
fn prompt_lines() -> Vec<Value> {
    vec![json!({"wait_input": true}); 7]
}

/// A first turn, then for each of `turns`: one autofix prompt and
/// the steps the agent plays after it.
fn scenario_of(turns: &[Vec<Value>]) -> Value {
    let mut steps = vec![json!({"stop": "ready"})];
    for after in turns {
        steps.extend(prompt_lines());
        steps.extend(after.iter().cloned());
    }
    json!({ "steps": steps })
}

/// The agent ends its turn after `ms`, time for the test to move the PR's head first.
fn stop_after(ms: u64) -> Vec<Value> {
    vec![json!({"sleep_ms": ms}), json!({"stop": "done"})]
}

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
    // The agent ends its turn (idle) and takes the autofix prompt; it ends no turn after
    // it, since a turn end on the unchanged head would count as no-push (WP-10).
    env.scenario(KEY, &scenario_of(&[vec![]]));
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

/// Starts the autofix task, waits until the agent is idle, opens PR 1 on its branch
/// with the branch tip as head and failing checks.
fn start_with_failing_pr(env: &Env, scenario: Value) {
    env.register_fixtures();
    env.scenario(KEY, &scenario);
    let api = env.api();
    api.set_preference("language", json!("en"));
    api.start_run(json!({
        "key": KEY, "title": "Autofix", "project": "signal", "prompt": "Autofix",
        "ci_autofix": true,
    }));
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
}

fn prompts(env: &Env) -> Vec<String> {
    env.record_for(KEY)
        .map(|r| {
            r.inputs
                .into_iter()
                .filter(|i| i.contains(" of 3"))
                .collect()
        })
        .unwrap_or_default()
}

fn autofix_items(env: &Env) -> Vec<Value> {
    items(&env.api().inbox())
        .iter()
        .filter(|i| i["task"] == KEY && i["kind"] == "pr-autofix")
        .cloned()
        .collect()
}

#[test]
fn always_failing_checks_stop_after_three_prompts() {
    let env = Env::start(EnvOptions::default());
    // Each turn ends 3s after its prompt, once the test has moved the head.
    start_with_failing_pr(
        &env,
        scenario_of(&[stop_after(3000), stop_after(3000), stop_after(3000)]),
    );
    for n in 1..=3usize {
        env.wait_for(&format!("prompt {n}"), T, || {
            let p = prompts(&env);
            if p.len() >= n {
                Ok(p)
            } else {
                Err((autofix_of(&env), event_names(&env)))
            }
        });
        let p = prompts(&env);
        assert!(p[n - 1].contains(&format!("attempt {n} of 3")), "{p:?}");
        // A new red head after each prompt (autofix never sends the same sha twice).
        if n < 3 {
            env.gh.set_head(1, &format!("sha-1-red-{n}"));
            env.gh.set_checks(1, Checks::Failing);
        }
    }
    // Attempt 3 stays red on a new head: it stops at the limit.
    env.gh.set_head(1, "sha-1-red-3");
    env.gh.set_checks(1, Checks::Failing);
    let item = env.wait_for("the stopped item", T, || {
        let i = autofix_items(&env);
        i.first().cloned().ok_or(autofix_of(&env))
    });
    assert_eq!(item["reason"], "limit", "{item}");
    assert_eq!(item["attempts"], 3, "{item}");
    let a = autofix_of(&env);
    assert_eq!(a["state"], "stopped", "{a}");

    // More poll cycles and another red head: no 4th prompt, still one item.
    env.gh.set_head(1, "sha-1-red-4");
    env.gh.set_checks(1, Checks::Failing);
    std::thread::sleep(Duration::from_secs(4));
    assert_eq!(prompts(&env).len(), 3, "{:?}", prompts(&env));
    assert_eq!(autofix_items(&env).len(), 1);
}

#[test]
fn a_turn_that_does_not_push_stops_with_no_push() {
    let env = Env::start(EnvOptions::default());
    // The agent takes the prompt and ends its turn without pushing.
    start_with_failing_pr(
        &env,
        scenario_of(&[vec![json!({"stop": "nothing to push"})]]),
    );
    env.wait_for("the autofix prompt", T, || {
        let p = prompts(&env);
        if p.is_empty() {
            Err((autofix_of(&env), event_names(&env)))
        } else {
            Ok(p)
        }
    });
    let item = env.wait_for("the no-push item", T, || {
        let i = autofix_items(&env);
        i.first().cloned().ok_or(autofix_of(&env))
    });
    assert_eq!(item["reason"], "no-push", "{item}");
    let a = autofix_of(&env);
    assert_eq!(a["state"], "stopped", "{a}");
    assert_eq!(a["reason"], "no-push", "{a}");
    assert_eq!(autofix_items(&env).len(), 1);
    assert_eq!(prompts(&env).len(), 1);
}
