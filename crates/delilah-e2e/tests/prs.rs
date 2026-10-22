//! US2-7: the PR loop — failing checks, a review, then the merge (FR-010, FR-012).

use std::process::Command;
use std::time::Duration;

use delilah_e2e::api::items;
use delilah_e2e::fixtures::{Checks, FakeGh};
use delilah_e2e::{Env, EnvOptions};
use reqwest::Method;
use serde_json::{Value, json};

const T: Duration = Duration::from_secs(30);

fn branches(env: &Env) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(&env.fixtures.repo)
        .args(["branch", "--list", "feature/*"])
        .output()
        .expect("git branch");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The inbox items of the task whose kind is `kind`.
fn notices(env: &Env, key: &str, kind: &str) -> Vec<Value> {
    let inbox = env.api().inbox();
    items(&inbox).iter().filter(|i| i["task"] == key && i["kind"] == kind).cloned().collect()
}

#[test]
fn failing_checks_a_review_and_the_merge() {
    let env = Env::start(EnvOptions::default());
    env.register_fixtures();
    env.api().start_run(json!({
        "key": "E2E-10", "title": "Pr loop", "project": "signal", "prompt": "Pr loop",
    }));
    env.wait_for("a workspace and its branch", T, || {
        let b = branches(&env);
        if b.contains("feature/E2E-10") { Ok(()) } else { Err(b) }
    });

    // The loop only looks for a branch's PR once the task is in review.
    env.api().set_phase("E2E-10", "review");
    env.gh.open_pr(1, "feature/E2E-10");
    // The PR's head is the branch tip, as on GitHub; archiving on merge checks it.
    let tip = Command::new("git")
        .arg("-C")
        .arg(&env.fixtures.repo)
        .args(["rev-parse", "feature/E2E-10"])
        .output()
        .expect("git rev-parse");
    env.gh.set_head(1, String::from_utf8_lossy(&tip.stdout).trim());
    let pr = env.wait_for("PR 1 linked to E2E-10", T, || {
        let all = env.api().prs();
        match items(&all).iter().find(|p| p["task"] == "E2E-10") {
            Some(p) => Ok(p.clone()),
            None => Err(all),
        }
    });
    assert_eq!((pr["count"].as_i64(), pr["state"].as_str()), (Some(1), Some("open")), "{pr}");
    let (code, detail) = env.api().raw(Method::GET, "/api/v1/tasks/E2E-10/prs", None);
    assert!(code == 200 && detail.to_string().contains(&FakeGh::pr_url(1)), "{code} {detail}");

    env.gh.set_checks(1, Checks::Failing);
    let failed = env.wait_for("a pr-failed notice", T, || {
        let n = notices(&env, "E2E-10", "pr-failed");
        if n.is_empty() { Err(env.api().inbox()) } else { Ok(n) }
    });
    assert!(failed[0].to_string().contains("build"), "{}", failed[0]);

    env.gh.add_review(1, "please rename this");
    let review = env.wait_for("a pr-review notice", T, || {
        let n = notices(&env, "E2E-10", "pr-review");
        if n.is_empty() { Err(env.api().inbox()) } else { Ok(n) }
    });
    assert!(review[0].to_string().contains("reviewer requested changes"), "{}", review[0]);

    env.gh.merge(1);
    env.wait_for("E2E-10 archived", T, || {
        let t = env.api().task("E2E-10");
        if t["status"]["type"] == "done" && !env.root.join("delilah/workspaces/E2E-10").exists() {
            Ok(())
        } else {
            Err((t, env.api().inbox()))
        }
    });
    env.wait_for("the local branch deleted", T, || {
        let b = branches(&env);
        if b.contains("feature/E2E-10") { Err(b) } else { Ok(()) }
    });
}
