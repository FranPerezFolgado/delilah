//! US2: Jira against the fake server, sample data only (FR-009, FR-010).

use std::path::Path;

use delilah_e2e::api::items;
use delilah_e2e::{Env, EnvOptions};
use serde_json::json;

/// Every regular file under `dir` that contains `needle`.
fn files_with(dir: &Path, needle: &str, out: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            files_with(&p, needle, out);
        } else if ft.is_file()
            && let Ok(bytes) = std::fs::read(&p)
            && bytes.windows(needle.len()).any(|w| w == needle.as_bytes())
        {
            out.push(p.display().to_string());
        }
    }
}

#[test]
fn jira_issues_become_tasks_and_the_token_stays_in_memory() {
    let mut env = Env::start(EnvOptions { jira: true, ..Default::default() });
    let site = env.start_jira();
    env.register_fixtures();
    let api = env.api();

    let connected = api.jira_configure(&site);
    assert!(connected["account"].is_string(), "{connected}");

    let issues = api.jira_issues("assigned", "open", "all");
    let issue = items(&issues)
        .iter()
        .find(|i| i["key"].as_str().is_some_and(|k| k.starts_with("PROJ-")))
        .unwrap_or_else(|| panic!("no PROJ issue in {issues}"));
    let key = issue["key"].as_str().unwrap();
    let title = issue["summary"].as_str().unwrap_or_else(|| panic!("no summary in {issue}"));

    let created = api.new_task(json!({"key": key, "title": title, "project": "signal"}));
    assert_eq!(created["key"], key, "{created}");
    let task = api.task(key);
    assert_eq!(task["status"]["type"], "queued", "{task}");
    assert_eq!(task["title"], title, "{task}");

    // Secrets are in memory (DELILAH_SECRETS=memory): the status never echoes the token
    // and no file of the environment holds it.
    let status = api.raw(reqwest::Method::GET, "/api/v1/jira", None).1;
    assert!(!status.to_string().contains("not-a-token"), "{status}");
    let mut found = Vec::new();
    files_with(&env.root, "not-a-token", &mut found);
    assert!(found.is_empty(), "token written to disk: {found:?}");
    assert!(env.tripwire_log().is_none());
}
