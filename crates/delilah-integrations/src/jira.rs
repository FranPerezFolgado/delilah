//! Jira client: Cloud (REST v3, ADF descriptions), Server/Data Center (REST v2) or
//! built-in sample data for developing without access to a real site.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::adf::adf_to_text;

const FIELDS: &str =
    "summary,issuetype,priority,status,assignee,reporter,parent,components,description";
const MAX_RESULTS: &str = "50";
const SAMPLE: &str = include_str!("../fixtures/jira-search.json");
/// Who the sample data treats as the connected account.
const SAMPLE_ME: &str = "Fran";
/// Data Center keeps the epic in a custom field whose id varies per site.
const EPIC_LINK_SCHEMA: &str = "com.pyxis.greenhopper.jira:gh-epic-link";

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
    /// Jira's bucket for the status: `new`, `indeterminate` or `done`.
    #[serde(default)]
    pub status_category: String,
    pub assignee: Option<String>,
    #[serde(default)]
    pub reporter: Option<String>,
    #[serde(default)]
    pub epic: Option<JiraEpic>,
    pub components: Vec<String>,
    /// Plain text with light Markdown.
    pub description: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JiraEpic {
    pub key: String,
    /// Data Center's epic link gives only the key until the epic is looked up.
    pub summary: Option<String>,
}

/// Whose tickets: the ones assigned to the account, the ones it reported, or the
/// JQL saved with the connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Who {
    #[default]
    Assigned,
    Reported,
    Query,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Progress {
    #[default]
    Open,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub struct Filter {
    #[serde(default)]
    pub who: Who,
    #[serde(default)]
    pub progress: Progress,
}

impl Filter {
    /// The JQL for this filter; `saved` is the connection's own query, used by
    /// [`Who::Query`] and keeping its `ORDER BY` if it has one.
    pub fn jql(&self, saved: &str) -> String {
        let (saved_where, saved_order) = split_order_by(saved);
        let who = match self.who {
            Who::Assigned => "assignee = currentUser()".to_string(),
            Who::Reported => "reporter = currentUser()".to_string(),
            Who::Query if saved_where.is_empty() => String::new(),
            Who::Query => format!("({saved_where})"),
        };
        let progress = match self.progress {
            Progress::Open => "statusCategory != Done",
            Progress::Done => "statusCategory = Done",
        };
        let order = match (self.who, saved_order) {
            (Who::Query, Some(order)) => order,
            _ => "ORDER BY updated DESC",
        };
        if who.is_empty() {
            format!("{progress} {order}")
        } else {
            format!("{who} AND {progress} {order}")
        }
    }

    /// The sample data's stand-in for the JQL.
    fn matches(&self, issue: &JiraIssue) -> bool {
        let me = Some(SAMPLE_ME);
        let who = match self.who {
            Who::Assigned => issue.assignee.as_deref() == me,
            Who::Reported => issue.reporter.as_deref() == me,
            Who::Query => true,
        };
        who && (issue.status_category == "done") == (self.progress == Progress::Done)
    }
}

/// `(where, Some("ORDER BY …"))`, splitting at the last `ORDER BY`, any case.
fn split_order_by(jql: &str) -> (&str, Option<&str>) {
    let jql = jql.trim();
    match jql.to_ascii_uppercase().rfind("ORDER BY") {
        Some(at) => (jql[..at].trim(), Some(jql[at..].trim())),
        None => (jql, None),
    }
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
            site: site_root(site),
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

    /// The tickets for `filter`, with every epic's title.
    pub async fn list(
        &self,
        filter: &Filter,
        saved_jql: &str,
    ) -> Result<Vec<JiraIssue>, JiraError> {
        if let Self::Sample = self {
            let mut issues = self.search("").await?;
            issues.retain(|i| filter.matches(i));
            return Ok(issues);
        }
        let mut issues = self.search(&filter.jql(saved_jql)).await?;
        self.name_epics(&mut issues).await?;
        Ok(issues)
    }

    /// Fills in the titles of epics known only by key (Data Center's epic link).
    async fn name_epics(&self, issues: &mut [JiraIssue]) -> Result<(), JiraError> {
        let mut keys: Vec<&str> = issues
            .iter()
            .filter_map(|i| i.epic.as_ref())
            .filter(|e| e.summary.is_none())
            .map(|e| e.key.as_str())
            .collect();
        keys.sort_unstable();
        keys.dedup();
        if keys.is_empty() {
            return Ok(());
        }
        let jql = format!("key in ({})", keys.join(","));
        let epics = self.search(&jql).await?;
        for issue in issues.iter_mut() {
            if let Some(epic) = issue.epic.as_mut()
                && let Some(found) = epics.iter().find(|e| e.key == epic.key)
            {
                epic.summary = Some(found.summary.clone());
            }
        }
        Ok(())
    }

    /// Data Center: the id of the epic link field, if the site has Jira Software.
    async fn epic_link_field(&self) -> Option<String> {
        let fields = self.get("field", &[]).await.ok()?;
        fields
            .as_array()?
            .iter()
            .find(|f| f["schema"]["custom"] == EPIC_LINK_SCHEMA)
            .and_then(|f| f["id"].as_str())
            .map(str::to_string)
    }

    pub async fn search(&self, jql: &str) -> Result<Vec<JiraIssue>, JiraError> {
        let mut epic_field = None;
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
                epic_field = self.epic_link_field().await;
                let fields = match &epic_field {
                    Some(id) => format!("{FIELDS},{id}"),
                    None => FIELDS.to_string(),
                };
                self.get(
                    "search",
                    &[
                        ("jql", jql),
                        ("fields", &fields),
                        ("maxResults", MAX_RESULTS),
                    ],
                )
                .await?
            }
        };
        let issues = body["issues"]
            .as_array()
            .ok_or_else(|| JiraError::Parse("no `issues` list".into()))?;
        issues
            .iter()
            .map(|issue| self.map_issue(issue, epic_field.as_deref()))
            .collect()
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
                self.map_issue(&issue, None)
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
        if text.trim_start().starts_with('<') {
            // A login or web page instead of the API: the URL is not the site's.
            return Err(JiraError::Parse(
                "Jira answered with a web page, not data: check the site URL (just https://your-company.atlassian.net)".into(),
            ));
        }
        serde_json::from_str(&text).map_err(|e| JiraError::Parse(e.to_string()))
    }

    fn map_issue(&self, issue: &Value, epic_field: Option<&str>) -> Result<JiraIssue, JiraError> {
        let site = match self {
            Self::Remote { site, .. } => Some(site.as_str()),
            Self::Sample => None,
        };
        map_issue(issue, site, epic_field)
    }
}

/// Maps one issue of a v2 or v3 response. Cloud descriptions are ADF documents,
/// Data Center ones plain (wiki) text.
/// The site's base URL from whatever was pasted: a board, a ticket or a filter page
/// all work. Jira Cloud keeps only the host; Server/Data Center keeps its context path
/// (e.g. `https://intranet.example.com/jira`).
pub fn site_root(pasted: &str) -> String {
    let mut url = pasted.trim().to_string();
    if !url.contains("://") {
        url = format!("https://{url}");
    }
    let url = url.split(['?', '#']).next().unwrap_or_default();
    let (scheme, rest) = url.split_once("://").unwrap_or(("https", url));
    let (host, path) = rest.split_once('/').map_or((rest, ""), |(h, p)| (h, p));
    if host.ends_with(".atlassian.net") {
        return format!("{scheme}://{host}");
    }
    // Where Jira's own pages start; anything before is the context path.
    const PAGES: &[&str] = &[
        "browse",
        "secure",
        "projects",
        "plugins",
        "issues",
        "rest",
        "software",
        "servicedesk",
        "dashboards",
    ];
    let context: Vec<&str> = path
        .split('/')
        .filter(|s| !s.is_empty())
        .take_while(|s| !PAGES.contains(s))
        .collect();
    if context.is_empty() {
        format!("{scheme}://{host}")
    } else {
        format!("{scheme}://{host}/{}", context.join("/"))
    }
}

/// `epic_field` is Data Center's epic link field; Cloud gives the epic as `parent`.
pub fn map_issue(
    issue: &Value,
    site: Option<&str>,
    epic_field: Option<&str>,
) -> Result<JiraIssue, JiraError> {
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
        status_category: f["status"]["statusCategory"]["key"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        assignee: f["assignee"]["displayName"].as_str().map(str::to_string),
        reporter: f["reporter"]["displayName"].as_str().map(str::to_string),
        epic: epic(f, epic_field),
        components: f["components"]
            .as_array()
            .map(|c| c.iter().filter_map(name).collect())
            .unwrap_or_default(),
        description,
        url: site.map(|s| format!("{s}/browse/{key}")),
    })
}

/// The epic a ticket belongs to: a `parent` one level up (Cloud, where epics have
/// hierarchy level 1) or Data Center's epic link.
fn epic(fields: &Value, epic_field: Option<&str>) -> Option<JiraEpic> {
    let parent = &fields["parent"];
    let kind = &parent["fields"]["issuetype"];
    let is_epic =
        kind["hierarchyLevel"] == 1 || matches!(kind["name"].as_str(), Some("Epic" | "Épica"));
    if is_epic && let Some(key) = parent["key"].as_str() {
        return Some(JiraEpic {
            key: key.to_string(),
            summary: parent["fields"]["summary"].as_str().map(str::to_string),
        });
    }
    let key = fields[epic_field?].as_str()?;
    Some(JiraEpic {
        key: key.to_string(),
        summary: None,
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
        let issue = map_issue(&cloud, Some("https://acme.atlassian.net"), None).unwrap();
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
        let issue = map_issue(&dc, None, None).unwrap();
        assert_eq!(issue.description, "h3. Plain wiki text");
        assert_eq!(issue.priority, None);
        assert!(issue.components.is_empty());
    }

    #[test]
    fn finds_the_epic_in_cloud_and_data_center() {
        let cloud = json!({ "key": "PROJ-1", "fields": {
            "status": { "name": "En curso", "statusCategory": { "key": "indeterminate" } },
            "reporter": { "displayName": "Ana" },
            "parent": { "key": "PROJ-9", "fields": { "summary": "Pagos",
                "issuetype": { "name": "Epic", "hierarchyLevel": 1 } } }
        }});
        let issue = map_issue(&cloud, None, None).unwrap();
        assert_eq!(issue.status_category, "indeterminate");
        assert_eq!(issue.reporter.as_deref(), Some("Ana"));
        assert_eq!(
            issue.epic,
            Some(JiraEpic {
                key: "PROJ-9".into(),
                summary: Some("Pagos".into())
            })
        );

        // A sub-task's parent is a story, not an epic.
        let subtask = json!({ "key": "PROJ-2", "fields": {
            "parent": { "key": "PROJ-1", "fields": { "issuetype": { "name": "Story", "hierarchyLevel": 0 } } }
        }});
        assert_eq!(map_issue(&subtask, None, None).unwrap().epic, None);

        let dc = json!({ "key": "OPS-2", "fields": { "customfield_10100": "OPS-1" } });
        let issue = map_issue(&dc, None, Some("customfield_10100")).unwrap();
        assert_eq!(issue.epic.unwrap().key, "OPS-1");
    }

    #[test]
    fn builds_the_jql_for_each_filter() {
        let f = |who, progress| Filter { who, progress };
        let saved = "project = OPS and labels = api order by priority desc";
        assert_eq!(
            f(Who::Assigned, Progress::Open).jql(saved),
            "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC"
        );
        assert_eq!(
            f(Who::Reported, Progress::Done).jql(saved),
            "reporter = currentUser() AND statusCategory = Done ORDER BY updated DESC"
        );
        assert_eq!(
            f(Who::Query, Progress::Open).jql(saved),
            "(project = OPS and labels = api) AND statusCategory != Done order by priority desc"
        );
        assert_eq!(
            f(Who::Query, Progress::Done).jql(""),
            "statusCategory = Done ORDER BY updated DESC"
        );
    }

    #[tokio::test]
    async fn the_sample_answers_each_filter() {
        let jira = JiraClient::sample();
        let keys = |issues: Vec<JiraIssue>| issues.into_iter().map(|i| i.key).collect::<Vec<_>>();
        let f = |who, progress| Filter { who, progress };
        let mine = keys(
            jira.list(&f(Who::Assigned, Progress::Open), "")
                .await
                .unwrap(),
        );
        assert!(mine.contains(&"PROJ-1242".to_string()));
        assert!(!mine.contains(&"PROJ-1245".to_string()), "unassigned");
        assert!(!mine.contains(&"PROJ-1238".to_string()), "done");
        let reported = keys(
            jira.list(&f(Who::Reported, Progress::Open), "")
                .await
                .unwrap(),
        );
        assert!(reported.contains(&"PROJ-1247".to_string()));
        let done = keys(
            jira.list(&f(Who::Assigned, Progress::Done), "")
                .await
                .unwrap(),
        );
        assert_eq!(done, ["PROJ-1238", "PROJ-1231"]);
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

    #[test]
    fn a_pasted_board_or_ticket_url_becomes_the_site() {
        assert_eq!(
            site_root("https://acme.atlassian.net/jira/software/c/projects/ADM/boards/700"),
            "https://acme.atlassian.net"
        );
        assert_eq!(
            site_root("acme.atlassian.net/browse/ADM-12?x=1"),
            "https://acme.atlassian.net"
        );
        assert_eq!(
            site_root("https://intranet.acme.com/jira/browse/ADM-12"),
            "https://intranet.acme.com/jira"
        );
        assert_eq!(site_root("https://jira.acme.com/"), "https://jira.acme.com");
    }
}
