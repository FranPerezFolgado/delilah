//! Runs the built `fake-claude` with a temp settings file (not behind `e2e`).

use std::process::{Command, Stdio};

use delilah_e2e::fake::record::Record;
use serde_json::json;

#[test]
fn runs_a_scenario_and_records_it() {
    let dir = tempfile::tempdir().unwrap();
    let fake = dir.path().join("fake");
    std::fs::create_dir_all(fake.join("scenarios")).unwrap();
    std::fs::write(
        fake.join("scenarios/default.json"),
        r#"{"steps":[{"print":"hi"},{"stop":"done"},{"exit":0}]}"#,
    )
    .unwrap();
    let log = dir.path().join("hooks.log");
    let hook = |ev: &str| {
        json!([{"matcher": "*", "hooks": [{"type": "command",
            "command": format!("cat >/dev/null; echo {ev} >> {}?task=K-1", log.display())}]}])
    };
    let settings = dir.path().join("settings.json");
    std::fs::write(
        &settings,
        json!({"hooks": {"SessionStart": hook("SessionStart"), "Stop": hook("Stop"),
                         "SessionEnd": hook("SessionEnd")}})
        .to_string(),
    )
    .unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_fake-claude"))
        .args([
            "--fake-dir",
            fake.to_str().unwrap(),
            "--session-id",
            "sess-1",
            "--settings",
        ])
        .arg(&settings)
        .args(["--bogus", "--", "do the thing"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), "hi\n");

    let r = Record::load(&fake, "sess-1").unwrap();
    assert_eq!(r.mode, "new");
    assert_eq!(r.scenario, "default");
    assert_eq!(r.prompt.as_deref(), Some("do the thing"));
    assert_eq!(r.steps_done, 3);
    assert_eq!(r.exited, Some(0));
    assert!(r.args.contains(&"--bogus".to_string()));
    assert!(r.env.contains_key("PATH"));
    assert_eq!(
        r.cwd,
        std::env::current_dir().unwrap().display().to_string()
    );
    let events: Vec<_> = r.hooks.iter().map(|h| h.event.as_str()).collect();
    assert_eq!(events, ["SessionStart", "Stop", "SessionEnd"]);
}

#[test]
fn missing_settings_exits_2() {
    let out = Command::new(env!("CARGO_BIN_EXE_fake-claude"))
        .args(["--fake-dir", "/nonexistent", "--session-id", "x"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn scenario_selection_failure_runs_stop_failure_and_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let fake = dir.path().join("fake");
    std::fs::create_dir_all(fake.join("scenarios")).unwrap();
    let log = dir.path().join("hooks.log");
    let settings = dir.path().join("settings.json");
    let command = format!("cat >> {} # ?task=K-1", log.display());
    std::fs::write(
        &settings,
        json!({"hooks": {"StopFailure": [{"matcher": "*", "hooks": [{"type": "command", "command": command}]}]}})
            .to_string(),
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_fake-claude"))
        .args([
            "--fake-dir",
            fake.to_str().unwrap(),
            "--session-id",
            "sess-2",
            "--settings",
        ])
        .arg(&settings)
        .args(["--", "[fake:nope]"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let logged: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&log).unwrap()).unwrap();
    assert_eq!(logged["hook_event_name"], "StopFailure");
    assert_eq!(logged["session_id"], "sess-2");
}

#[test]
fn stream_mode_plays_turns_and_follows_the_permission_answer() {
    use std::io::{BufRead, BufReader, Write};

    let dir = tempfile::tempdir().unwrap();
    let fake = dir.path().join("fake");
    std::fs::create_dir_all(fake.join("scenarios")).unwrap();
    std::fs::write(fake.join("scenarios/default.json"), r#"{"steps":[]}"#).unwrap();
    std::fs::write(
        fake.join("scenarios/chat.json"),
        json!({"turns": [[
            {"tool": {"name": "Bash", "input": {"command": "ls -la"}, "output": "total 0"}},
            {"say": {"text": "Hecho."}},
        ]]})
        .to_string(),
    )
    .unwrap();
    let settings = dir.path().join("settings.json");
    std::fs::write(&settings, r#"{"hooks":{}}"#).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_fake-claude"))
        .args(["--fake-dir", fake.to_str().unwrap()])
        .args([
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
        ])
        .args([
            "--permission-prompt-tool",
            "stdio",
            "--session-id",
            "sess-s",
            "--settings",
        ])
        .arg(&settings)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut next = || -> serde_json::Value {
        serde_json::from_str(&stdout.next().expect("a line").unwrap()).unwrap()
    };
    writeln!(
        stdin,
        "{}",
        json!({"type": "user", "message": {"role": "user", "content": "hola [fake:chat]"}})
    )
    .unwrap();
    let request = loop {
        let v = next();
        if v["type"] == "control_request" {
            break v;
        }
    };
    assert_eq!(request["request"]["subtype"], "can_use_tool");
    assert_eq!(request["request"]["input"]["command"], "ls -la");
    writeln!(
        stdin,
        "{}",
        json!({"type": "control_response", "response": {"subtype": "success",
            "request_id": request["request_id"],
            "response": {"behavior": "allow", "updatedInput": {"command": "ls -la"}}}})
    )
    .unwrap();
    let mut seen = vec![];
    loop {
        let v = next();
        let done = v["type"] == "result";
        seen.push(v);
        if done {
            break;
        }
    }
    let result = seen
        .iter()
        .find(|v| v["type"] == "user")
        .expect("a tool result");
    assert_eq!(result["message"]["content"][0]["content"], "total 0");
    assert_eq!(result["message"]["content"][0]["is_error"], false);
    assert_eq!(seen.last().unwrap()["result"], "Hecho.");
    drop(stdin);
    assert_eq!(child.wait().unwrap().code(), Some(0));

    let r = Record::load(&fake, "sess-s").unwrap();
    assert_eq!(r.scenario, "chat");
    assert_eq!(r.inputs, ["hola [fake:chat]"]);
    assert_eq!(r.permissions[0]["response"]["behavior"], "allow");
    assert_eq!(r.exited, Some(0));
}
