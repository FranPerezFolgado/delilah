//! Jira client: Cloud (REST v3, ADF descriptions), Server/Data Center (REST v2) or
//! built-in sample data for developing without access to a real site.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::adf::adf_to_text;

const FIELDS: &str = "summary,issuetype,priority,status,assignee,components,description";
const MAX_RESULTS: &str = "50";
const SAMPLE: &str = include_str!("../fixtures/jira-search.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JiraMode {
    /// Built-in sample tickets.
    Sample,
    /// Jira Cloud (`*.atlassian.net`): email + API token.
    Cloud,
    /// Jira Server / Data Center: personal access token.
    DataCenter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JiraIssue {
    pub key: String,
    pub summary: String,
    pub issue_type: String,
    pub priority: Option<String>,
    pub status: String,
    pub assignee: Option<String>,
    pub components: Vec<String>,
    /// Plain text with light Markdown.
    pub description: String,
    pub url: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum JiraError {
    #[error("Jira rejected the credentials (HTTP {0}); check the email and token")]
    Unauthorized(u16),
    #[error("Jira answered HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("cannot reach Jira: {0}")]
    Network(String),
    #[error("unexpected answer from Jira: {0}")]
    Parse(String),
    #[error("{0} is not in the sample data")]
    NotInSample(String),
}

pub enum JiraClient {
    Sample,
    Remote {
        mode: JiraMode,
        site: String,
        auth: Auth,
        http: reqwest::Client,
    },
}

pub enum Auth {
    /// Cloud: email + API token.
    Basic { email: String, token: String },
    /// Data Center: personal access token.
    Bearer(String),
}

impl JiraClient {
    pub fn sample() -> Self {
        Self::Sample
    }

    pub fn cloud(site: &str, email: &str, token: &str) -> Self {
        Self::remote(
            JiraMode::Cloud,
            site,
            Auth::Basic {
                email: email.into(),
                token: token.into(),
            },
        )
    }

    pub fn data_center(site: &str, token: &str) -> Self {
        Self::remote(JiraMode::DataCenter, site, Auth::Bearer(token.into()))
    }

    fn remote(mode: JiraMode, site: &str, auth: Auth) -> Self {
        Self::Remote {
            mode,
            site: site.trim_end_matches('/').to_string(),
            auth,
            // reqwest honours HTTPS_PROXY / NO_PROXY, as corporate networks need.
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .build()
                .expect("HTTP client"),
        }
    }

    /// Display name of the connected account: a cheap check of the credentials.
    pub async fn myself(&self) -> Result<String, JiraError> {
        match self {
            Self::Sample => Ok("Datos de ejemplo".into()),
            Self::Remote { .. } => {
                let me = self.get("myself", &[]).await?;
                Ok(me["displayName"].as_str().unwrap_or("?").to_string())
            }
        }
    }

    pub async fn search(&self, jql: &str) -> Result<Vec<JiraIssue>, JiraError> {
        let body = match self {
            Self::Sample => serde_json::from_str(SAMPLE).expect("valid sample"),
            // Cloud retired `/search` in favour of `/search/jql`.
            Self::Remote {
                mode: JiraMode::Cloud,
                ..
            } => {
                self.get(
                    "search/jql",
                    &[
                        ("jql", jql),
                        ("fields", FIELDS),
                        ("maxResults", MAX_RESULTS),
                    ],
                )
                .await?
            }
            Self::Remote { .. } => {
                self.get(
                    "search",
                    &[
                        ("jql", jql),
                        ("fields", FIELDS),
                        ("maxResults", MAX_RESULTS),
                    ],
                )
                .await?
            }
        };
        let issues = body["issues"]
            .as_array()
            .ok_or_else(|| JiraError::Parse("no `issues` list".into()))?;
        issues.iter().map(|issue| self.map_issue(issue)).collect()
    }

    pub async fn issue(&self, key: &str) -> Result<JiraIssue, JiraError> {
        match self {
            Self::Sample => self
                .search("")
                .await?
                .into_iter()
                .find(|i| i.key == key)
                .ok_or_else(|| JiraError::NotInSample(key.into())),
            Self::Remote { .. } => {
                let issue = self
                    .get(&format!("issue/{key}"), &[("fields", FIELDS)])
                    .await?;
                self.map_issue(&issue)
            }
        }
    }

    async fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value, JiraError> {
        let Self::Remote {
            mode,
            site,
            auth,
            http,
        } = self
        else {
            unreachable!("sample data never calls Jira")
        };
        let version = if *mode == JiraMode::Cloud { "3" } else { "2" };
        let req = http
            .get(format!("{site}/rest/api/{version}/{path}"))
            .query(query);
        let req = match auth {
            Auth::Basic { email, token } => req.basic_auth(email, Some(token)),
            Auth::Bearer(token) => req.bearer_auth(token),
        };
        let res = req
            .header("accept", "application/json")
            .send()
            .await
            .map_err(|e| JiraError::Network(e.to_string()))?;
        let status = res.status().as_u16();
        if status == 401 || status == 403 {
            return Err(JiraError::Unauthorized(status));
        }
        let text = res
            .text()
            .await
            .map_err(|e| JiraError::Network(e.to_string()))?;
        if !(200..300).contains(&status) {
            return Err(JiraError::Http {
                status,
                message: error_message(&text),
            });
        }
        serde_json::from_str(&text).map_err(|e| JiraError::Parse(e.to_string()))
    }

    fn map_issue(&self, issue: &Value) -> Result<JiraIssue, JiraError> {
        let site = match self {
            Self::Remote { site, .. } => Some(site.as_str()),
            Self::Sample => None,
        };
        map_issue(issue, site)
    }
}

/// Maps one issue of a v2 or v3 response. Cloud descriptions are ADF documents,
/// Data Center ones plain (wiki) text.
pub fn map_issue(issue: &Value, site: Option<&str>) -> Result<JiraIssue, JiraError> {
    let key = issue["key"]
        .as_str()
        .ok_or_else(|| JiraError::Parse("issue without key".into()))?;
    let f = &issue["fields"];
    let name = |v: &Value| v["name"].as_str().map(str::to_string);
    let description = match &f["description"] {
        Value::String(text) => text.clone(),
        doc @ Value::Object(_) => adf_to_text(doc),
        _ => String::new(),
    };
    Ok(JiraIssue {
        key: key.to_string(),
        summary: f["summary"].as_str().unwrap_or("").to_string(),
        issue_type: name(&f["issuetype"]).unwrap_or_default(),
        priority: name(&f["priority"]),
        status: name(&f["status"]).unwrap_or_default(),
        assignee: f["assignee"]["displayName"].as_str().map(str::to_string),
        components: f["components"]
            .as_array()
            .map(|c| c.iter().filter_map(name).collect())
            .unwrap_or_default(),
        description,
        url: site.map(|s| format!("{s}/browse/{key}")),
    })
}

fn error_message(body: &str) -> String {
    let parsed: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let messages: Vec<&str> = parsed["errorMessages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if messages.is_empty() {
        body.chars().take(200).collect()
    } else {
        messages.join("; ")
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn the_sample_has_tickets_with_readable_descriptions() {
        let jira = JiraClient::sample();
        let issues = jira.search("assignee = currentUser()").await.unwrap();
        assert!(issues.len() >= 4);

        let dup = jira.issue("PROJ-1242").await.unwrap();
        assert_eq!(dup.components, ["svc-payments", "svc-orders"]);
        assert!(dup.description.contains("### Criterios de aceptación"));
        assert!(dup.description.contains("- Un segundo cobro"));
        assert!(matches!(
            jira.issue("NOPE-1").await,
            Err(JiraError::NotInSample(_))
        ));
    }

    #[test]
    fn maps_cloud_and_data_center_issues() {
        let cloud = json!({ "key": "PROJ-1", "fields": {
            "summary": "S", "issuetype": { "name": "Bug" }, "priority": { "name": "Alta" },
            "status": { "name": "Por hacer" }, "assignee": null, "components": [],
            "description": { "type": "doc", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "ADF" }] }] }
        }});
        let issue = map_issue(&cloud, Some("https://acme.atlassian.net")).unwrap();
        assert_eq!(issue.description, "ADF");
        assert_eq!(issue.assignee, None);
        assert_eq!(
            issue.url.as_deref(),
            Some("https://acme.atlassian.net/browse/PROJ-1")
        );

        let dc = json!({ "key": "OPS-2", "fields": {
            "summary": "S", "issuetype": { "name": "Task" }, "status": { "name": "Open" },
            "description": "h3. Plain wiki text"
        }});
        let issue = map_issue(&dc, None).unwrap();
        assert_eq!(issue.description, "h3. Plain wiki text");
        assert_eq!(issue.priority, None);
        assert!(issue.components.is_empty());
    }

    #[test]
    fn reads_jira_error_messages() {
        assert_eq!(
            error_message(r#"{"errorMessages":["Field 'sprint' does not exist"]}"#),
            "Field 'sprint' does not exist"
        );
        assert_eq!(
            error_message("<html>gateway</html>"),
            "<html>gateway</html>"
        );
    }
}
