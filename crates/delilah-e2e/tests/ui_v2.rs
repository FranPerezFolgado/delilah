//! UI v2 data side (specs/007-ui-v2/contracts/api.md), through the real HTTP API of an
//! isolated daemon with a fake Claude, a fake `gh` and the sample Jira: a permission ask
//! makes the task wait, reopen, the inbox's ids / dismiss / clear / restore and its avisos,
//! the PR snapshot and the merge, the setup detail, Jira sub-tasks, a session's runtime,
//! `health.home` and the checkout as the default workspace.

use std::time::Duration;

use delilah_e2e::api::{items, status_of};
use delilah_e2e::fixtures::{Checks, FakeGh, GH_USER};
use delilah_e2e::{Env, EnvOptions};
use reqwest::Method;
use serde_json::{Value, json};

const T: Duration = Duration::from_secs(30);

/// Starts a run in a worktree (the default is now the checkout, and two tasks cannot share it).
fn run_in_worktree(env: &Env, key: &str, prompt: &str, extra: Value) {
    let mut body = json!({
        "key": key, "title": format!("{key} title"), "project": "signal",
        "prompt": prompt, "in_checkout": false,
    });
    for (k, v) in extra.as_object().into_iter().flatten() {
        body[k] = v.clone();
    }
    env.api().start_run(body);
}

fn status(env: &Env, key: &str) -> String {
    status_of(&env.api().task(key)).to_string()
}

fn wait_status(env: &Env, key: &str, want: &str) {
    env.wait_for(&format!("{key} {want}"), T, || {
        let s = status(env, key);
        if s == want { Ok(()) } else { Err(s) }
    });
}

/// The first inbox item of `task` whose kind is `kind`.
fn inbox_item(env: &Env, task: &str, kind: &str) -> Option<Value> {
    items(&env.api().inbox())
        .iter()
        .find(|i| i["task"] == task && i["kind"] == kind)
        .cloned()
}

fn wait_item(env: &Env, task: &str, kind: &str) -> Value {
    env.wait_for(&format!("a {kind} item of {task}"), T, || {
        inbox_item(env, task, kind).ok_or_else(|| env.api().inbox())
    })
}

fn inbox_ids(env: &Env) -> Vec<String> {
    items(&env.api().inbox())
        .iter()
        .map(|i| {
            i["id"]
                .as_str()
                .expect("every inbox item has an id")
                .to_string()
        })
        .collect()
}

fn dismiss_uri(id: &str) -> String {
    let encoded: String = id
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    format!("/api/v1/inbox/{encoded}/dismiss")
}

fn post(env: &Env, path: &str, body: Option<Value>) -> (u16, Value) {
    env.api().raw(Method::POST, path, body)
}

fn get(env: &Env, path: &str) -> (u16, Value) {
    env.api().raw(Method::GET, path, None)
}

/// The kinds of the task events, in order.
fn event_types(env: &Env, key: &str) -> Vec<String> {
    let (code, v) = get(env, &format!("/api/v1/tasks/{key}/events"));
    assert_eq!(code, 200, "{v}");
    items(&v)
        .iter()
        .map(|e| {
            e["event"]["type"]
                .as_str()
                .or_else(|| e["event"].as_str())
                .unwrap_or("")
                .to_string()
        })
        .collect()
}

fn session_of(env: &Env, key: &str) -> Value {
    let (code, v) = get(env, &format!("/api/v1/tasks/{key}/sessions"));
    assert_eq!(code, 200, "{v}");
    items(&v)
        .iter()
        .find(|s| s["primary"] == true)
        .unwrap_or_else(|| panic!("no primary session in {v}"))
        .clone()
}

#[test]
fn a_pending_permission_makes_the_task_wait_and_the_answer_runs_it() {
    let env = Env::start(EnvOptions::default());
    env.register_fixtures();
    env.scenario(
        "perm",
        &json!({"turns": [[
            {"tool": {"name": "Bash", "input": {"command": "ls -la"}, "output": "total 0", "auto": "ask"}},
            {"sleep_ms": 4000},
            {"say": {"text": "Hecho."}},
        ]]}),
    );
    run_in_worktree(
        &env,
        "E2E-30",
        "Permiso [fake:perm]",
        json!({"chat_mode": true}),
    );

    let item = wait_item(&env, "E2E-30", "permission");
    assert!(item["id"].is_string(), "{item}");
    assert_eq!(item["permission"]["tool"], "Bash", "{item}");
    wait_status(&env, "E2E-30", "waiting-for-user");
    // The permission is the only item of that session: no extra `waiting`.
    assert!(inbox_item(&env, "E2E-30", "waiting").is_none());
    assert!(
        event_types(&env, "E2E-30")
            .iter()
            .any(|t| t == "permission-asked"),
        "{:?}",
        event_types(&env, "E2E-30")
    );

    let session = item["session"]["id"].as_str().unwrap();
    let ask = item["permission"]["id"].as_str().unwrap();
    let (code, body) = post(
        &env,
        &format!("/api/v1/sessions/{session}/permissions/{ask}"),
        Some(json!({"decision": "allow_once"})),
    );
    assert_eq!(code, 204, "{body}");
    wait_status(&env, "E2E-30", "running");
    assert!(inbox_item(&env, "E2E-30", "permission").is_none());
}

#[test]
fn a_done_task_reopens_and_one_that_is_not_done_is_a_409() {
    let env = Env::start(EnvOptions::default());
    env.register_fixtures();
    run_in_worktree(&env, "E2E-31", "Reabrir", json!({}));
    wait_status(&env, "E2E-31", "waiting-for-user");

    // created_at is always there.
    let task = env.api().task("E2E-31");
    assert!(
        task["created_at"].as_str().is_some_and(|t| t.contains('T')),
        "{task}"
    );
    for t in items(&env.api().tasks()) {
        assert!(t["created_at"].is_string(), "{t}");
    }

    let (code, body) = post(&env, "/api/v1/tasks/E2E-31/reopen", None);
    assert_eq!(code, 409, "not done: {body}");
    let (code, _) = post(&env, "/api/v1/tasks/NOPE-1/reopen", None);
    assert_eq!(code, 404);

    env.api().done("E2E-31");
    assert_eq!(status(&env, "E2E-31"), "done");
    let (code, task) = post(&env, "/api/v1/tasks/E2E-31/reopen", None);
    assert_eq!(code, 200, "{task}");
    assert_eq!(status_of(&task), "waiting-for-user", "{task}");
    assert_eq!(status(&env, "E2E-31"), "waiting-for-user");
    assert!(event_types(&env, "E2E-31").iter().any(|t| t == "reopened"));
}

#[test]
fn the_inbox_has_ids_and_dismiss_clear_and_restore_agree() {
    let env = Env::start(EnvOptions::default());
    env.register_fixtures();
    env.scenario(
        "ask",
        &json!({"steps": [{"ask": {"question": "Which?", "header": "H",
            "options": ["a", "b"]}}, {"sleep_ms": 5000}, {"stop": "done"}]}),
    );
    // A blocking item (a question) ...
    run_in_worktree(&env, "E2E-32", "Pregunta [fake:ask]", json!({}));
    let blocking = wait_item(&env, "E2E-32", "question");
    // ... and a non-blocking one (a turn-done aviso of a second session).
    run_in_worktree(&env, "E2E-33", "Aviso", json!({}));
    wait_status(&env, "E2E-33", "waiting-for-user");
    let (code, session) = post(
        &env,
        "/api/v1/tasks/E2E-33/sessions",
        Some(json!({"kind": "claude", "prompt": "Otra"})),
    );
    assert!(code == 200 || code == 201, "{code} {session}");
    let aviso = wait_item(&env, "E2E-33", "turn-done");
    assert_eq!(aviso["session"]["id"], session["id"], "{aviso}");
    assert!(aviso["notice"].is_i64(), "{aviso}");

    let blocking_id = blocking["id"].as_str().unwrap().to_string();
    let aviso_id = aviso["id"].as_str().unwrap().to_string();
    for id in inbox_ids(&env) {
        assert!(id.contains(':'), "{id}");
    }

    // Dismiss: 204, repeat 204, blocking 409, unknown 404.
    let (code, body) = post(&env, &dismiss_uri(&blocking_id), None);
    assert_eq!(code, 409, "{body}");
    let (code, body) = post(&env, &dismiss_uri(&aviso_id), None);
    assert_eq!(code, 204, "{body}");
    assert!(!inbox_ids(&env).contains(&aviso_id));
    // A repeat is 204 as well.
    let (code, _) = post(&env, &dismiss_uri(&aviso_id), None);
    assert_eq!(code, 204, "a repeat dismiss");
    assert_eq!(post(&env, &dismiss_uri("question:NOPE-1:x:1"), None).0, 404);
    let (code, _) = post(
        &env,
        "/api/v1/inbox/restore",
        Some(json!({"ids": [aviso_id]})),
    );
    assert_eq!(code, 204);
    assert!(inbox_ids(&env).contains(&aviso_id));
    // A repeat dismiss of a listed item is 204 too.
    assert_eq!(post(&env, &dismiss_uri(&aviso_id), None).0, 204);
    assert_eq!(
        post(
            &env,
            "/api/v1/inbox/restore",
            Some(json!({"ids": [aviso_id]}))
        )
        .0,
        204
    );

    // Clear never returns a blocking id; restore brings the rest back.
    let (code, cleared) = post(&env, "/api/v1/inbox/clear", None);
    assert_eq!(code, 200, "{cleared}");
    let cleared: Vec<String> = items(&cleared["ids"])
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(cleared.contains(&aviso_id), "{cleared:?}");
    assert!(!cleared.contains(&blocking_id), "{cleared:?}");
    let left = inbox_ids(&env);
    assert!(
        left.contains(&blocking_id) && !left.contains(&aviso_id),
        "{left:?}"
    );
    let (code, _) = post(&env, "/api/v1/inbox/restore", Some(json!({"ids": cleared})));
    assert_eq!(code, 204);
    assert!(inbox_ids(&env).contains(&aviso_id));

    // Restore: unknown ids are ignored, more than 500 are a 400.
    assert_eq!(
        post(
            &env,
            "/api/v1/inbox/restore",
            Some(json!({"ids": ["nope:x:y:1"]}))
        )
        .0,
        204
    );
    let many: Vec<String> = (0..501).map(|n| format!("k:t:s:{n}")).collect();
    assert_eq!(
        post(&env, "/api/v1/inbox/restore", Some(json!({"ids": many}))).0,
        400
    );
}

/// Opens PR 1 for the task's branch and waits for the daemon to link it.
fn open_linked_pr(env: &Env, key: &str) -> i64 {
    let branch = env.wait_for("the task's branch", T, || {
        let t = env.api().task(key);
        t["branch"].as_str().map(str::to_string).ok_or(t)
    });
    env.api().set_phase(key, "review");
    env.gh.open_pr(1, &branch);
    env.wait_for("the PR to be linked", T, || {
        let all = env.api().prs();
        items(&all)
            .iter()
            .find(|p| p["task"] == key)
            .cloned()
            .ok_or(all)
    });
    let (_, v) = get(env, &format!("/api/v1/tasks/{key}/prs"));
    let id = pr_id(&v);
    assert!(id > 0, "{v}");
    id
}

/// The id of the first PR in `GET /tasks/{key}/prs` (an object with `prs`, or a list).
fn pr_id(v: &Value) -> i64 {
    let list = v.get("prs").unwrap_or(v);
    items(list)
        .first()
        .and_then(|p| p["id"].as_i64())
        .unwrap_or(0)
}

/// Writes the repository merge flags into the fake PR snapshot.
fn allow_merge_methods(env: &Env, n: i64, merge: bool, squash: bool) {
    let path = env.gh.dir.join(format!("pr-{n}.json"));
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    v["data"]["repository"]["mergeCommitAllowed"] = json!(merge);
    v["data"]["repository"]["squashMergeAllowed"] = json!(squash);
    v["data"]["repository"]["rebaseMergeAllowed"] = json!(false);
    std::fs::write(&path, serde_json::to_vec_pretty(&v).unwrap()).unwrap();
}

fn edit_pr(env: &Env, n: i64, f: impl FnOnce(&mut Value)) {
    let path = env.gh.dir.join(format!("pr-{n}.json"));
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    f(&mut v["data"]["repository"]["pullRequest"]);
    std::fs::write(&path, serde_json::to_vec_pretty(&v).unwrap()).unwrap();
}

fn set_user(env: &Env, login: &str) {
    std::fs::write(
        env.gh.dir.join("user.json"),
        json!({"login": login}).to_string(),
    )
    .unwrap();
}

fn refresh_prs(env: &Env, key: &str) -> Value {
    let (code, v) = post(
        env,
        &format!("/api/v1/tasks/{key}/prs/refresh"),
        Some(json!({})),
    );
    assert_eq!(code, 202, "{v}");
    v
}

fn merge(env: &Env, key: &str, id: i64, body: Option<Value>) -> (u16, Value) {
    post(env, &format!("/api/v1/tasks/{key}/prs/{id}/merge"), body)
}

#[test]
fn the_pr_snapshot_carries_times_comments_and_methods_and_merge_refuses_by_reason() {
    let env = Env::start(EnvOptions::default());
    env.register_fixtures();
    run_in_worktree(&env, "E2E-34", "Merge", json!({}));
    // The account of the PR tab is not known until someone looks: signed out at first.
    env.gh.sign_out();
    let id = open_linked_pr(&env, "E2E-34");

    // Never read: no account shown yet, the account is looked up fresh and found.
    set_user(&env, GH_USER);
    let (code, body) = merge(&env, "E2E-34", id, None);
    assert_eq!(code, 409, "{body}");
    assert_eq!(body["reason"], "account-unknown", "{body}");

    // The PR tab shows the account; the snapshot has times, review comments, methods.
    allow_merge_methods(&env, 1, true, true);
    edit_pr(&env, 1, |pr| {
        pr["commits"]["nodes"][0]["commit"]["statusCheckRollup"] = json!({"contexts": {"nodes": [
            {"__typename": "CheckRun", "name": "ci", "status": "IN_PROGRESS", "conclusion": null,
             "detailsUrl": "https://ci.invalid/1", "startedAt": "2026-10-20T10:00:00Z",
             "completedAt": null},
            {"__typename": "CheckRun", "name": "lint", "status": "COMPLETED",
             "conclusion": "SUCCESS", "detailsUrl": "https://ci.invalid/2",
             "startedAt": "2026-10-20T10:00:00Z", "completedAt": "2026-10-20T10:05:00Z"},
        ]}});
        pr["reviewThreads"] = json!({"nodes": [{"comments": {"nodes": [
            {"databaseId": 9, "author": {"login": "ana"}, "path": "src/a.rs", "line": 12,
             "body": "rename", "createdAt": "2026-10-20T11:00:00Z"}]}}]});
    });
    refresh_prs(&env, "E2E-34");
    let snap = env.wait_for("the snapshot with its checks", T, || {
        let (_, v) = get(&env, "/api/v1/tasks/E2E-34/prs");
        let list = v.get("prs").unwrap_or(&v).clone();
        let s = items(&list)[0]["snapshot"].clone();
        if s["checks"].as_array().is_some_and(|c| c.len() == 2) {
            Ok((v.clone(), s))
        } else {
            Err(v)
        }
    });
    let (tab, snap) = snap;
    assert_eq!(tab["account"], GH_USER, "{tab}");
    let checks = snap["checks"].as_array().unwrap();
    let ci = checks.iter().find(|c| c["name"] == "ci").unwrap();
    assert_eq!(ci["outcome"], "pending", "{ci}");
    assert!(
        ci["started_at"].is_string() && ci["finished_at"].is_null(),
        "{ci}"
    );
    let lint = checks.iter().find(|c| c["name"] == "lint").unwrap();
    assert!(
        lint["started_at"].is_string() && lint["finished_at"].is_string(),
        "{lint}"
    );
    let rc = &snap["review_comments"][0];
    assert_eq!(
        (
            rc["author"].as_str(),
            rc["path"].as_str(),
            rc["line"].as_i64()
        ),
        (Some("ana"), Some("src/a.rs"), Some(12)),
        "{snap}"
    );
    assert!(rc["body"] == "rename" && rc["at"].is_string(), "{rc}");
    assert_eq!(snap["merge_methods"], json!(["merge", "squash"]), "{snap}");

    // Refused by reason: checks pending, a method the repo does not allow.
    let (code, body) = merge(&env, "E2E-34", id, None);
    assert_eq!(
        (code, body["reason"].as_str()),
        (409, Some("checks")),
        "{body}"
    );

    env.gh.set_checks(1, Checks::Passing);
    refresh_prs(&env, "E2E-34");
    env.wait_for("passing checks", T, || {
        let (_, v) = get(&env, "/api/v1/tasks/E2E-34/prs");
        let list = v.get("prs").unwrap_or(&v).clone();
        if items(&list)[0]["snapshot"]["checks"][0]["outcome"] == "success" {
            Ok(())
        } else {
            Err(v)
        }
    });
    let (code, body) = merge(&env, "E2E-34", id, Some(json!({"method": "rebase"})));
    assert_eq!(
        (code, body["reason"].as_str()),
        (409, Some("method-not-allowed")),
        "{body}"
    );

    // The account changed since the tab showed it.
    set_user(&env, "someone-else");
    let (code, body) = merge(&env, "E2E-34", id, None);
    assert_eq!(
        (code, body["reason"].as_str()),
        (409, Some("account-changed")),
        "{body}"
    );
    // Back to the first account: the merge remembers the one it read last, so it still
    // says «changed» once, then agrees.
    set_user(&env, GH_USER);
    let (code, body) = merge(&env, "E2E-34", id, None);
    assert_eq!(
        (code, body["reason"].as_str()),
        (409, Some("account-changed")),
        "{body}"
    );

    // A draft is refused.
    edit_pr(&env, 1, |pr| pr["isDraft"] = json!(true));
    refresh_prs(&env, "E2E-34");
    env.wait_for("the draft snapshot", T, || {
        let (_, v) = get(&env, "/api/v1/tasks/E2E-34/prs");
        let list = v.get("prs").unwrap_or(&v).clone();
        if items(&list)[0]["snapshot"]["draft"] == true {
            Ok(())
        } else {
            Err(v)
        }
    });
    let (code, body) = merge(&env, "E2E-34", id, None);
    assert_eq!(
        (code, body["reason"].as_str()),
        (409, Some("draft")),
        "{body}"
    );

    // Merged or not, an unknown PR and an unknown task are a 404.
    assert_eq!(merge(&env, "E2E-34", 99999, None).0, 404);
    assert_eq!(merge(&env, "NOPE-1", 1, None).0, 404);
}

/// The merge goes through the fake `gh` (`scripts/fake-gh.sh` reads the PR number from the URL).
#[test]
fn a_passing_pr_merges_through_the_fake_gh() {
    let env = Env::start(EnvOptions::default());
    env.register_fixtures();
    run_in_worktree(&env, "E2E-40", "Merge ok", json!({}));
    let id = open_linked_pr(&env, "E2E-40");
    allow_merge_methods(&env, 1, true, true);
    env.gh.set_checks(1, Checks::Passing);
    refresh_prs(&env, "E2E-40");
    env.wait_for("the snapshot with its methods", T, || {
        let (_, v) = get(&env, "/api/v1/tasks/E2E-40/prs");
        let list = v.get("prs").unwrap_or(&v).clone();
        if items(&list)[0]["snapshot"]["merge_methods"] == json!(["merge", "squash"]) {
            Ok(())
        } else {
            Err(v)
        }
    });
    let (code, body) = merge(&env, "E2E-40", id, Some(json!({"method": "squash"})));
    assert_eq!(code, 200, "{body}");
    assert_eq!(body["snapshot"]["state"], "merged", "{body}");
    let (code, body) = merge(&env, "E2E-40", id, None);
    assert_eq!(code, 409, "{body}");
}

#[test]
fn pr_opened_and_turn_done_avisos_come_with_ids_and_notices() {
    let env = Env::start(EnvOptions::default());
    env.register_fixtures();
    run_in_worktree(&env, "E2E-35", "Avisos", json!({}));
    wait_status(&env, "E2E-35", "waiting-for-user");

    let branch = env.wait_for("the branch", T, || {
        let t = env.api().task("E2E-35");
        t["branch"].as_str().map(str::to_string).ok_or(t)
    });
    env.gh.open_pr(1, &branch);
    let (code, linked) = post(
        &env,
        "/api/v1/tasks/E2E-35/prs/link",
        Some(json!({"url": FakeGh::pr_url(1)})),
    );
    assert_eq!(code, 200, "{linked}");
    let opened = wait_item(&env, "E2E-35", "pr-opened");
    assert!(opened["id"].is_string(), "{opened}");
    assert_eq!(opened["pr"]["url"], FakeGh::pr_url(1), "{opened}");
    assert!(
        opened["notice"].is_i64() || opened["pr"]["notice"].is_i64(),
        "{opened}"
    );

    // The old route and the new one share one record of dismissals.
    let notice = opened["pr"]["notice"].as_i64().unwrap();
    assert_eq!(
        post(&env, &format!("/api/v1/pr-notices/{notice}/dismiss"), None).0,
        204
    );
    assert!(inbox_item(&env, "E2E-35", "pr-opened").is_none());
    let id = opened["id"].as_str().unwrap();
    assert_eq!(
        post(&env, "/api/v1/inbox/restore", Some(json!({"ids": [id]}))).0,
        204
    );
    assert!(inbox_item(&env, "E2E-35", "pr-opened").is_some());

    // A second Claude session that ends its turn raises a turn-done aviso.
    let (code, session) = post(
        &env,
        "/api/v1/tasks/E2E-35/sessions",
        Some(json!({"kind": "claude", "prompt": "Otra"})),
    );
    assert!(code == 200 || code == 201, "{code} {session}");
    let done = wait_item(&env, "E2E-35", "turn-done");
    assert_eq!(done["session"]["id"], session["id"], "{done}");
    assert!(done["notice"].is_i64() && done["id"].is_string(), "{done}");
}

#[test]
fn a_failed_setup_has_its_info_and_log() {
    let env = Env::start(EnvOptions::default());
    env.register_fixtures();
    let (code, body) = env.api().raw(
        Method::PUT,
        "/api/v1/projects/signal/settings",
        Some(json!({"setup": "echo installing deps; exit 3"})),
    );
    assert_eq!(code, 200, "{body}");
    // Nothing ran yet.
    let (code, _) = get(&env, "/api/v1/tasks/E2E-36/setup");
    assert_eq!(code, 404);

    run_in_worktree(&env, "E2E-36", "Setup", json!({}));
    wait_status(&env, "E2E-36", "setup-failed");
    let (code, info) = get(&env, "/api/v1/tasks/E2E-36/setup");
    assert_eq!(code, 200, "{info}");
    assert_eq!(info["exit_code"], 3, "{info}");
    assert_eq!(info["command"], "echo installing deps; exit 3", "{info}");
    assert_eq!(
        (info["cancelled"].as_bool(), info["timed_out"].as_bool()),
        (Some(false), Some(false))
    );
    assert!(
        info["started_at"].is_string() && info["finished_at"].is_string(),
        "{info}"
    );
    assert!(info["duration_ms"].is_u64(), "{info}");
    assert!(
        info["tail"].as_str().unwrap().contains("installing deps"),
        "{info}"
    );
    assert!(info["log_bytes"].as_u64().unwrap() > 0, "{info}");

    let (code, log) = get(&env, "/api/v1/tasks/E2E-36/setup/log");
    assert_eq!(code, 200);
    assert!(log.to_string().contains("installing deps"), "{log}");
}

#[test]
fn jira_subtasks_and_counts_come_from_the_sample() {
    let env = Env::start(EnvOptions::default());
    // Not connected: a 409, not a 404.
    let (code, body) = get(&env, "/api/v1/jira/issues/PROJ-1242/subtasks");
    assert_eq!(code, 409, "{body}");

    let (code, body) = env
        .api()
        .raw(Method::PUT, "/api/v1/jira", Some(json!({"mode": "sample"})));
    assert_eq!(code, 200, "{body}");
    let (code, kids) = get(&env, "/api/v1/jira/issues/PROJ-1242/subtasks");
    assert_eq!(code, 200, "{kids}");
    let keys: Vec<&str> = items(&kids)
        .iter()
        .map(|i| i["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["PROJ-1250", "PROJ-1251"]);
    assert_eq!(
        get(&env, "/api/v1/jira/issues/PROJ-1238/subtasks").1,
        json!([])
    );
    assert_eq!(get(&env, "/api/v1/jira/issues/NOPE-1/subtasks").0, 404);

    let (_, parent) = get(&env, "/api/v1/jira/issues/PROJ-1242");
    assert_eq!(
        (
            parent["subtask_count"].as_i64(),
            parent["subtasks_done"].as_i64()
        ),
        (Some(2), Some(0)),
        "{parent}"
    );
    let (_, story) = get(&env, "/api/v1/jira/issues/PROJ-1246");
    assert_eq!(
        (
            story["subtask_count"].as_i64(),
            story["subtasks_done"].as_i64()
        ),
        (Some(4), Some(1)),
        "{story}"
    );
    let listed = env.api().jira_issues("assigned", "open", "all");
    assert!(
        items(&listed)
            .iter()
            .all(|i| i.get("subtask_count").is_some()),
        "{listed}"
    );
}

#[test]
fn a_sessions_runtime_switches_alone_and_the_task_route_clears_it() {
    let env = Env::start(EnvOptions::default());
    env.register_fixtures();
    env.scenario(
        "busy",
        &json!({"turns": [[
            {"tool": {"name": "Bash", "input": {"command": "ls"}, "output": "x"}},
            {"sleep_ms": 3000},
            {"say": {"text": "ok"}},
        ]]}),
    );
    run_in_worktree(&env, "E2E-37", "Runtime", json!({"chat_mode": true}));
    let first = env.wait_for("the first turn to end", T, || {
        let s = session_of(&env, "E2E-37");
        if status(&env, "E2E-37") == "waiting-for-user" && s["runtime"] == "stream" {
            Ok(s)
        } else {
            Err(s)
        }
    });
    let id = first["id"].as_str().unwrap().to_string();
    assert_eq!(first["effective_runtime"], "stream", "{first}");
    assert!(first["runtime_override"].is_null(), "{first}");
    let uri = format!("/api/v1/sessions/{id}/runtime");

    assert_eq!(post(&env, &uri, Some(json!({"mode": "nope"}))).0, 400);
    assert_eq!(
        post(
            &env,
            "/api/v1/sessions/nope/runtime",
            Some(json!({"mode": "chat"}))
        )
        .0,
        404
    );
    // Already in that runtime.
    let (code, body) = post(&env, &uri, Some(json!({"mode": "chat"})));
    assert_eq!(code, 409, "{body}");

    let heard = {
        let api = env.api();
        let (tx, rx) = std::sync::mpsc::channel();
        let listener = std::thread::spawn(move || {
            api.events_when_connected(6000, move || {
                let _ = tx.send(());
            })
        });
        rx.recv_timeout(T).expect("the events socket");
        let (code, body) = post(&env, &uri, Some(json!({"mode": "terminal"})));
        assert_eq!(code, 204, "{body}");
        listener.join().unwrap()
    };
    assert!(heard.iter().any(|t| t == "sessions"), "{heard:?}");
    let s = env.wait_for("the session to run in a terminal", T, || {
        let s = session_of(&env, "E2E-37");
        if s["runtime_override"] == "pty" && s["effective_runtime"] == "pty" {
            Ok(s)
        } else {
            Err(s)
        }
    });
    assert_eq!(s["id"], id.as_str(), "{s}");
    let switched: Vec<_> = {
        let (_, evs) = get(&env, "/api/v1/tasks/E2E-37/events");
        items(&evs)
            .iter()
            .filter(|e| e.to_string().contains("runtime-switched"))
            .cloned()
            .collect()
    };
    assert!(!switched.is_empty(), "no runtime-switched event");
    assert!(switched[0].to_string().contains(&id), "{switched:?}");

    // The task's route puts every session back on the task's runtime.
    let (code, body) = post(
        &env,
        "/api/v1/tasks/E2E-37/runtime",
        Some(json!({"mode": "chat"})),
    );
    assert_eq!(code, 204, "{body}");
    let s = env.wait_for("the override to be cleared", T, || {
        let s = session_of(&env, "E2E-37");
        if s["runtime_override"].is_null() && s["effective_runtime"] == "stream" {
            Ok(s)
        } else {
            Err(s)
        }
    });
    assert_eq!(s["id"], id.as_str(), "{s}");
}

#[test]
fn a_session_with_a_turn_running_refuses_the_switch() {
    let env = Env::start(EnvOptions::default());
    env.register_fixtures();
    env.scenario(
        "busy2",
        &json!({"turns": [[
            {"tool": {"name": "Bash", "input": {"command": "ls"}, "output": "x", "auto": "ask"}},
            {"sleep_ms": 3000},
            {"say": {"text": "ok"}},
        ]]}),
    );
    run_in_worktree(
        &env,
        "E2E-38",
        "Ocupada [fake:busy2]",
        json!({"chat_mode": true}),
    );
    // The turn waits on a permission: it is busy.
    let item = wait_item(&env, "E2E-38", "permission");
    let id = item["session"]["id"].as_str().unwrap();
    let (code, body) = post(
        &env,
        &format!("/api/v1/sessions/{id}/runtime"),
        Some(json!({"mode": "terminal"})),
    );
    assert_eq!(code, 409, "{body}");
}

#[test]
fn health_has_the_home_and_a_run_without_a_choice_works_in_the_checkout() {
    let env = Env::start(EnvOptions::default());
    let health = env.api().health();
    let home = health["home"]
        .as_str()
        .unwrap_or_else(|| panic!("no home: {health}"));
    assert!(
        home.starts_with('/') && std::path::Path::new(home).exists(),
        "{health}"
    );

    env.register_fixtures();
    env.api().start_run(json!({
        "key": "E2E-39", "title": "Checkout", "project": "signal", "prompt": "Checkout",
    }));
    wait_status(&env, "E2E-39", "waiting-for-user");
    let task = env.api().task("E2E-39");
    // The task's branch is checked out in the project's own checkout.
    let head = std::process::Command::new("git")
        .arg("-C")
        .arg(&env.fixtures.repo)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .expect("git rev-parse");
    assert_eq!(
        String::from_utf8_lossy(&head.stdout).trim(),
        task["branch"].as_str().expect("a branch"),
        "{task}"
    );
    assert!(
        !env.root.join("delilah/workspaces/E2E-39").exists(),
        "a worktree was made"
    );
}
