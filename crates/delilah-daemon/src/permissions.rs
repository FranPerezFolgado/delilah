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
/// Asks one session may have waiting; beyond them the agent is told no.
const MAX_PENDING: usize = 50;
/// What the agent is told when it has too many asks waiting.
const TOO_MANY_DENIAL: &str = "demasiados permisos pendientes; espera a que se respondan";

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
    /// Claude's `permission_suggestions`, as sent.
    pub suggestion: Option<Value>,
    /// The exact rule "always allow" would add to the task, or null when there is none
    /// that is safe to remember: then "always allow" is not offered (and answers 400).
    pub rule: Option<String>,
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

    /// Keeps the ask unless the session already has [`MAX_PENDING`] waiting.
    pub(crate) fn insert(&self, ask: PermissionAsk) -> bool {
        let mut pending = self.pending.lock().unwrap();
        if pending
            .values()
            .filter(|a| a.session == ask.session)
            .count()
            >= MAX_PENDING
        {
            return false;
        }
        pending.insert(ask.id.clone(), ask);
        true
    }

    /// Takes the ask off the list, so only one answer can ever act on it.
    pub(crate) fn take(&self, id: &str) -> Option<PermissionAsk> {
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

/// The line the inbox shows for an ask: `Tool: summary`.
fn attention_text(ask: &PermissionAsk) -> String {
    if ask.summary.is_empty() {
        ask.tool.clone()
    } else {
        format!("{}: {}", ask.tool, ask.summary)
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
    // A request after the session is gone must not leave an ask nobody can answer.
    if app.sessions.get(session).is_none() {
        return;
    }
    let rules = match app.store.get_task(&TaskKey::new(task)).await {
        Ok(Some(t)) => t.allowed_tools,
        _ => Vec::new(),
    };
    let workspace = crate::owners::workspace(app, task)
        .await
        .map(|w| w.path)
        .unwrap_or_default();
    let allowed = rules.iter().any(|r| {
        if !delilah_agents::claude::is_safe_allow_rule(r) {
            tracing::warn!(%task, "ignoring the stored rule `{r}`: it is not specific enough");
            return false;
        }
        rule_matches(r, &tool, &input, &workspace)
    });
    if allowed {
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
        rule: rule_to_add(&tool, suggestion.as_ref(), &input, &workspace),
        tool,
        input,
        suggestion,
        at: Utc::now(),
    };
    let text = attention_text(&ask);
    let request_id = ask.request_id.clone();
    let tool = ask.tool.clone();
    if !app.permissions.insert(ask) {
        tracing::warn!(%session, "too many permissions waiting; denying");
        let denial = Decision::Deny {
            message: TOO_MANY_DENIAL.into(),
        };
        let _ = reply(app, session, &request_id, denial).await;
        return;
    }
    if let Some(live) = app.sessions.get(session) {
        live.set_state(LiveState::Waiting);
    }
    let attention = Attention {
        kind: AttentionKind::Permission,
        text,
        since: Utc::now(),
    };
    let _ = app.store.set_attention(session, Some(&attention)).await;
    crate::autopilot::on_permission(app, &TaskKey::new(task), session, &tool).await;
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
    if body.decision == "allow_always" && app.permissions.get(&id).is_none_or(|a| a.rule.is_none())
    {
        return Err(ApiError::bad_request(
            "no rule to remember for this request",
        ));
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
        Decision::Allow {
            input: ask.input.clone(),
        }
    };
    let allowed = matches!(decision, Decision::Allow { .. });
    let result = reply(&app, &session, &ask.request_id, decision).await;
    // Remembered only once the agent has the answer.
    if result.is_ok()
        && body.decision == "allow_always"
        && let Some(rule) = &ask.rule
    {
        if let Err(e) = app
            .store
            .add_allowed_tool(&TaskKey::new(&ask.task), rule)
            .await
        {
            tracing::warn!(task = %ask.task, "cannot remember the rule `{rule}`: {e}");
        }
        app.emit(Topic::Tasks);
    }
    // Other asks of the session still wait: attention moves to the next one, the state stays.
    let next = app
        .permissions
        .list()
        .into_iter()
        .find(|a| a.session == session);
    if let Some(next) = next {
        let attention = Attention {
            kind: AttentionKind::Permission,
            text: attention_text(&next),
            since: next.at,
        };
        let _ = app.store.set_attention(&session, Some(&attention)).await;
    } else {
        clear_attention(&app, &session).await;
        if allowed && let Some(live) = app.sessions.get(&session) {
            live.set_state(LiveState::Working);
        }
    }
    app.emit(Topic::Inbox);
    app.emit(Topic::Conversations);
    result.map_err(|e| ApiError::conflict(format!("cannot answer the agent: {e}")))?;
    Ok(StatusCode::NO_CONTENT)
}

/// The one rule "always allow" adds, for a tool Claude offered a rule for: the first
/// suggested rule that passes [`validate_rule`], else (Bash) the exact command when it
/// may be remembered. Never a bare tool name. `workspace` bounds file rules.
pub fn rule_to_add(
    tool: &str,
    suggestion: Option<&Value>,
    input: &Value,
    workspace: &std::path::Path,
) -> Option<String> {
    let mut offered = false;
    for s in suggestion.and_then(Value::as_array).into_iter().flatten() {
        if s["type"] != "addRules" || s["behavior"] != "allow" {
            continue;
        }
        for r in s["rules"].as_array().into_iter().flatten() {
            if r["toolName"].as_str() != Some(tool) {
                continue;
            }
            offered = true;
            if let Some(content) = r["ruleContent"].as_str()
                && let Some(rule) = validate_rule(tool, content, input, workspace)
            {
                return Some(rule);
            }
        }
    }
    // A prefix that is not on the allowlist is remembered as the exact command instead.
    if offered && tool == "Bash" {
        let command = input["command"].as_str()?.trim();
        return validate_rule(tool, command, input, workspace);
    }
    // Claude often suggests a mode or a directory for a file tool, not a rule: the rule is
    // built from the approved file whenever it suggested anything.
    let suggested = suggestion
        .and_then(Value::as_array)
        .is_some_and(|a| !a.is_empty());
    if suggested && delilah_agents::claude::PATH_TOOLS.contains(&tool) {
        return validate_rule(tool, "", input, workspace);
    }
    None
}

/// The file the call works on, relative to the workspace (`None` when it lies outside).
fn relative_file(input: &Value, workspace: &std::path::Path) -> Option<String> {
    let file = ["file_path", "notebook_path", "path"]
        .iter()
        .find_map(|k| input[*k].as_str())?;
    let path = std::path::Path::new(file);
    if !path.is_absolute() {
        return Some(file.to_string());
    }
    // Unknown workspace: an absolute path is not inside it (`strip_prefix("")` would say so).
    if workspace.as_os_str().is_empty() {
        return None;
    }
    let canonical = workspace.canonicalize().ok();
    std::iter::once(workspace)
        .chain(canonical.as_deref())
        .find_map(|w| path.strip_prefix(w).ok())
        .and_then(|p| p.to_str())
        .map(str::to_string)
}

/// The host of a URL, lowercased.
fn url_host(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let host = rest.split(['/', '?', '#']).next()?;
    if host.contains('@') || host.contains(':') {
        return None;
    }
    Some(host.to_ascii_lowercase())
}

/// `Tool(content)` when it is safe to remember (see
/// [`delilah_agents::claude::is_safe_allow_rule`]) and covers the call being approved:
/// - Bash: the command starts with the prefix at a word boundary (a prefix must be on the
///   claude.rs allowlist) or equals the exact command. An exact command is remembered
///   unless it has shell metacharacters, a leading `VAR=value` or an interpreter/wrapper
///   head (`sh x.sh` runs whatever the file holds later).
/// - File tools: the content is ignored; the rule is the approved file itself, relative to
///   `workspace` (a directory `/**` rule is never offered). A file outside it gets none.
/// - WebFetch: `domain:<host>` for exactly the host being fetched.
pub fn validate_rule(
    tool: &str,
    content: &str,
    input: &Value,
    workspace: &std::path::Path,
) -> Option<String> {
    let content = content.trim();
    let rule = if delilah_agents::claude::PATH_TOOLS.contains(&tool) {
        format!("{tool}({})", relative_file(input, workspace)?)
    } else {
        format!("{tool}({content})")
    };
    if !delilah_agents::claude::is_safe_allow_rule(&rule) {
        return None;
    }
    if tool == "WebFetch" {
        let host = content.strip_prefix("domain:")?;
        if url_host(input["url"].as_str()?)? != host.to_ascii_lowercase() {
            return None;
        }
    }
    if tool == "Bash" {
        let command = input["command"].as_str()?.trim();
        let covered = match content.strip_suffix(":*") {
            Some(prefix) => command
                .strip_prefix(prefix.trim())
                .is_some_and(|tail| tail.is_empty() || tail.starts_with(char::is_whitespace)),
            None => command == content,
        };
        if !covered {
            return None;
        }
    }
    Some(rule)
}

/// Whether an allow rule covers this call: `Tool` covers any use of the tool;
/// `Bash(<prefix>:*)` a command that starts with the prefix followed by bounded arguments
/// (`claude::is_safe_prefix_tail`); `Bash(<command>)` exactly it;
/// `Edit(<file>)` (any file tool) that file, relative to `workspace` (`<dir>/**`: inside
/// that dir); `WebFetch(domain:<host>)` exactly that host. Anything else does not match.
pub fn rule_matches(rule: &str, tool: &str, input: &Value, workspace: &std::path::Path) -> bool {
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
    if name != tool {
        return false;
    }
    if delilah_agents::claude::PATH_TOOLS.contains(&tool) {
        let Some(file) = relative_file(input, workspace) else {
            return false;
        };
        let content = content.trim();
        if std::path::Path::new(&file)
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return false;
        }
        return match content.strip_suffix("/**") {
            Some(dir) => {
                !dir.is_empty() && file.strip_prefix(dir).is_some_and(|t| t.starts_with('/'))
            }
            None => file == content,
        };
    }
    if tool == "WebFetch" {
        let Some(domain) = content.trim().strip_prefix("domain:") else {
            return false;
        };
        return input["url"]
            .as_str()
            .and_then(url_host)
            .is_some_and(|h| !domain.is_empty() && h == domain.to_ascii_lowercase());
    }
    if tool != "Bash" {
        return false;
    }
    let Some(command) = input["command"].as_str().map(str::trim) else {
        return false;
    };
    match content.strip_suffix(":*") {
        Some(prefix) => {
            let prefix = prefix.trim();
            !prefix.is_empty()
                && command
                    .strip_prefix(prefix)
                    .is_some_and(delilah_agents::claude::is_safe_prefix_tail)
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
                    ..crate::config::Config::test_defaults()
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
            self.ask_with("NEEDS_PERMISSION").await
        }

        /// The same, with the user line the fake agent reads (its markers pick the tool).
        async fn ask_with(&mut self, line: &str) -> Option<PermissionAsk> {
            self.live
                .runtime
                .as_stream()
                .unwrap()
                .send_user(line)
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
        let ask = f.ask().await.unwrap();
        // The fake agent's suggestion has no rules; give the ask a safe one.
        f.app
            .permissions
            .pending
            .lock()
            .unwrap()
            .get_mut(&ask.id)
            .unwrap()
            .rule = Some("Bash(npm --version)".into());
        let status = f
            .post(
                &format!("/api/v1/sessions/s-1/permissions/{}", ask.id),
                json!({ "decision": "allow_always" }),
            )
            .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(f.turn_text().await, "allowed");
        let task = f.app.store.get_task(&TaskKey::new("PROJ-1")).await.unwrap();
        assert_eq!(task.unwrap().allowed_tools, ["Bash(npm --version)"]);
        // Same request again: no ask, the agent goes on by itself.
        assert!(f.ask().await.is_none());
        assert_eq!(f.turn_text().await, "allowed");
    }

    #[tokio::test]
    async fn allow_always_without_a_rule_is_400_and_keeps_the_ask() {
        let mut f = Fixture::new().await;
        let ask = f.ask().await.unwrap();
        assert!(ask.rule.is_none(), "no bare `Bash` fallback");
        let uri = format!("/api/v1/sessions/s-1/permissions/{}", ask.id);
        assert_eq!(
            f.post(&uri, json!({ "decision": "allow_always" })).await,
            StatusCode::BAD_REQUEST
        );
        assert!(f.app.permissions.get(&ask.id).is_some());
        let task = f.app.store.get_task(&TaskKey::new("PROJ-1")).await.unwrap();
        assert!(task.unwrap().allowed_tools.is_empty());
        // allow_once still works.
        assert_eq!(
            f.post(&uri, json!({ "decision": "allow_once" })).await,
            StatusCode::NO_CONTENT
        );
    }

    #[tokio::test]
    async fn a_failed_reply_is_409_and_remembers_nothing() {
        let mut f = Fixture::new().await;
        let ask = f.ask().await.unwrap();
        f.app
            .permissions
            .pending
            .lock()
            .unwrap()
            .get_mut(&ask.id)
            .unwrap()
            .rule = Some("Bash(npm --version)".into());
        f.app.sessions.remove_if_same("s-1", &f.live.runtime);
        let status = f
            .post(
                &format!("/api/v1/sessions/s-1/permissions/{}", ask.id),
                json!({ "decision": "allow_always" }),
            )
            .await;
        assert_eq!(status, StatusCode::CONFLICT);
        let task = f.app.store.get_task(&TaskKey::new("PROJ-1")).await.unwrap();
        assert!(task.unwrap().allowed_tools.is_empty());
    }

    #[tokio::test]
    async fn a_request_for_a_gone_session_leaves_no_ask() {
        let f = Fixture::new().await;
        on_request(
            &f.app,
            "PROJ-1",
            "nope",
            "r-1".into(),
            "Bash".into(),
            json!({ "command": "ls" }),
            None,
        )
        .await;
        assert!(f.app.permissions.list().is_empty());
    }

    #[test]
    fn a_session_holds_at_most_fifty_asks() {
        let p = Permissions::default();
        let ask = |session: &str| PermissionAsk {
            id: uuid::Uuid::new_v4().to_string(),
            request_id: "r".into(),
            task: "T".into(),
            session: session.into(),
            tool: "Bash".into(),
            input: json!({}),
            suggestion: None,
            rule: None,
            summary: String::new(),
            at: Utc::now(),
        };
        for _ in 0..MAX_PENDING {
            assert!(p.insert(ask("a")));
        }
        assert!(!p.insert(ask("a")));
        assert!(p.insert(ask("b")));
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
    fn the_rule_comes_only_from_a_validated_suggestion_never_a_bare_tool() {
        let ws = std::path::Path::new("/ws");
        let bash = json!({ "command": "npm install left-pad" });
        let suggestion = json!([{
            "type": "addRules", "behavior": "allow", "destination": "localSettings",
            "rules": [
                {"toolName": "Bash", "ruleContent": "sh:*"},
                {"toolName": "Write"},
                {"toolName": "Bash", "ruleContent": "  "},
                {"toolName": "Bash", "ruleContent": "npm install:*"},
            ]
        }]);
        // `npm install` is not on the allowlist: the exact command is remembered.
        assert_eq!(
            rule_to_add("Bash", Some(&suggestion), &bash, ws).as_deref(),
            Some("Bash(npm install left-pad)")
        );
        let test = json!({ "command": "npm test -- --ci" });
        let allow = |c: &str| {
            json!([{"type": "addRules", "behavior": "allow",
                "rules": [{"toolName": "Bash", "ruleContent": c}]}])
        };
        assert_eq!(
            rule_to_add("Bash", Some(&allow("npm test:*")), &test, ws).as_deref(),
            Some("Bash(npm test:*)")
        );
        // A write tool with no file to approve has no rule.
        assert_eq!(
            rule_to_add("Write", Some(&suggestion), &json!({}), ws),
            None
        );
        assert_eq!(rule_to_add("Read", Some(&suggestion), &json!({}), ws), None);
        assert_eq!(rule_to_add("Bash", None, &bash, ws), None);
        assert_eq!(
            rule_to_add("Read", Some(&json!([{"type": "setMode"}])), &json!({}), ws),
            None
        );
    }

    #[test]
    fn bypass_prefixes_are_downgraded_to_the_exact_command_or_nothing() {
        let ws = std::path::Path::new("/ws");
        let offer = |c: &str| {
            json!([{"type": "addRules", "behavior": "allow",
                "rules": [{"toolName": "Bash", "ruleContent": c}]}])
        };
        let rule = |prefix: &str, command: &str| {
            rule_to_add(
                "Bash",
                Some(&offer(prefix)),
                &json!({ "command": command }),
                ws,
            )
        };
        for (prefix, command) in [
            ("git:*", "git status"),
            ("npm run:*", "npm run build"),
            ("find:*", "find . -name x"),
            ("git config:*", "git config user.name"),
            ("rm:*", "rm -rf x"),
            ("curl:*", "curl example.com"),
            ("git -c core.pager=sh:*", "git -c core.pager=sh log"),
        ] {
            assert_eq!(
                rule(prefix, command).as_deref(),
                Some(format!("Bash({command})").as_str()),
                "{prefix}"
            );
        }
        // Nothing at all: interpreter head, env assignment, metacharacters.
        assert_eq!(rule("FOO=1 sh:*", "FOO=1 sh x.sh"), None);
        assert_eq!(rule("sh:*", "sh x.sh"), None);
        assert_eq!(rule("git:*", "git log; rm -rf x"), None);
        // Accepted prefixes.
        for (prefix, command) in [
            ("npm test:*", "npm test"),
            ("cargo test:*", "cargo test -p x"),
            ("git status:*", "git status -s"),
            ("ls:*", "ls -la"),
        ] {
            assert_eq!(
                rule(prefix, command).as_deref(),
                Some(format!("Bash({prefix})").as_str())
            );
        }
    }

    #[test]
    fn file_rules_are_the_approved_file_inside_the_workspace() {
        let ws = std::path::Path::new("/ws");
        let sugg = json!([{"type": "setMode", "mode": "acceptEdits"}]);
        let rule = |tool: &str, input: Value| rule_to_add(tool, Some(&sugg), &input, ws);
        assert_eq!(
            rule("Edit", json!({ "file_path": "/ws/src/a.rs" })).as_deref(),
            Some("Edit(src/a.rs)")
        );
        assert_eq!(
            rule("Write", json!({ "file_path": "src/b.rs" })).as_deref(),
            Some("Write(src/b.rs)")
        );
        for outside in [
            "/etc/passwd",
            "/ws/../etc/passwd",
            "../x",
            "~/x",
            "/ws/src/*.rs",
            "/wsx/a.rs",
        ] {
            assert_eq!(
                rule("Edit", json!({ "file_path": outside })),
                None,
                "{outside}"
            );
        }
        assert_eq!(rule("Edit", json!({})), None);
    }

    #[test]
    fn web_fetch_rules_are_the_exact_host() {
        let ws = std::path::Path::new("/ws");
        let v = |c: &str, url: &str| validate_rule("WebFetch", c, &json!({ "url": url }), ws);
        assert_eq!(
            v("domain:docs.rs", "https://docs.rs/serde").as_deref(),
            Some("WebFetch(domain:docs.rs)")
        );
        assert_eq!(v("domain:*.com", "https://a.com/x"), None);
        assert_eq!(v("domain:docs.rs", "https://evil.com/docs.rs"), None);
        assert_eq!(v("domain:*", "https://docs.rs"), None);
    }

    #[test]
    fn validate_rule_checks_the_content_and_that_the_command_is_covered() {
        let ws = std::path::Path::new("/ws");
        let cmd = |c: &str| json!({ "command": c });
        let ok = |t: &str, c: &str, i: &Value| validate_rule(t, c, i, ws);
        assert_eq!(
            ok("Bash", " npm test:* ", &cmd("npm test x")).as_deref(),
            Some("Bash(npm test:*)")
        );
        assert_eq!(
            ok("Bash", "git status", &cmd("git status")).as_deref(),
            Some("Bash(git status)")
        );
        assert_eq!(
            ok("Bash", "npm install lodash", &cmd("npm install lodash")).as_deref(),
            Some("Bash(npm install lodash)")
        );
        // The prefix must cover the command, at a word boundary.
        assert_eq!(ok("Bash", "npm test:*", &cmd("npm testing")), None);
        assert_eq!(ok("Bash", "npm test:*", &cmd("npm install")), None);
        assert_eq!(ok("Bash", "git status", &cmd("git status -s")), None);
        assert_eq!(ok("Bash", "npm test:*", &json!({})), None);
        for bad in [
            "",
            ":*",
            "*",
            "npm*:*",
            "sh:*",
            "node -e:*",
            "python -c:*",
            "env FOO=1:*",
            "npm i; rm:*",
            "npm i && x:*",
            "a|b:*",
            "x $(id):*",
            "x `id`:*",
            "a\nb:*",
        ] {
            assert_eq!(ok("Bash", bad, &cmd(bad)), None, "{bad:?}");
        }
        for bad in ["", "*", "**", "//**", "domain:*"] {
            assert_eq!(ok("WebFetch", bad, &json!({})), None, "{bad:?}");
            assert_eq!(ok("Read", bad, &json!({})), None, "{bad:?}");
        }
    }

    #[test]
    fn rules_match_a_tool_or_a_bash_prefix() {
        fn rule_matches(rule: &str, tool: &str, input: &Value) -> bool {
            super::rule_matches(rule, tool, input, std::path::Path::new("/ws"))
        }
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
            "npm install x`id`",
            "npm install x\r\nls",
            "FOO=1 npm install x",
            "env npm install x",
        ] {
            assert!(
                !rule_matches("Bash(npm install:*)", "Bash", &bash(chained)),
                "{chained}"
            );
        }
        // Nor a flag that runs a program, nor a path out of the workspace.
        for escape in [
            ("Bash(go test:*)", "go test -exec /bin/sh ./..."),
            (
                "Bash(cargo test:*)",
                "cargo test --config target.x.runner=sh",
            ),
            ("Bash(git diff:*)", "git diff --output=/Users/x/.zshrc"),
            ("Bash(cat:*)", "cat ~/.ssh/id_rsa"),
            ("Bash(cat:*)", "cat ../../secret"),
        ] {
            assert!(
                !rule_matches(escape.0, "Bash", &bash(escape.1)),
                "{escape:?}"
            );
        }
        assert!(rule_matches(
            "Bash(go test:*)",
            "Bash",
            &bash("go test -v ./...")
        ));
        assert!(rule_matches("Bash(ls -la)", "Bash", &bash("ls -la")));
        assert!(!rule_matches("Bash(ls -la)", "Bash", &bash("ls -la /")));
        assert!(!rule_matches("Bash(:*)", "Bash", &bash("ls")));
    }

    #[test]
    fn file_and_web_fetch_rules_match_what_rule_to_add_writes() {
        let ws = std::path::Path::new("/ws");
        let m = |r: &str, t: &str, i: Value| rule_matches(r, t, &i, ws);
        assert!(m(
            "Edit(src/a.rs)",
            "Edit",
            json!({ "file_path": "/ws/src/a.rs" })
        ));
        assert!(m(
            "Edit(src/a.rs)",
            "Edit",
            json!({ "file_path": "src/a.rs" })
        ));
        assert!(!m(
            "Edit(src/a.rs)",
            "Edit",
            json!({ "file_path": "/ws/src/b.rs" })
        ));
        assert!(!m(
            "Edit(src/a.rs)",
            "Write",
            json!({ "file_path": "/ws/src/a.rs" })
        ));
        assert!(!m(
            "Edit(src/a.rs)",
            "Edit",
            json!({ "file_path": "/ws/../etc/a.rs" })
        ));
        assert!(!m("Edit(src/a.rs)", "Edit", json!({})));
        // An unknown workspace never makes an absolute path "inside" it.
        assert!(!rule_matches(
            "Edit(etc/passwd)",
            "Edit",
            &json!({ "file_path": "/etc/passwd" }),
            std::path::Path::new("")
        ));
        assert_eq!(
            relative_file(
                &json!({ "file_path": "/etc/passwd" }),
                std::path::Path::new("")
            ),
            None
        );
        assert!(m("Grep(src)", "Grep", json!({ "path": "/ws/src" })));
        assert!(m(
            "Edit(src/**)",
            "Edit",
            json!({ "file_path": "/ws/src/x/a.rs" })
        ));
        assert!(!m(
            "Edit(src/**)",
            "Edit",
            json!({ "file_path": "/ws/srcx/a.rs" })
        ));
        assert!(!m(
            "Edit(src/**)",
            "Edit",
            json!({ "file_path": "/ws/src/../etc/a" })
        ));
        let fetch = |url: &str| json!({ "url": url });
        assert!(m(
            "WebFetch(domain:docs.rs)",
            "WebFetch",
            fetch("https://docs.rs/serde")
        ));
        assert!(m(
            "WebFetch(domain:docs.rs)",
            "WebFetch",
            fetch("https://DOCS.rs/x")
        ));
        assert!(!m(
            "WebFetch(domain:docs.rs)",
            "WebFetch",
            fetch("https://evil.docs.rs/x")
        ));
        assert!(!m(
            "WebFetch(domain:docs.rs)",
            "WebFetch",
            fetch("https://evil.com/docs.rs")
        ));
        assert!(!m("WebFetch(domain:docs.rs)", "WebFetch", json!({})));
        assert!(!m("Task(x)", "Task", json!({})));
    }

    async fn second_session_ask(f: &Fixture) {
        let ask = PermissionAsk {
            id: "second".into(),
            request_id: "perm-2".into(),
            task: "PROJ-1".into(),
            session: "s-1".into(),
            tool: "Edit".into(),
            input: json!({}),
            suggestion: None,
            rule: None,
            summary: "src/z.rs".into(),
            at: Utc::now() + chrono::Duration::seconds(1),
        };
        assert!(f.app.permissions.insert(ask));
    }

    #[tokio::test]
    async fn answering_one_ask_keeps_the_state_while_others_wait() {
        let mut f = Fixture::new().await;
        f.app
            .store
            .create_session(&crate::store_sessions::SessionRecord {
                id: "s-1".into(),
                task: "PROJ-1".into(),
                kind: SessionKind::Claude,
                label: "chat".into(),
                primary: true,
                closed: false,
                read_only: false,
                conversation: false,
                step: None,
            })
            .await
            .unwrap();
        let ask = f.ask().await.unwrap();
        second_session_ask(&f).await;
        let status = f
            .post(
                &format!("/api/v1/sessions/s-1/permissions/{}", ask.id),
                json!({ "decision": "allow_once" }),
            )
            .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(f.live.state(), LiveState::Waiting);
        let a = f.app.store.attention("s-1").await.unwrap().unwrap();
        assert_eq!(a.kind, AttentionKind::Permission);
        assert_eq!(a.text, "Edit: src/z.rs");
        // The last one answered: attention clears and the agent works again.
        f.app.permissions.take("second");
        let ask = f.ask().await.unwrap();
        f.post(
            &format!("/api/v1/sessions/s-1/permissions/{}", ask.id),
            json!({ "decision": "allow_once" }),
        )
        .await;
        assert_eq!(f.live.state(), LiveState::Working);
        assert_eq!(f.app.store.attention("s-1").await.ok().flatten(), None);
    }

    #[tokio::test]
    async fn allow_always_on_a_file_or_a_url_is_not_asked_again_in_the_same_session() {
        for (marker, tool, rule, first, same, other) in [
            (
                "NEEDS_PERMISSION EDIT_A",
                "Edit",
                "Edit(src/a.rs)",
                json!({ "file_path": "src/a.rs" }),
                "NEEDS_PERMISSION EDIT_A",
                "NEEDS_PERMISSION EDIT_B",
            ),
            (
                "NEEDS_PERMISSION FETCH_A",
                "WebFetch",
                "WebFetch(domain:docs.rs)",
                json!({ "url": "https://docs.rs/serde" }),
                "NEEDS_PERMISSION FETCH_A",
                "NEEDS_PERMISSION FETCH_B",
            ),
        ] {
            let mut f = Fixture::new().await;
            let ask = f.ask_with(marker).await.unwrap();
            assert_eq!(ask.tool, tool);
            assert_eq!(ask.input, first);
            f.app
                .permissions
                .pending
                .lock()
                .unwrap()
                .get_mut(&ask.id)
                .unwrap()
                .rule = Some(rule.into());
            let status = f
                .post(
                    &format!("/api/v1/sessions/s-1/permissions/{}", ask.id),
                    json!({ "decision": "allow_always" }),
                )
                .await;
            assert_eq!(status, StatusCode::NO_CONTENT);
            assert_eq!(f.turn_text().await, "allowed");
            assert!(f.ask_with(same).await.is_none(), "{tool}: not asked again");
            assert_eq!(f.turn_text().await, "allowed");
            assert!(
                f.ask_with(other).await.is_some(),
                "{tool}: another target asks"
            );
        }
    }

    #[tokio::test]
    async fn the_agent_gets_the_trimmed_reason_or_the_default_denial() {
        for (reason, expected) in [
            (json!("  no way  "), "no way"),
            (json!("   "), DEFAULT_DENIAL),
            (Value::Null, DEFAULT_DENIAL),
        ] {
            let mut f = Fixture::new().await;
            let ask = f.ask_with("NEEDS_PERMISSION ECHO_DENY").await.unwrap();
            let status = f
                .post(
                    &format!("/api/v1/sessions/s-1/permissions/{}", ask.id),
                    json!({ "decision": "deny", "reason": reason }),
                )
                .await;
            assert_eq!(status, StatusCode::NO_CONTENT);
            assert_eq!(f.turn_text().await, format!("denied: {expected}"));
        }
    }
}
