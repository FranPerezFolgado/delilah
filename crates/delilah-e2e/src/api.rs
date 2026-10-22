//! A typed-enough client for the daemon's HTTP API (`crates/delilah-daemon/src/api.rs`,
//! shapes in `app/src/api.ts`). Bodies are `serde_json::Value`; panics carry the route.

use std::net::TcpStream;
use std::time::{Duration, Instant};

use reqwest::Method;
use reqwest::blocking::Client;
use serde_json::{Value, json};

pub struct Api {
    base: String,
    http: Client,
}

/// Panics unless `v` is an array; returns its items.
pub fn items(v: &Value) -> &[Value] {
    v.as_array().unwrap_or_else(|| panic!("expected a JSON array, got {v}"))
}

/// `task["status"]["type"]` (the status is an object); a plain string also works.
pub fn status_of(task: &Value) -> &str {
    let s = &task["status"];
    s["type"].as_str().or_else(|| s.as_str()).unwrap_or("")
}

/// `task["phase"]`, empty if none.
pub fn phase_of(task: &Value) -> &str {
    task["phase"].as_str().unwrap_or("")
}

impl Api {
    pub fn new(base: &str) -> Api {
        Api {
            base: base.trim_end_matches('/').to_string(),
            http: Client::builder().timeout(Duration::from_secs(20)).build().expect("http client"),
        }
    }

    /// Sends the request; returns (status code, body as JSON or Null).
    pub fn raw(&self, method: Method, path: &str, body: Option<Value>) -> (u16, Value) {
        let mut req = self.http.request(method.clone(), format!("{}{path}", self.base));
        if let Some(b) = body {
            req = req.json(&b);
        }
        let res = req.send().unwrap_or_else(|e| panic!("{method} {path}: {e}"));
        let code = res.status().as_u16();
        let text = res.text().unwrap_or_default();
        let v = serde_json::from_str(&text).unwrap_or(if text.is_empty() {
            Value::Null
        } else {
            Value::String(text)
        });
        (code, v)
    }

    /// Like `raw`, but panics unless the answer is 2xx.
    fn ok(&self, method: Method, path: &str, body: Option<Value>) -> Value {
        let (code, v) = self.raw(method.clone(), path, body);
        assert!((200..300).contains(&code), "{method} {path}: HTTP {code} {v}");
        v
    }

    fn get(&self, path: &str) -> Value {
        self.ok(Method::GET, path, None)
    }

    fn post(&self, path: &str, body: Value) -> Value {
        self.ok(Method::POST, path, Some(body))
    }

    pub fn health(&self) -> Value {
        self.get("/api/v1/health")
    }

    pub fn projects(&self) -> Value {
        self.get("/api/v1/projects")
    }

    /// `kind` is "repo" or "parent".
    pub fn add_project(&self, path: &str, kind: &str, name: Option<&str>) -> Value {
        self.post("/api/v1/projects", json!({"path": path, "kind": kind, "name": name}))
    }

    pub fn inspect_project(&self, path: &str) -> Value {
        self.post("/api/v1/projects/inspect", json!({"path": path}))
    }

    /// `body` is a `NewTask`: `{key?, title, project?, notes?, free?, ...}`.
    pub fn new_task(&self, body: Value) -> Value {
        self.post("/api/v1/tasks", body)
    }

    /// `body` is a `RunRequest`: `{key, title?, project?, submodules, prompt?, ...}`.
    pub fn start_run(&self, body: Value) -> Value {
        self.post("/api/v1/runs", body)
    }

    pub fn tasks(&self) -> Value {
        self.get("/api/v1/tasks")
    }

    pub fn task(&self, key: &str) -> Value {
        self.get(&format!("/api/v1/tasks/{key}"))
    }

    pub fn set_phase(&self, key: &str, phase: &str) -> Value {
        self.post(&format!("/api/v1/tasks/{key}/phase"), json!({"phase": phase}))
    }

    pub fn done(&self, key: &str) -> Value {
        self.post(&format!("/api/v1/tasks/{key}/done"), json!({}))
    }

    pub fn archive(&self, key: &str) -> Value {
        self.post(&format!("/api/v1/tasks/{key}/archive"), json!({}))
    }

    pub fn delete_task(&self, key: &str) -> Value {
        self.ok(Method::DELETE, &format!("/api/v1/tasks/{key}"), None)
    }

    pub fn resume(&self, key: &str) -> Value {
        self.post(&format!("/api/v1/tasks/{key}/resume"), json!({}))
    }

    pub fn inbox(&self) -> Value {
        self.get("/api/v1/inbox")
    }

    /// `answers`: question text → chosen label (or own words).
    pub fn answer_ask(&self, id: &str, answers: Value) -> Value {
        self.post(&format!("/api/v1/asks/{id}/answer"), json!({"answers": answers}))
    }

    pub fn release_ask(&self, id: &str) -> Value {
        self.post(&format!("/api/v1/asks/{id}/release"), json!({}))
    }

    pub fn sessions(&self) -> Value {
        self.get("/api/v1/sessions")
    }

    pub fn reply(&self, session: &str, text: &str) -> Value {
        self.post(&format!("/api/v1/sessions/{session}/reply"), json!({"text": text}))
    }

    /// Starts an ended session again (Claude with `--resume`).
    pub fn reopen_session(&self, session: &str) -> Value {
        self.post(&format!("/api/v1/sessions/{session}/reopen"), json!({}))
    }

    /// Closes a live session (ends the Claude process).
    pub fn close_session(&self, session: &str) -> Value {
        self.post(&format!("/api/v1/sessions/{session}/close"), json!({}))
    }

    /// Opens a Claude session in the project's checkout; `conversation` makes it an idea
    /// conversation. Returns the `Session`.
    pub fn open_project_session(&self, project: &str, conversation: bool) -> Value {
        self.post(
            &format!("/api/v1/projects/{project}/sessions"),
            json!({"kind": "claude", "conversation": conversation}),
        )
    }

    /// Writes a message into a conversation session.
    pub fn send_message(&self, session: &str, text: &str) -> Value {
        self.post(&format!("/api/v1/sessions/{session}/messages"), json!({"text": text}))
    }

    pub fn conversation(&self, session: &str) -> Value {
        self.get(&format!("/api/v1/sessions/{session}/conversation"))
    }

    pub fn prs(&self) -> Value {
        self.get("/api/v1/prs")
    }

    /// `PUT /api/v1/jira` with a `Connect` body; `site` is the fake Jira's URL.
    pub fn jira_configure(&self, site: &str) -> Value {
        self.ok(
            Method::PUT,
            "/api/v1/jira",
            Some(json!({"mode": "cloud", "site": site, "email": "e2e@example.invalid",
                "token": "not-a-token", "jql": ""})),
        )
    }

    /// `who`: assigned|reported|query; `progress`: open|done; `sprint`: all|current.
    pub fn jira_issues(&self, who: &str, progress: &str, sprint: &str) -> Value {
        self.get(&format!("/api/v1/jira/issues?who={who}&progress={progress}&sprint={sprint}"))
    }

    /// Listens on the `/api/v1/events` WebSocket for `ms` milliseconds and returns the
    /// topic names received (lowercase, as sent: `["tasks", "inbox"]`), in order. Bounded: at most
    /// `ms` plus the connect time.
    pub fn events(&self, ms: u64) -> Vec<String> {
        self.events_when_connected(ms, || {})
    }

    /// Like [`Api::events`], calling `connected` once the WebSocket handshake is done.
    pub fn events_when_connected(&self, ms: u64, connected: impl FnOnce()) -> Vec<String> {
        let addr = self.base.trim_start_matches("http://");
        let stream = TcpStream::connect(addr).unwrap_or_else(|e| panic!("events {addr}: {e}"));
        let url = format!("ws://{addr}/api/v1/events");
        let (mut ws, _) = tungstenite::client(url, stream)
            .unwrap_or_else(|e| panic!("events handshake: {e}"));
        connected();
        let deadline = Instant::now() + Duration::from_millis(ms);
        let mut topics = Vec::new();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            ws.get_ref().set_read_timeout(Some(left.max(Duration::from_millis(1)))).ok();
            match ws.read() {
                Ok(tungstenite::Message::Text(t)) => {
                    if let Ok(v) = serde_json::from_str::<Value>(t.as_str())
                        && let Some(name) = v["type"].as_str()
                    {
                        topics.push(name.to_string());
                    }
                }
                Ok(tungstenite::Message::Close(_)) => break,
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => break,
            }
        }
        let _ = ws.close(None);
        topics
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn status_of_reads_the_type_of_the_status_object() {
        assert_eq!(status_of(&json!({"status": {"type": "running"}})), "running");
        assert_eq!(status_of(&json!({"status": "done"})), "done");
        assert_eq!(status_of(&json!({})), "");
    }
}

#[cfg(all(test, feature = "e2e"))]
mod tests {
    use super::*;
    use crate::{Env, EnvOptions};

    #[test]
    fn health_is_ok_against_a_real_env() {
        let env = Env::start(EnvOptions::default());
        assert!(env.api().health()["version"].is_string());
        assert!(items(&env.api().tasks()).is_empty());
    }

    #[test]
    fn events_report_a_change_and_jira_issues_come_from_the_fake() {
        let mut env = Env::start(EnvOptions { jira: true, ..Default::default() });
        let url = env.start_jira();
        let api = env.api();
        env.register_fixtures();
        let connected = api.jira_configure(&url);
        assert!(connected["account"].is_string(), "{connected}");
        assert!(!items(&api.jira_issues("assigned", "open", "all")).is_empty());
        let a2 = Api::new(&env.api);
        let (tx, rx) = std::sync::mpsc::channel();
        let t = std::thread::spawn(move || {
            a2.events_when_connected(3000, move || {
                let _ = tx.send(());
            })
        });
        rx.recv_timeout(Duration::from_secs(10)).expect("the events socket to connect");
        api.new_task(json!({"key": "E2E-1", "title": "t", "free": true}));
        let topics = t.join().unwrap();
        assert!(topics.iter().any(|t| t == "tasks"), "{topics:?}");
    }
}
