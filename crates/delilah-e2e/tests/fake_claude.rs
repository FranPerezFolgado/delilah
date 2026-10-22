//! US1: the daemon launches the fake Claude instead of the real one (FR-001..FR-007).

use std::time::Duration;

use delilah_e2e::api::items;
use delilah_e2e::fake::record::Record;
use delilah_e2e::{Env, EnvOptions};
use serde_json::{Value, json};

const T: Duration = Duration::from_secs(20);

/// Copy of `delilah-agents`' `KEPT_ENV` (crate-private there).
const KEPT_ENV: &[&str] = &[
    "HOME", "USER", "LOGNAME", "SHELL", "PATH", "LANG", "LC_ALL", "TMPDIR", "HTTPS_PROXY",
    "HTTP_PROXY", "NO_PROXY", "ALL_PROXY", "https_proxy", "http_proxy", "no_proxy", "all_proxy",
    "SSL_CERT_FILE", "NODE_EXTRA_CA_CERTS",
];

/// Variables the `#!/bin/sh` wrapper around the fake (and macOS) add on their own.
const SHELL_ADDED: &[&str] = &["PWD", "OLDPWD", "SHLVL", "_", "__CF_USER_TEXT_ENCODING"];

fn start(env: &Env, key: &str, title: &str, marker: &str) -> Value {
    env.register_fixtures();
    env.api().start_run(json!({
        "key": key, "title": title, "project": "signal",
        "prompt": format!("{title} {marker}"),
    }))
}

/// The task's primary session as `GET /sessions` shows it: (id, state).
fn live_session(env: &Env, key: &str) -> Option<(String, String)> {
    let all = env.api().sessions();
    items(&all)
        .iter()
        .find(|s| s["task"] == key && s["primary"] == true && s["live"] == true)
        .map(|s| (s["id"].as_str().unwrap().to_string(), s["state"].as_str().unwrap_or("").to_string()))
}

fn wait_state(env: &Env, key: &str, want: &str) -> String {
    env.wait_for(&format!("{key} live state {want}"), T, || match live_session(env, key) {
        Some((id, s)) if s == want => Ok(id),
        other => Err(other),
    })
}

fn wait_record(env: &Env, key: &str, what: &str, ok: impl Fn(&Record) -> bool) -> Record {
    env.wait_for(what, T, || match env.record_for(key) {
        Some(r) if ok(&r) => Ok(r),
        other => Err(other.map(|r| (r.mode, r.steps_done, r.exited))),
    })
}

#[test]
fn launches_with_the_real_arguments_and_clean_env() {
    let env = Env::start(EnvOptions::default());
    env.scenario("echo", &json!({"steps": [{"print": "hello"}, {"stop": "ok"}]}));
    start(&env, "E2E-1", "Echo task", "[fake:echo]");
    let rec = wait_record(&env, "E2E-1", "the echo record", |r| r.scenario == "echo");

    let ws = env.root.join("delilah/workspaces/E2E-1");
    assert_eq!(std::fs::canonicalize(&rec.cwd).unwrap(), std::fs::canonicalize(&ws).unwrap());
    assert_eq!(rec.mode, "new");
    let sid = wait_state(&env, "E2E-1", "waiting");
    assert_eq!(rec.session_id, sid);
    let settings = std::fs::read_to_string(&rec.settings).expect("settings file");
    assert!(settings.contains("hooks") && settings.contains("Stop"), "{settings}");
    assert!(rec.prompt.as_deref().unwrap_or("").contains("Echo task"), "{:?}", rec.prompt);
    for k in rec.env.keys() {
        assert!(KEPT_ENV.contains(&k.as_str()) || k == "TERM" || k == "COLORTERM" || SHELL_ADDED.contains(&k.as_str()), "stray env {k}");
    }
    assert!(env.tripwire_log().is_none());
}

#[test]
fn default_scenario_goes_working_then_waiting() {
    let env = Env::start(EnvOptions::default());
    env.scenario(
        "slow",
        &json!({"steps": [{"hook": "UserPromptSubmit"}, {"sleep_ms": 2500}, {"stop": "done"}]}),
    );
    start(&env, "E2E-2", "Slow", "[fake:slow]");
    wait_state(&env, "E2E-2", "working");
    wait_state(&env, "E2E-2", "waiting");
}

#[test]
fn an_ask_reaches_the_inbox_and_its_answer_the_fake() {
    let env = Env::start(EnvOptions::default());
    env.scenario(
        "ask",
        &json!({"steps": [{"ask": {"question": "Which colour?", "header": "Colour",
            "options": ["red", "blue"]}}, {"stop": "done"}]}),
    );
    start(&env, "E2E-3", "Ask", "[fake:ask]");
    let ask = env.wait_for("the ask in the inbox", T, || {
        let inbox = env.api().inbox();
        match items(&inbox).iter().find(|i| i["task"] == "E2E-3" && i["ask"].is_object()) {
            Some(i) => Ok(i["ask"].clone()),
            None => Err(inbox),
        }
    });
    assert!(ask["questions"].to_string().contains("Which colour?"), "{ask}");
    env.api().answer_ask(ask["id"].as_str().unwrap(), json!({"Which colour?": "blue"}));
    let rec = wait_record(&env, "E2E-3", "the answer in the record", |r| !r.answers.is_empty());
    assert!(rec.answers[0].to_string().contains("blue"), "{:?}", rec.answers);
}

#[test]
fn resume_launches_the_fake_in_resume_mode_with_the_same_session() {
    let mut env = Env::start(EnvOptions::default());
    start(&env, "E2E-4", "Resume", "");
    let first = wait_record(&env, "E2E-4", "the first record", |r| r.steps_done >= 1);
    assert_eq!(first.mode, "new");
    wait_state(&env, "E2E-4", "waiting");
    env.stop_daemon();
    env.spawn_daemon();
    env.api().reopen_session(&first.session_id);
    let second = wait_record(&env, "E2E-4", "a resume record", |r| r.mode == "resume");
    assert_eq!(second.session_id, first.session_id);
}

#[test]
fn a_failing_script_shows_the_session_as_failed() {
    let env = Env::start(EnvOptions::default());
    env.scenario("fail", &json!({"steps": [{"fail": "boom"}]}));
    start(&env, "E2E-5", "Fail", "[fake:fail]");
    // Like real Claude, the fake stays alive after a failed turn; the daemon shows the
    // error in the inbox for that task, and the session as live.
    let item = env.wait_for("the error item in the inbox", T, || {
        let inbox = env.api().inbox();
        match items(&inbox).iter().find(|i| i["task"] == "E2E-5" && i["kind"] == "error") {
            Some(i) => Ok(i.clone()),
            None => Err(inbox),
        }
    });
    assert!(item["text"].as_str().unwrap_or("").contains("boom"), "{item}");
    let rec = wait_record(&env, "E2E-5", "the StopFailure hook", |r| {
        r.hooks.iter().any(|h| h.event == "StopFailure")
    });
    assert_eq!(rec.exited, None, "{rec:?}");
    let (_, state) = live_session(&env, "E2E-5").expect("a live session after the failure");
    assert!(!state.is_empty());
    // It is still there a moment later (the SessionEnd would clear the item).
    std::thread::sleep(Duration::from_millis(500));
    let inbox = env.api().inbox();
    assert!(items(&inbox).iter().any(|i| i["task"] == "E2E-5" && i["kind"] == "error"), "{inbox}");
}

#[test]
fn without_the_fake_the_daemon_hits_the_tripwire() {
    let env = Env::start(EnvOptions { no_agent_bin: true, ..Default::default() });
    start(&env, "E2E-6", "Tripwire", "[fake:echo]");
    let log = env.wait_for("tripwire.log", T, || env.tripwire_log().ok_or(()));
    assert!(log.contains("claude"), "{log}");
    std::fs::remove_file(env.root.join("tripwire.log")).unwrap();
}

#[test]
fn the_trust_dialog_is_answered_by_the_daemon() {
    let env = Env::start(EnvOptions::default());
    env.scenario(
        "trust",
        &json!({"steps": [{"trust_dialog": true}, {"hook": "UserPromptSubmit"}, {"sleep_ms": 2500}, {"stop": "done"}]}),
    );
    start(&env, "E2E-7", "Trust", "[fake:trust]");
    wait_state(&env, "E2E-7", "working");
    // Past the dialog: it read the daemon's Enter, so the first step is done.
    let rec = wait_record(&env, "E2E-7", "past the dialog", |r| r.steps_done >= 1);
    assert_eq!(rec.exited, None);
}
