//! `/api/v1/jira`: the connection to Jira and the tickets it offers.

use axum::Json;
use axum::extract::{Path, Query, State};
use delilah_integrations::jira::{Filter, JiraClient, JiraIssue, JiraMode};
use serde::{Deserialize, Serialize};

use crate::api::ApiError;
use crate::state::AppState;

const SETTINGS_KEY: &str = "jira";
pub const DEFAULT_JQL: &str =
    "assignee = currentUser() AND resolution = Unresolved ORDER BY updated DESC";

/// What is kept in the settings table. The token is not: it goes to the Keychain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JiraSettings {
    pub mode: JiraMode,
    pub site: Option<String>,
    pub email: Option<String>,
    pub jql: String,
}

#[derive(Debug, Serialize)]
pub struct JiraStatus {
    pub settings: Option<JiraSettings>,
}

#[derive(Debug, Deserialize)]
pub struct Connect {
    pub mode: JiraMode,
    pub site: Option<String>,
    pub email: Option<String>,
    /// Omit to keep the token already saved for that site.
    pub token: Option<String>,
    pub jql: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Connected {
    pub account: String,
    pub settings: JiraSettings,
}

#[derive(Debug, Serialize)]
pub struct IssueRow {
    #[serde(flatten)]
    pub issue: JiraIssue,
    /// Status of the Delilah task for this ticket, if there is one.
    pub task_status: Option<String>,
}

pub async fn status(State(app): State<AppState>) -> Result<Json<JiraStatus>, ApiError> {
    Ok(Json(JiraStatus {
        settings: app.store.setting(SETTINGS_KEY).await?,
    }))
}

/// Saves the connection after checking it against Jira.
pub async fn connect(
    State(app): State<AppState>,
    Json(req): Json<Connect>,
) -> Result<Json<Connected>, ApiError> {
    let site = req
        .site
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        // A board's or a ticket's URL works too: keep the site's.
        .map(delilah_integrations::jira::site_root);
    // The token goes with every request: never in clear text.
    if req.mode != JiraMode::Sample
        && let Some(site) = &site
        && !site.starts_with("https://")
    {
        return Err(ApiError::bad_request(format!(
            "the Jira site must be an https:// URL, got `{site}`"
        )));
    }
    let email = req
        .email
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let settings = JiraSettings {
        mode: req.mode,
        site: site.clone(),
        email: email.clone(),
        jql: req
            .jql
            .filter(|q| !q.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_JQL.into()),
    };
    let token = match (&req.token, &site) {
        (Some(token), _) if !token.trim().is_empty() => Some(token.trim().to_string()),
        (_, Some(site)) => app.secrets.get(site),
        _ => None,
    };
    let client = build(&settings, token.as_deref())?;
    let account = client
        .myself()
        .await
        .map_err(|e| ApiError::unprocessable(e.to_string()))?;

    if let (Some(token), Some(site)) = (
        req.token
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty()),
        &site,
    ) {
        app.secrets.set(site, token).map_err(ApiError::internal)?;
    }
    app.store.set_setting(SETTINGS_KEY, &settings).await?;
    Ok(Json(Connected { account, settings }))
}

/// `?who=assigned|reported|query&progress=open|done`; assigned and open by default.
pub async fn issues(
    State(app): State<AppState>,
    Query(filter): Query<Filter>,
) -> Result<Json<Vec<IssueRow>>, ApiError> {
    let (settings, client) = client(&app).await?;
    let issues = client
        .list(&filter, &settings.jql)
        .await
        .map_err(|e| ApiError::unprocessable(e.to_string()))?;
    let tasks = app.store.list_tasks().await?;
    let rows = issues
        .into_iter()
        .map(|issue| {
            let task_status = tasks
                .iter()
                .find(|t| t.key.as_str() == issue.key)
                .map(|t| t.status.name().to_string());
            IssueRow { issue, task_status }
        })
        .collect();
    Ok(Json(rows))
}

pub async fn issue(
    State(app): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<JiraIssue>, ApiError> {
    let (_, client) = client(&app).await?;
    let issue = client
        .issue(&key)
        .await
        .map_err(|e| ApiError::unprocessable(e.to_string()))?;
    Ok(Json(issue))
}

/// The ticket, if Jira is connected and has it. Never fails a run: no ticket is fine.
pub async fn fetch_issue(app: &AppState, key: &str) -> Option<JiraIssue> {
    let (_, client) = client(app).await.ok()?;
    match client.issue(key).await {
        Ok(issue) => Some(issue),
        Err(e) => {
            tracing::info!(task = key, "no Jira ticket for the agent: {e}");
            None
        }
    }
}

/// The ticket as Markdown for the agent to read.
pub fn write_ticket(
    path: &std::path::Path,
    issue: &JiraIssue,
    lang: crate::lang::Lang,
) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let l = |es, en| lang.pick(es, en);
    let mut md = format!("# {}: {}\n\n", issue.key, issue.summary);
    md.push_str(&format!("- {}: {}\n", l("Tipo", "Type"), issue.issue_type));
    if let Some(p) = &issue.priority {
        md.push_str(&format!("- {}: {p}\n", l("Prioridad", "Priority")));
    }
    md.push_str(&format!("- {}: {}\n", l("Estado", "Status"), issue.status));
    if let Some(epic) = &issue.epic {
        let title = epic
            .summary
            .as_deref()
            .map(|s| format!(" {s}"))
            .unwrap_or_default();
        md.push_str(&format!("- {}: {}{title}\n", l("Épica", "Epic"), epic.key));
    }
    if !issue.components.is_empty() {
        md.push_str(&format!(
            "- {}: {}\n",
            l("Componentes", "Components"),
            issue.components.join(", ")
        ));
    }
    if let Some(url) = &issue.url {
        md.push_str(&format!("- {}: {url}\n", l("Enlace", "Link")));
    }
    md.push_str(&format!("\n## {}\n\n", l("Descripción", "Description")));
    md.push_str(if issue.description.is_empty() {
        l("(sin descripción)", "(no description)")
    } else {
        &issue.description
    });
    md.push('\n');
    std::fs::write(path, md)
}

async fn client(app: &AppState) -> Result<(JiraSettings, JiraClient), ApiError> {
    let settings: JiraSettings = app
        .store
        .setting(SETTINGS_KEY)
        .await?
        .ok_or_else(|| ApiError::not_found("Jira is not connected yet"))?;
    let token = settings
        .site
        .as_deref()
        .and_then(|site| app.secrets.get(site));
    let client = build(&settings, token.as_deref())?;
    Ok((settings, client))
}

fn build(settings: &JiraSettings, token: Option<&str>) -> Result<JiraClient, ApiError> {
    let need = |what: &str| ApiError::bad_request(format!("Jira {what} is missing"));
    Ok(match settings.mode {
        JiraMode::Sample => JiraClient::sample(),
        JiraMode::Cloud => JiraClient::cloud(
            settings.site.as_deref().ok_or_else(|| need("site URL"))?,
            settings
                .email
                .as_deref()
                .ok_or_else(|| need("account email"))?,
            token.ok_or_else(|| need("API token"))?,
        ),
        JiraMode::DataCenter => JiraClient::data_center(
            settings.site.as_deref().ok_or_else(|| need("site URL"))?,
            token.ok_or_else(|| need("personal access token"))?,
        ),
    })
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use crate::config::Config;
    use crate::state::AppState;
    use crate::store::Store;

    async fn app() -> AppState {
        let config = Config {
            home: std::env::temp_dir(),
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
        };
        AppState::new(Store::in_memory().await.unwrap(), config)
    }

    async fn call(
        state: &AppState,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(uri);
        let body = match body {
            Some(b) => {
                req = req.header("content-type", "application/json");
                Body::from(b.to_string())
            }
            None => Body::empty(),
        };
        let res = crate::api::router(state.clone())
            .oneshot(req.body(body).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    #[tokio::test]
    async fn connects_to_the_sample_and_lists_its_tickets() {
        let state = app().await;
        let (status, _) = call(&state, "GET", "/api/v1/jira/issues", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, body) = call(
            &state,
            "PUT",
            "/api/v1/jira",
            Some(json!({ "mode": "sample" })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["account"], "Datos de ejemplo");

        let (status, rows) = call(&state, "GET", "/api/v1/jira/issues", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(rows[0]["key"], "PROJ-1242");
        assert_eq!(rows[0]["task_status"], Value::Null);

        let (_, issue) = call(&state, "GET", "/api/v1/jira/issues/PROJ-1243", None).await;
        assert_eq!(issue["components"], json!(["svc-billing"]));
        assert_eq!(issue["epic"]["summary"], "Facturación v2");

        let (status, done) = call(
            &state,
            "GET",
            "/api/v1/jira/issues?who=reported&progress=done",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{done}");
        assert_eq!(done[0]["key"], "PROJ-1238");
        assert_eq!(done[0]["status_category"], "done");
    }

    #[tokio::test]
    async fn writes_the_ticket_for_the_agent() {
        let state = app().await;
        call(
            &state,
            "PUT",
            "/api/v1/jira",
            Some(json!({ "mode": "sample" })),
        )
        .await;
        let issue = super::fetch_issue(&state, "PROJ-1242").await.unwrap();
        assert!(super::fetch_issue(&state, "NOPE-1").await.is_none());

        let path =
            std::env::temp_dir().join(format!("delilah-ticket-{}/ticket.md", uuid::Uuid::new_v4()));
        super::write_ticket(&path, &issue, crate::lang::Lang::Es).unwrap();
        let md = std::fs::read_to_string(&path).unwrap();
        assert!(
            md.starts_with("# PROJ-1242: Rechazar cobros duplicados"),
            "{md}"
        );
        assert!(md.contains("- Componentes: svc-payments, svc-orders"));
        assert!(md.contains("## Descripción"));
    }

    #[tokio::test]
    async fn the_token_never_goes_over_plain_http() {
        let state = app().await;
        let (status, body) = call(
            &state,
            "PUT",
            "/api/v1/jira",
            Some(json!({ "mode": "data-center", "site": "http://jira.acme.test", "token": "t" })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"].as_str().unwrap().contains("https"), "{body}");
    }

    #[tokio::test]
    async fn a_cloud_connection_needs_its_credentials() {
        let state = app().await;
        let (status, body) = call(
            &state,
            "PUT",
            "/api/v1/jira",
            Some(json!({ "mode": "cloud", "site": "https://acme.atlassian.net" })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"].as_str().unwrap().contains("email"), "{body}");
        let (_, saved) = call(&state, "GET", "/api/v1/jira", None).await;
        assert_eq!(saved["settings"], Value::Null);
    }
}
