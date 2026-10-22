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
        .args(["--fake-dir", fake.to_str().unwrap(), "--session-id", "sess-1", "--settings"])
        .arg(&settings)
        .args(["--bogus", "--", "do the thing"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "hi\n");

    let r = Record::load(&fake, "sess-1").unwrap();
    assert_eq!(r.mode, "new");
    assert_eq!(r.scenario, "default");
    assert_eq!(r.prompt.as_deref(), Some("do the thing"));
    assert_eq!(r.steps_done, 3);
    assert_eq!(r.exited, Some(0));
    assert!(r.args.contains(&"--bogus".to_string()));
    assert!(r.env.contains_key("PATH"));
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
