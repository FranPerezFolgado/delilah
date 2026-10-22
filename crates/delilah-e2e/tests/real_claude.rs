//! US5: optional smoke test with the real Claude Code. Opt-in twice: `#[ignore]` and
//! `DELILAH_E2E_REAL_CLAUDE=1` (it uses the user's subscription and real HOME). Only
//! claude is real; `gh` stays fake and tripwired. Run it with `scripts/e2e.sh --real-claude`.

use std::time::Duration;

use delilah_e2e::api::items;
use delilah_e2e::{Env, EnvOptions};
use serde_json::json;

const T: Duration = Duration::from_secs(30);

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

#[test]
#[ignore = "uses the real Claude and your subscription: scripts/e2e.sh --real-claude"]
fn real_claude_goes_working_then_waiting() {
    if std::env::var("DELILAH_E2E_REAL_CLAUDE").as_deref() != Ok("1") {
        eprintln!("skipped: set DELILAH_E2E_REAL_CLAUDE=1 to run the real Claude");
        return;
    }
    let env = Env::start(EnvOptions { real_claude: true, ..Default::default() });
    env.register_fixtures();
    env.api().start_run(json!({
        "key": "E2E-REAL", "title": "Reply OK and stop.", "project": "signal",
        "prompt": "Reply OK and stop.",
    }));
    wait_state(&env, "E2E-REAL", "working");
    let session = wait_state(&env, "E2E-REAL", "waiting");
    env.api().close_session(&session);
    env.api().archive("E2E-REAL");
    assert!(!env.finish(), "the gh tripwire fired");
}
