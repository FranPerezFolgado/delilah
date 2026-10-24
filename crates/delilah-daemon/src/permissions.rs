//! Permissions a chat-mode (stream) agent asks for, answered from Delilah.
//!
//! Unlike a question (`asks.rs`) nothing blocks here: the agent's `can_use_tool` request
//! arrives as a stream event, is kept as a [`PermissionAsk`] and the agent waits for the
//! control response that [`answer`] (or a release) writes later, through the session's
//! stream. A request that matches one of the task's allowed rules (`tasks.allowed_tools`,
//! the ones "always allow" appended) is answered at once and never shown.
//!
//! Rules are written as Claude writes them: `Tool`, or `Bash(<prefix>:*)`.

use std::collections::HashMap;
use std::sync::Mutex;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use delilah_agents::stream::Decision;
use delilah_core::TaskKey;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::ApiError;
use crate::events::Topic;
use crate::inbox::{Attention, AttentionKind};
use crate::sessions::LiveState;
use crate::state::AppState;

/// What the agent is told when you deny without a reason.
pub const DEFAULT_DENIAL: &str = "El usuario lo ha denegado";
/// What it is told when the session is stopped, killed or closed with the ask pending.
pub const STOPPED_DENIAL: &str = "sesión detenida";
/// A reason longer than this is cut.
const MAX_REASON: usize = 1000;
const MAX_SUMMARY: usize = 300;

#[derive(Debug, Clone, Serialize)]
pub struct PermissionAsk {
    pub id: String,
    /// The agent's own id for the request: what the control response answers.
    #[serde(skip)]
    pub request_id: String,
    pub task: String,
    pub session: String,
    pub tool: String,
    #[serde(skip)]
    pub input: Value,
    /// What "always allow" would add: Claude's `permission_suggestions`, as sent.
    pub suggestion: Option<Value>,
    /// A line for a card or an inbox item: the command, the file, the URL.
    pub summary: String,
    pub at: DateTime<Utc>,
}

#[derive(Default)]
pub struct Permissions {
    pending: Mutex<HashMap<String, PermissionAsk>>,
}

impl Permissions {
    /// Oldest first.
    pub fn list(&self) -> Vec<PermissionAsk> {
        let mut asks: Vec<PermissionAsk> = self.pending.lock().unwrap().values().cloned().collect();
        asks.sort_by_key(|a| a.at);
        asks
    }

    pub fn get(&self, id: &str) -> Option<PermissionAsk> {
        self.pending.lock().unwrap().get(id).cloned()
    }

    /// Whether the session has a permission waiting for you.
    pub fn has_session(&self, session: &str) -> bool {
        self.pending
            .lock()
            .unwrap()
            .values()
            .any(|a| a.session == session)
    }

    fn insert(&self, ask: PermissionAsk) {
        self.pending.lock().unwrap().insert(ask.id.clone(), ask);
    }

    /// Takes the ask off the list, so only one answer can ever act on it.
    fn take(&self, id: &str) -> Option<PermissionAsk> {
        self.pending.lock().unwrap().remove(id)
    }

    fn take_where(&self, f: impl Fn(&PermissionAsk) -> bool) -> Vec<PermissionAsk> {
        let mut pending = self.pending.lock().unwrap();
        let ids: Vec<String> = pending
            .values()
            .filter(|a| f(a))
            .map(|a| a.id.clone())
            .collect();
        ids.iter().filter_map(|id| pending.remove(id)).collect()
    }
}

/// Lets go of every permission of one session (stopped, killed, closed, switched to the
/// terminal): the agent, if it still listens, is told "sesión detenida". Returns how many.
pub async fn release_session(app: &AppState, session: &str) -> usize {
    let asks = app.permissions.take_where(|a| a.session == session);
    deny_all(app, asks).await
}

/// The same for every session of a task (it is being deleted).
pub async fn release_task(app: &AppState, task: &str) -> usize {
    let asks = app.permissions.take_where(|a| a.task == task);
    deny_all(app, asks).await
}

async fn deny_all(app: &AppState, asks: Vec<PermissionAsk>) -> usize {
    for ask in &asks {
        // Best effort: the process may be gone already.
        let _ = reply(
            app,
            &ask.session,
            &ask.request_id,
            Decision::Deny {
                message: STOPPED_DENIAL.into(),
            },
        )
        .await;
        clear_attention(app, &ask.session).await;
    }
    if !asks.is_empty() {
        app.emit(Topic::Inbox);
        app.emit(Topic::Conversations);
    }
    asks.len()
}

/// Writes the answer to the agent through the session's stream.
async fn reply(
    app: &AppState,
    session: &str,
    request_id: &str,
    decision: Decision,
) -> Result<(), String> {
    let live = app
        .sessions
        .get(session)
        .ok_or("the session is not running")?;
    let stream = live
        .runtime
        .as_stream()
        .ok_or("the session is not in chat mode")?;
    stream
        .reply_permission(request_id, decision)
        .await
        .map_err(|e| e.to_string())
}

/// What the session waited on was a permission: it no longer is.
async fn clear_attention(app: &AppState, session: &str) {
    if let Ok(Some(a)) = app.store.attention(session).await
        && a.kind == AttentionKind::Permission
    {
        let _ = app.store.set_attention(session, None).await;
    }
}

/// A `can_use_tool` request arrived. Quick by design (the pump awaits it): it answers by
/// itself when a rule allows the call, else keeps the ask and returns.
pub async fn on_request(
    app: &AppState,
    task: &str,
    session: &str,
    request_id: String,
    tool: String,
    input: Value,
    suggestion: Option<Value>,
) {
    let rules = match app.store.get_task(&TaskKey::new(task)).await {
        Ok(Some(t)) => t.allowed_tools,
        _ => Vec::new(),
    };
    if rules.iter().any(|r| rule_matches(r, &tool, &input)) {
        if let Err(e) = reply(app, session, &request_id, Decision::Allow { input }).await {
            tracing::warn!(%session, %tool, "cannot answer an allowed permission: {e}");
        }
        return;
    }
    let ask = PermissionAsk {
        id: uuid::Uuid::new_v4().to_string(),
        request_id,
        task: task.to_string(),
        session: session.to_string(),
        summary: summary(&tool, &input),
        tool,
        input,
        suggestion,
        at: Utc::now(),
    };
    let text = if ask.summary.is_empty() {
        ask.tool.clone()
    } else {
        format!("{}: {}", ask.tool, ask.summary)
    };
    app.permissions.insert(ask);
    if let Some(live) = app.sessions.get(session) {
        live.set_state(LiveState::Waiting);
    }
    let attention = Attention {
        kind: AttentionKind::Permission,
        text,
        since: Utc::now(),
    };
    let _ = app.store.set_attention(session, Some(&attention)).await;
    app.emit(Topic::Inbox);
    app.emit(Topic::Conversations);
}

#[derive(Debug, Deserialize)]
pub struct Answer {
    /// `allow_once`, `allow_always` or `deny`.
    pub decision: String,
    #[serde(default)]
    pub reason: Option<String>,
}

/// `POST /api/v1/sessions/{id}/permissions/{ask}`: 204; 404 when the ask is gone.
pub async fn answer(
    State(app): State<AppState>,
    Path((session, id)): Path<(String, String)>,
    Json(body): Json<Answer>,
) -> Result<StatusCode, ApiError> {
    let gone = || ApiError::not_found("that permission is no longer waiting");
    if !matches!(
        body.decision.as_str(),
        "allow_once" | "allow_always" | "deny"
    ) {
        return Err(ApiError::bad_request(
            "decision is allow_once, allow_always or deny",
        ));
    }
    if app
        .permissions
        .get(&id)
        .is_none_or(|a| a.session != session)
    {
        return Err(gone());
    }
    let ask = app.permissions.take(&id).ok_or_else(gone)?;
    let decision = if body.decision == "deny" {
        let reason = body.reason.as_deref().map(str::trim).unwrap_or_default();
        Decision::Deny {
            message: if reason.is_empty() {
                DEFAULT_DENIAL.into()
            } else {
                reason.chars().take(MAX_REASON).collect()
            },
        }
    } else {
        if body.decision == "allow_always" {
            for rule in rules_to_add(&ask.tool, ask.suggestion.as_ref()) {
                if let Err(e) = app
                    .store
                    .add_allowed_tool(&TaskKey::new(&ask.task), &rule)
                    .await
                {
                    tracing::warn!(task = %ask.task, "cannot remember the rule `{rule}`: {e}");
                }
            }
            app.emit(Topic::Tasks);
        }
        Decision::Allow {
            input: ask.input.clone(),
        }
    };
    let allowed = matches!(decision, Decision::Allow { .. });
    let result = reply(&app, &session, &ask.request_id, decision).await;
    clear_attention(&app, &session).await;
    if allowed && let Some(live) = app.sessions.get(&session) {
        live.set_state(LiveState::Working);
    }
    app.emit(Topic::Inbox);
    app.emit(Topic::Conversations);
    result.map_err(|e| ApiError::conflict(format!("cannot answer the agent: {e}")))?;
    Ok(StatusCode::NO_CONTENT)
}

/// The rules "always allow" appends: those of Claude's suggestion for this very tool,
/// else the tool's name. Trimmed, never empty.
pub fn rules_to_add(tool: &str, suggestion: Option<&Value>) -> Vec<String> {
    let mut rules: Vec<String> = Vec::new();
    for s in suggestion.and_then(Value::as_array).into_iter().flatten() {
        if s["type"] != "addRules" || s["behavior"] != "allow" {
            continue;
        }
        for r in s["rules"].as_array().into_iter().flatten() {
            if r["toolName"].as_str() != Some(tool) {
                continue;
            }
            let rule = match r["ruleContent"].as_str().map(str::trim) {
                Some(content) if !content.is_empty() => format!("{tool}({content})"),
                _ => tool.to_string(),
            };
            if !rules.contains(&rule) {
                rules.push(rule);
            }
        }
    }
    if rules.is_empty() {
        rules.push(tool.to_string());
    }
    rules.retain(|r| !r.trim().is_empty());
    rules
}

/// Whether an allow rule covers this call: `Tool` covers any use of the tool;
/// `Bash(<prefix>:*)` a command that starts with the prefix; `Bash(<command>)` exactly it.
pub fn rule_matches(rule: &str, tool: &str, input: &Value) -> bool {
    let rule = rule.trim();
    if rule.is_empty() {
        return false;
    }
    let Some((name, rest)) = rule.split_once('(') else {
        return rule == tool;
    };
    let Some(content) = rest.strip_suffix(')') else {
        return false;
    };
    if name != tool || tool != "Bash" {
        return false;
    }
    let Some(command) = input["command"].as_str().map(str::trim) else {
        return false;
    };
    match content.strip_suffix(":*") {
        Some(prefix) => {
            let prefix = prefix.trim();
            !prefix.is_empty()
                && command.strip_prefix(prefix).is_some_and(|tail| {
                    // The prefix is whole words, and nothing is chained after it.
                    (tail.is_empty() || tail.starts_with(char::is_whitespace))
                        && !tail.contains([';', '&', '|', '`', '$', '<', '>', '\n', '\r'])
                })
        }
        None => command == content.trim(),
    }
}

fn summary(tool: &str, input: &Value) -> String {
    let field = ["command", "file_path", "path", "url", "pattern", "query"]
        .iter()
        .find_map(|k| input[*k].as_str());
    let text = field.unwrap_or_default();
    let _ = tool;
    text.chars().take(MAX_SUMMARY).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::Live;
    use crate::store_sessions::SessionKind;
    use axum::body::Body;
    use axum::http::Request;
    use delilah_agents::stream::StreamEvent;
    use delilah_core::Task;
    use serde_json::json;
    use tower::ServiceExt;

    struct Fixture {
        app: AppState,
        live: Live,
        events: tokio::sync::broadcast::Receiver<StreamEvent>,
        topics: tokio::sync::broadcast::Receiver<Topic>,
    }

    impl Fixture {
        /// Task PROJ-1 with the primary chat-mode session "s-1" on the fake stream agent.
        async fn new() -> Self {
            let app = AppState::new(
                crate::store::Store::in_memory().await.unwrap(),
                crate::config::Config {
                    home: std::env::temp_dir()
                        .join(format!("delilah-perm-{}", uuid::Uuid::new_v4())),
                    addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
                    allow_file_protocol: false,
                    notify: false,
                    ui_origin: None,
                    agent_program: None,
                    claude_projects: crate::config::Config::scratch_projects(),
                    claude_bin: "claude".into(),
                },
            );
            let mut task = Task::new(TaskKey::new("PROJ-1"), "t", "feature");
            task.chat_mode = true;
            app.store.create_task(&task).await.unwrap();
            let script = concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../delilah-agents/tests/fixtures/fake-claude-stream.sh"
            );
            let mut cmd = tokio::process::Command::new("bash");
            cmd.arg(script);
            let (session, events) = delilah_agents::stream::StreamSession::spawn(cmd).unwrap();
            let live = app.sessions.insert(
                "s-1",
                "PROJ-1",
                SessionKind::Claude,
                true,
                std::sync::Arc::new(session),
            );
            let topics = app.events.subscribe();
            Self {
                app,
                live,
                events,
                topics,
            }
        }

        /// Makes the agent ask and feeds its request through `on_request`.
        async fn ask(&mut self) -> Option<PermissionAsk> {
            self.live
                .runtime
                .as_stream()
                .unwrap()
                .send_user("NEEDS_PERMISSION")
                .await
                .unwrap();
            loop {
                let event =
                    tokio::time::timeout(std::time::Duration::from_secs(10), self.events.recv())
                        .await
                        .expect("the agent asks")
                        .unwrap();
                if let StreamEvent::PermissionRequest {
                    request_id,
                    tool,
                    input,
                    suggestion,
                } = event
                {
                    on_request(
                        &self.app, "PROJ-1", "s-1", request_id, tool, input, suggestion,
                    )
                    .await;
                    return self.app.permissions.list().into_iter().next();
                }
            }
        }

        /// Reads events until the turn's result: the tool's text and the final one.
        async fn turn_text(&mut self) -> String {
            let mut seen = String::new();
            loop {
                let event =
                    tokio::time::timeout(std::time::Duration::from_secs(10), self.events.recv())
                        .await
                        .expect("the turn ends")
                        .unwrap();
                if let StreamEvent::TurnResult { result, .. } = event {
                    seen.push_str(&result.unwrap_or_default());
                    return seen;
                }
            }
        }

        async fn post(&self, uri: &str, body: Value) -> StatusCode {
            let req = Request::post(uri)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap();
            crate::api::router(self.app.clone())
                .oneshot(req)
                .await
                .unwrap()
                .status()
        }

        fn topics(&mut self) -> Vec<Topic> {
            let mut seen = Vec::new();
            while let Ok(t) = self.topics.try_recv() {
                seen.push(t);
            }
            seen
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = self.live.runtime.kill();
        }
    }

    #[tokio::test]
    async fn a_request_creates_an_ask_emits_and_waits_for_the_user() {
        let mut f = Fixture::new().await;
        f.live.set_state(LiveState::Working);
        let ask = f.ask().await.expect("an ask");
        assert_eq!(ask.tool, "Bash");
        assert_eq!(ask.summary, "npm --version");
        assert_eq!(ask.session, "s-1");
        assert_eq!(f.live.state(), LiveState::Waiting);
        let topics = f.topics();
        assert!(topics.contains(&Topic::Inbox) && topics.contains(&Topic::Conversations));
        assert!(f.app.permissions.has_session("s-1"));
    }

    #[tokio::test]
    async fn allow_once_lets_the_tool_run_with_its_input_and_remembers_nothing() {
        let mut f = Fixture::new().await;
        let ask = f.ask().await.unwrap();
        let status = f
            .post(
                &format!("/api/v1/sessions/s-1/permissions/{}", ask.id),
                json!({ "decision": "allow_once" }),
            )
            .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(f.turn_text().await, "allowed");
        let task = f.app.store.get_task(&TaskKey::new("PROJ-1")).await.unwrap();
        assert!(task.unwrap().allowed_tools.is_empty());
        // Answered once: a second answer finds nothing.
        let again = f
            .post(
                &format!("/api/v1/sessions/s-1/permissions/{}", ask.id),
                json!({ "decision": "allow_once" }),
            )
            .await;
        assert_eq!(again, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn allow_always_appends_the_rule_and_the_next_request_is_answered_at_once() {
        let mut f = Fixture::new().await;
        // The fake agent's suggestion has no rules: the tool's name is what is kept.
        let ask = f.ask().await.unwrap();
        f.post(
            &format!("/api/v1/sessions/s-1/permissions/{}", ask.id),
            json!({ "decision": "allow_always" }),
        )
        .await;
        assert_eq!(f.turn_text().await, "allowed");
        let task = f.app.store.get_task(&TaskKey::new("PROJ-1")).await.unwrap();
        assert_eq!(task.unwrap().allowed_tools, ["Bash"]);
        // Same request again: no ask, the agent goes on by itself.
        assert!(f.ask().await.is_none());
        assert_eq!(f.turn_text().await, "allowed");
    }

    #[tokio::test]
    async fn deny_replies_with_the_reason_or_the_default() {
        let mut f = Fixture::new().await;
        let ask = f.ask().await.unwrap();
        let status = f
            .post(
                &format!("/api/v1/sessions/s-1/permissions/{}", ask.id),
                json!({ "decision": "deny", "reason": "  no  " }),
            )
            .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(f.turn_text().await, "denied");
        assert!(f.app.permissions.list().is_empty());
        assert_eq!(
            f.app.store.attention("s-1").await.ok().flatten(),
            None,
            "the permission no longer waits"
        );
    }

    #[tokio::test]
    async fn a_bad_decision_is_400_and_an_ask_of_another_session_is_404() {
        let mut f = Fixture::new().await;
        let ask = f.ask().await.unwrap();
        let uri = format!("/api/v1/sessions/s-1/permissions/{}", ask.id);
        assert_eq!(
            f.post(&uri, json!({ "decision": "maybe" })).await,
            StatusCode::BAD_REQUEST
        );
        let other = format!("/api/v1/sessions/other/permissions/{}", ask.id);
        assert_eq!(
            f.post(&other, json!({ "decision": "deny" })).await,
            StatusCode::NOT_FOUND
        );
        // Neither consumed it.
        assert!(f.app.permissions.get(&ask.id).is_some());
    }

    #[tokio::test]
    async fn releasing_a_session_denies_its_pending_asks_as_stopped() {
        let mut f = Fixture::new().await;
        f.ask().await.unwrap();
        assert_eq!(release_session(&f.app, "s-1").await, 1);
        assert!(f.app.permissions.list().is_empty());
        assert_eq!(f.turn_text().await, "denied");
        assert_eq!(release_session(&f.app, "s-1").await, 0);
    }

    #[test]
    fn rules_come_from_the_suggestion_of_that_tool_or_fall_back_to_its_name() {
        let suggestion = json!([{
            "type": "addRules", "behavior": "allow", "destination": "localSettings",
            "rules": [
                {"toolName": "Bash", "ruleContent": "npm install:*"},
                {"toolName": "Write"},
                {"toolName": "Bash", "ruleContent": "  "},
            ]
        }]);
        assert_eq!(
            rules_to_add("Bash", Some(&suggestion)),
            ["Bash(npm install:*)", "Bash"]
        );
        assert_eq!(rules_to_add("Read", Some(&suggestion)), ["Read"]);
        assert_eq!(rules_to_add("Read", None), ["Read"]);
        assert_eq!(
            rules_to_add("Read", Some(&json!([{"type": "setMode"}]))),
            ["Read"]
        );
    }

    #[test]
    fn rules_match_a_tool_or_a_bash_prefix() {
        let bash = |c: &str| json!({ "command": c });
        assert!(rule_matches("Read", "Read", &json!({})));
        assert!(!rule_matches("Read", "Write", &json!({})));
        assert!(!rule_matches("", "", &json!({})));
        assert!(rule_matches("Bash", "Bash", &bash("rm -rf x")));
        assert!(rule_matches(
            "Bash(npm install:*)",
            "Bash",
            &bash("npm install left-pad")
        ));
        assert!(rule_matches(
            "Bash(npm install:*)",
            "Bash",
            &bash("npm install")
        ));
        assert!(!rule_matches(
            "Bash(npm install:*)",
            "Bash",
            &bash("npm installer")
        ));
        assert!(!rule_matches(
            "Bash(npm install:*)",
            "Bash",
            &bash("npm test")
        ));
        assert!(!rule_matches(
            "Bash(npm install:*)",
            "Read",
            &bash("npm install")
        ));
        // Nothing chained behind an allowed prefix.
        for chained in [
            "npm install x && rm -rf /",
            "npm install x; ls",
            "npm install $(id)",
            "npm install x | sh",
            "npm install x\nls",
        ] {
            assert!(
                !rule_matches("Bash(npm install:*)", "Bash", &bash(chained)),
                "{chained}"
            );
        }
        assert!(rule_matches("Bash(ls -la)", "Bash", &bash("ls -la")));
        assert!(!rule_matches("Bash(ls -la)", "Bash", &bash("ls -la /")));
        assert!(!rule_matches("Bash(:*)", "Bash", &bash("ls")));
    }
}
