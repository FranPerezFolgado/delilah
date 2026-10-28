//! Jira client: Cloud (REST v3, ADF descriptions), Server/Data Center (REST v2) or
//! built-in sample data for developing without access to a real site.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::adf::adf_to_text;

const FIELDS: &str =
    "summary,issuetype,priority,status,assignee,reporter,parent,components,description,subtasks";
const MAX_RESULTS: usize = 50;
/// The sub-tasks of a listing come in one more search, with room for more of them.
const MAX_SUBTASKS: usize = 500;
/// How many sub-tasks `subtasks` returns for one parent.
const MAX_PARENT_SUBTASKS: usize = 50;
/// The sample parent with many children (a long list to scroll and group).
const SAMPLE_BIG_PARENT: &str = "PROJ-1244";
const SAMPLE_BIG_CHILDREN: usize = 20;
/// What Jira hands over per request, at most.
const PAGE_SIZE: usize = 100;
const SAMPLE: &str = include_str!("../fixtures/jira-search.json");
/// Who the sample data treats as the connected account.
const SAMPLE_ME: &str = "Fran";
/// The sample's sprint field, as Jira Cloud usually numbers it.
const SAMPLE_SPRINT_FIELD: &str = "customfield_10020";
/// Data Center keeps the epic in a custom field whose id varies per site.
const EPIC_LINK_SCHEMA: &str = "com.pyxis.greenhopper.jira:gh-epic-link";
/// Jira Software's sprint field, also with a per-site id.
const SPRINT_SCHEMA: &str = "com.pyxis.greenhopper.jira:gh-sprint";

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
    /// The story or task above a sub-task; an epic parent goes in `epic` instead.
    #[serde(default)]
    pub parent: Option<JiraParent>,
    #[serde(default)]
    pub subtask: bool,
    /// How many sub-tasks hang from this ticket, and how many of them are done.
    #[serde(default)]
    pub subtask_count: u32,
    #[serde(default)]
    pub subtasks_done: u32,
    /// The open sprint, or else the next one, or else the last it went through.
    #[serde(default)]
    pub sprint: Option<JiraSprint>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JiraParent {
    pub key: String,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JiraSprint {
    pub name: String,
    /// `active`, `future` or `closed`.
    pub state: String,
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

/// Every ticket, or only the ones in an open sprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SprintFilter {
    #[default]
    All,
    Current,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub struct Filter {
    #[serde(default)]
    pub who: Who,
    #[serde(default)]
    pub progress: Progress,
    #[serde(default)]
    pub sprint: SprintFilter,
}

impl Filter {
    /// The JQL for this filter; `saved` is the connection's own query, used by
    /// [`Who::Query`] and keeping its `ORDER BY` if it has one.
    pub fn jql(&self, saved: &str) -> String {
        let (saved_where, saved_order) = split_order_by(saved);
        let mut terms = Vec::new();
        match self.who {
            Who::Assigned => terms.push("assignee = currentUser()".to_string()),
            Who::Reported => terms.push("reporter = currentUser()".to_string()),
            Who::Query if saved_where.is_empty() => {}
            Who::Query => terms.push(format!("({saved_where})")),
        }
        terms.push(
            match self.progress {
                Progress::Open => "statusCategory != Done",
                Progress::Done => "statusCategory = Done",
            }
            .to_string(),
        );
        // Every started sprint the account can see (there may be parallel boards);
        // `openSprints()` alone also takes the ones not started yet.
        if self.sprint == SprintFilter::Current {
            terms.push("sprint in openSprints() AND sprint not in futureSprints()".to_string());
        }
        let order = match (self.who, saved_order) {
            (Who::Query, Some(order)) => order,
            _ => "ORDER BY updated DESC",
        };
        format!("{} {order}", terms.join(" AND "))
    }

    /// The sample data's stand-in for the JQL.
    fn matches(&self, issue: &JiraIssue) -> bool {
        let me = Some(SAMPLE_ME);
        let who = match self.who {
            Who::Assigned => issue.assignee.as_deref() == me,
            Who::Reported => issue.reporter.as_deref() == me,
            Who::Query => true,
        };
        let sprint = match self.sprint {
            SprintFilter::All => true,
            SprintFilter::Current => issue.sprint.as_ref().is_some_and(|s| s.state == "active"),
        };
        who && sprint && (issue.status_category == "done") == (self.progress == Progress::Done)
    }
}

/// The ids of the custom fields this client reads; each is `None` where the site
/// lacks it (no Jira Software, say).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CustomFields {
    pub epic_link: Option<String>,
    pub sprint: Option<String>,
}

impl CustomFields {
    /// `FIELDS` plus these.
    fn fields(&self) -> String {
        [&self.epic_link, &self.sprint]
            .into_iter()
            .flatten()
            .fold(FIELDS.to_string(), |all, id| format!("{all},{id}"))
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
    #[error("{0} is not a Jira issue key")]
    BadKey(String),
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

    /// The tickets for `filter` and all their sub-tasks, whoever has them, with
    /// every epic's title and each sub-task under its parent's epic.
    pub async fn list(
        &self,
        filter: &Filter,
        saved_jql: &str,
    ) -> Result<Vec<JiraIssue>, JiraError> {
        let custom = self.custom_fields().await;
        if let Self::Sample = self {
            let all = self.query("", MAX_RESULTS, &custom).await?;
            let mut issues: Vec<JiraIssue> = all
                .iter()
                .filter(|i| !i.subtask && filter.matches(i))
                .cloned()
                .collect();
            let subtasks: Vec<JiraIssue> = all
                .iter()
                .filter(|i| i.subtask)
                .filter(|i| filter.matches(i) || parent_listed(i, &issues))
                .cloned()
                .collect();
            issues.extend(subtasks);
            inherit_epics(&mut issues, &all);
            return Ok(issues);
        }
        let mut issues = self
            .query(&filter.jql(saved_jql), MAX_RESULTS, &custom)
            .await?;
        // Sub-tasks and the epics they inherit are extras: if Jira refuses them,
        // the tickets are still worth showing.
        if let Err(e) = self.add_subtasks(&mut issues, &custom).await {
            tracing::warn!("Jira sub-tasks left out: {e}");
        }
        let parents = self
            .missing_parents(&issues, &custom)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("Jira sub-tasks' epics left out: {e}");
                Vec::new()
            });
        inherit_epics(&mut issues, &parents);
        self.name_epics(&mut issues, &custom).await?;
        Ok(issues)
    }

    /// Appends the sub-tasks of every listed ticket that are not listed yet.
    async fn add_subtasks(
        &self,
        issues: &mut Vec<JiraIssue>,
        custom: &CustomFields,
    ) -> Result<(), JiraError> {
        let Some(parents) = keys_jql(issues.iter().filter(|i| !i.subtask).map(|i| i.key.as_str()))
        else {
            return Ok(());
        };
        let parents = format!("parent in {parents}");
        // On Cloud an epic's stories are its children too, hence the type filter;
        // a site without it answers 400 and the type is checked here instead.
        let typed = format!("{parents} AND issuetype in subTaskIssueTypes()");
        let found = match self.query(&typed, MAX_SUBTASKS, custom).await {
            Err(JiraError::Http { status: 400, .. }) => {
                self.query(&parents, MAX_SUBTASKS, custom).await?
            }
            found => found?,
        };
        let listed: HashSet<String> = issues.iter().map(|i| i.key.clone()).collect();
        issues.extend(
            found
                .into_iter()
                .filter(|i| i.subtask && !listed.contains(&i.key)),
        );
        Ok(())
    }

    /// The parents of listed sub-tasks that are not listed themselves.
    async fn missing_parents(
        &self,
        issues: &[JiraIssue],
        custom: &CustomFields,
    ) -> Result<Vec<JiraIssue>, JiraError> {
        let listed: HashSet<&str> = issues.iter().map(|i| i.key.as_str()).collect();
        let keys = issues
            .iter()
            .filter(|i| i.epic.is_none())
            .filter_map(|i| i.parent.as_ref())
            .map(|p| p.key.as_str())
            .filter(|key| !listed.contains(key));
        match keys_jql(keys) {
            Some(keys) => {
                self.query(&format!("key in {keys}"), MAX_RESULTS, custom)
                    .await
            }
            None => Ok(Vec::new()),
        }
    }

    /// Fills in the titles of epics known only by key (Data Center's epic link).
    async fn name_epics(
        &self,
        issues: &mut [JiraIssue],
        custom: &CustomFields,
    ) -> Result<(), JiraError> {
        let keys = issues
            .iter()
            .filter_map(|i| i.epic.as_ref())
            .filter(|e| e.summary.is_none())
            .map(|e| e.key.as_str());
        let Some(keys) = keys_jql(keys) else {
            return Ok(());
        };
        let epics = self
            .query(&format!("key in {keys}"), MAX_RESULTS, custom)
            .await?;
        for issue in issues.iter_mut() {
            if let Some(epic) = issue.epic.as_mut()
                && let Some(found) = epics.iter().find(|e| e.key == epic.key)
            {
                epic.summary = Some(found.summary.clone());
            }
        }
        Ok(())
    }

    /// The ids of the epic link and sprint fields; none if the site won't say.
    async fn custom_fields(&self) -> CustomFields {
        if let Self::Sample = self {
            return CustomFields {
                epic_link: None,
                sprint: Some(SAMPLE_SPRINT_FIELD.into()),
            };
        }
        let Ok(fields) = self.get("field", &[]).await else {
            return CustomFields::default();
        };
        let id = |schema: &str| {
            fields
                .as_array()?
                .iter()
                .find(|f| f["schema"]["custom"] == schema)
                .and_then(|f| f["id"].as_str())
                .map(str::to_string)
        };
        CustomFields {
            epic_link: id(EPIC_LINK_SCHEMA),
            sprint: id(SPRINT_SCHEMA),
        }
    }

    pub async fn search(&self, jql: &str) -> Result<Vec<JiraIssue>, JiraError> {
        let custom = self.custom_fields().await;
        self.query(jql, MAX_RESULTS, &custom).await
    }

    /// Up to `limit` tickets for `jql`, a page at a time.
    async fn query(
        &self,
        jql: &str,
        limit: usize,
        custom: &CustomFields,
    ) -> Result<Vec<JiraIssue>, JiraError> {
        let fields = custom.fields();
        let mut issues = Vec::new();
        // Cloud pages with a token, Data Center with an offset.
        let mut token: Option<String> = None;
        loop {
            let size = (limit - issues.len()).min(PAGE_SIZE).to_string();
            let start = issues.len().to_string();
            let mut query = vec![
                ("jql", jql),
                ("fields", fields.as_str()),
                ("maxResults", size.as_str()),
            ];
            let body = match self {
                Self::Sample => sample_body(),
                // Cloud retired `/search` in favour of `/search/jql`.
                Self::Remote {
                    mode: JiraMode::Cloud,
                    ..
                } => {
                    if let Some(token) = &token {
                        query.push(("nextPageToken", token));
                    }
                    self.get("search/jql", &query).await?
                }
                Self::Remote { .. } => {
                    query.push(("startAt", &start));
                    self.get("search", &query).await?
                }
            };
            let page = body["issues"]
                .as_array()
                .ok_or_else(|| JiraError::Parse("no `issues` list".into()))?;
            for issue in page {
                issues.push(self.map_issue(issue, custom)?);
            }
            let more = match self {
                Self::Sample => false,
                Self::Remote {
                    mode: JiraMode::Cloud,
                    ..
                } => {
                    token = body["nextPageToken"].as_str().map(str::to_string);
                    body["isLast"] != true && token.is_some()
                }
                Self::Remote { .. } => body["total"].as_u64().unwrap_or(0) > issues.len() as u64,
            };
            if !more || page.is_empty() || issues.len() >= limit {
                issues.truncate(limit);
                if let Self::Sample = self {
                    count_sample_subtasks(&mut issues);
                }
                return Ok(issues);
            }
        }
    }

    /// The sub-tasks of `parent` whoever has them and whatever their status, at most 50.
    /// Fails if the parent does not exist.
    pub async fn subtasks(&self, parent: &str) -> Result<Vec<JiraIssue>, JiraError> {
        // The key goes into a JQL string: only an issue key may.
        if !is_issue_key(parent) {
            return Err(JiraError::BadKey(parent.into()));
        }
        self.issue(parent).await?;
        let custom = self.custom_fields().await;
        match self {
            Self::Sample => {
                let all = self.query("", MAX_RESULTS, &custom).await?;
                Ok(all
                    .into_iter()
                    .filter(|i| i.parent.as_ref().is_some_and(|p| p.key == parent))
                    .take(MAX_PARENT_SUBTASKS)
                    .collect())
            }
            Self::Remote { .. } => {
                self.query(&format!("parent = {parent}"), MAX_PARENT_SUBTASKS, &custom)
                    .await
            }
        }
    }

    /// Keys of the sub-tasks of `parent` assigned to the current user.
    /// Fails if the parent does not exist.
    pub async fn my_subtask_keys(&self, parent: &str) -> Result<HashSet<String>, JiraError> {
        // The key goes into a JQL string: only an issue key may.
        if !is_issue_key(parent) {
            return Err(JiraError::BadKey(parent.into()));
        }
        self.issue(parent).await?;
        match self {
            Self::Sample => {
                let custom = self.custom_fields().await;
                let all = self.query("", MAX_RESULTS, &custom).await?;
                Ok(all
                    .into_iter()
                    .filter(|i| {
                        i.parent.as_ref().is_some_and(|p| p.key == parent)
                            && i.assignee.as_deref() == Some(SAMPLE_ME)
                    })
                    .map(|i| i.key)
                    .collect())
            }
            Self::Remote { mode, .. } => {
                let jql = format!("parent = {parent} AND assignee = currentUser()");
                let mut keys = HashSet::new();
                let mut token: Option<String> = None;
                loop {
                    let start = keys.len().to_string();
                    let mut query = vec![
                        ("jql", jql.as_str()),
                        ("fields", "key"),
                        ("maxResults", "100"),
                    ];
                    let body = if matches!(mode, JiraMode::Cloud) {
                        if let Some(token) = &token {
                            query.push(("nextPageToken", token));
                        }
                        self.get("search/jql", &query).await?
                    } else {
                        query.push(("startAt", &start));
                        self.get("search", &query).await?
                    };
                    let page = body["issues"]
                        .as_array()
                        .ok_or_else(|| JiraError::Parse("no `issues` list".into()))?;
                    keys.extend(
                        page.iter()
                            .filter_map(|i| i["key"].as_str().map(str::to_string)),
                    );
                    let more = if matches!(mode, JiraMode::Cloud) {
                        token = body["nextPageToken"].as_str().map(str::to_string);
                        body["isLast"] != true && token.is_some()
                    } else {
                        body["total"].as_u64().unwrap_or(0) > keys.len() as u64
                    };
                    if !more || page.is_empty() {
                        return Ok(keys);
                    }
                }
            }
        }
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
                let custom = self.custom_fields().await;
                let issue = self
                    .get(&format!("issue/{key}"), &[("fields", &custom.fields())])
                    .await?;
                self.map_issue(&issue, &custom)
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

    fn map_issue(&self, issue: &Value, custom: &CustomFields) -> Result<JiraIssue, JiraError> {
        let site = match self {
            Self::Remote { site, .. } => Some(site.as_str()),
            Self::Sample => None,
        };
        map_issue(issue, site, custom)
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

/// `custom` names the site's epic link (Data Center; Cloud gives the epic as
/// `parent`) and sprint fields.
pub fn map_issue(
    issue: &Value,
    site: Option<&str>,
    custom: &CustomFields,
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
    let subtask = f["issuetype"]["subtask"] == true || f["issuetype"]["hierarchyLevel"] == -1;
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
        epic: epic(f, custom.epic_link.as_deref()),
        parent: subtask.then(|| parent(f)).flatten(),
        subtask,
        subtask_count: f["subtasks"].as_array().map_or(0, |s| s.len() as u32),
        subtasks_done: f["subtasks"].as_array().map_or(0, |s| {
            s.iter()
                .filter(|t| t["fields"]["status"]["statusCategory"]["key"] == "done")
                .count() as u32
        }),
        sprint: sprint(f, custom.sprint.as_deref()),
        components: f["components"]
            .as_array()
            .map(|c| c.iter().filter_map(name).collect())
            .unwrap_or_default(),
        description,
        url: site.map(|s| format!("{s}/browse/{key}")),
    })
}

fn parent_is_epic(fields: &Value) -> bool {
    let kind = &fields["parent"]["fields"]["issuetype"];
    kind["hierarchyLevel"] == 1 || matches!(kind["name"].as_str(), Some("Epic" | "Épica"))
}

/// The epic a ticket belongs to: a `parent` one level up (Cloud, where epics have
/// hierarchy level 1) or Data Center's epic link.
fn epic(fields: &Value, epic_field: Option<&str>) -> Option<JiraEpic> {
    let parent = &fields["parent"];
    if parent_is_epic(fields)
        && let Some(key) = parent["key"].as_str()
    {
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

/// A sub-task's story or task; `None` when the parent is an epic.
fn parent(fields: &Value) -> Option<JiraParent> {
    let parent = &fields["parent"];
    if parent_is_epic(fields) {
        return None;
    }
    Some(JiraParent {
        key: parent["key"].as_str()?.to_string(),
        summary: parent["fields"]["summary"].as_str().map(str::to_string),
    })
}

/// The sprint that matters among those the ticket went through: the open one,
/// else the next, else the last.
fn sprint(fields: &Value, sprint_field: Option<&str>) -> Option<JiraSprint> {
    let sprints: Vec<JiraSprint> = fields[sprint_field?]
        .as_array()?
        .iter()
        .filter_map(|s| match s {
            Value::String(text) => data_center_sprint(text),
            _ => Some(JiraSprint {
                name: s["name"].as_str()?.to_string(),
                state: s["state"].as_str().unwrap_or_default().to_ascii_lowercase(),
            }),
        })
        .collect();
    ["active", "future"]
        .iter()
        .find_map(|state| sprints.iter().find(|s| s.state == *state))
        .or(sprints.last())
        .cloned()
}

/// Older Data Center sites send each sprint as a Java `toString()`:
/// `…Sprint@1a2b[id=7,rapidViewId=3,state=ACTIVE,name=Sprint 42,startDate=…]`.
fn data_center_sprint(text: &str) -> Option<JiraSprint> {
    let body = text.split_once('[')?.1;
    let body = body.strip_suffix(']').unwrap_or(body);
    // A value runs up to the next `,word=`: a name may hold commas, not that.
    let value = |attr: &str| {
        let start = body.find(&format!("{attr}="))? + attr.len() + 1;
        let rest = &body[start..];
        let end = rest
            .match_indices(',')
            .map(|(at, _)| at)
            .find(|&at| {
                let next = &rest[at + 1..];
                let word = next.len()
                    - next
                        .trim_start_matches(|c: char| c.is_ascii_alphabetic())
                        .len();
                word > 0 && next[word..].starts_with('=')
            })
            .unwrap_or(rest.len());
        Some(rest[..end].to_string())
    };
    Some(JiraSprint {
        name: value("name")?,
        state: value("state").unwrap_or_default().to_ascii_lowercase(),
    })
}

/// `(K-1,K-2)` for a JQL `in`, without repeats and without anything that is not an
/// issue key, so a key can never change the query; `None` if none is left.
fn keys_jql<'a>(keys: impl Iterator<Item = &'a str>) -> Option<String> {
    let mut keys: Vec<&str> = keys.filter(|k| is_issue_key(k)).collect();
    keys.sort_unstable();
    keys.dedup();
    (!keys.is_empty()).then(|| format!("({})", keys.join(",")))
}

/// `PROJ-123`: a project key (a letter, then letters, digits or `_`) and a number.
fn is_issue_key(key: &str) -> bool {
    let Some((project, number)) = key.split_once('-') else {
        return false;
    };
    project.starts_with(|c: char| c.is_ascii_alphabetic())
        && project
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit())
}

/// The bundled tickets plus a parent with many children, built here so the
/// fixture stays readable.
fn sample_body() -> Value {
    let mut body: Value = serde_json::from_str(SAMPLE).expect("valid sample");
    let parent = body["issues"]
        .as_array()
        .and_then(|a| a.iter().find(|i| i["key"] == SAMPLE_BIG_PARENT))
        .map(|i| i["fields"]["summary"].clone())
        .unwrap_or(Value::Null);
    let statuses = [
        ("Por hacer", "new"),
        ("En curso", "indeterminate"),
        ("Hecho", "done"),
    ];
    let people = [Some("Fran"), Some("Ana"), None];
    let children: Vec<Value> = (0..SAMPLE_BIG_CHILDREN)
        .map(|n| {
            let (status, category) = statuses[n % statuses.len()];
            let assignee = people[n % people.len()].map_or(
                Value::Null,
                |name| serde_json::json!({ "displayName": name }),
            );
            serde_json::json!({
                "key": format!("PROJ-{}", 1260 + n),
                "fields": {
                    "summary": format!("Paso {} de la migración", n + 1),
                    "issuetype": { "name": "Subtarea", "subtask": true, "hierarchyLevel": -1 },
                    "status": { "name": status, "statusCategory": { "key": category } },
                    "assignee": assignee,
                    "reporter": { "displayName": "Fran" },
                    "parent": { "key": SAMPLE_BIG_PARENT, "fields": { "summary": parent } },
                    "components": [],
                    "description": "",
                }
            })
        })
        .collect();
    if let Some(issues) = body["issues"].as_array_mut() {
        issues.extend(children);
    }
    body
}

/// Fills `subtask_count` / `subtasks_done` of the sample parents from the list.
fn count_sample_subtasks(issues: &mut [JiraIssue]) {
    let mut counts: HashMap<String, (u32, u32)> = HashMap::new();
    for child in issues.iter().filter(|i| i.subtask) {
        if let Some(parent) = &child.parent {
            let entry = counts.entry(parent.key.clone()).or_default();
            entry.0 += 1;
            entry.1 += u32::from(child.status_category == "done");
        }
    }
    for issue in issues.iter_mut() {
        if let Some((count, done)) = counts.get(&issue.key) {
            issue.subtask_count = *count;
            issue.subtasks_done = *done;
        }
    }
}

fn parent_listed(issue: &JiraIssue, issues: &[JiraIssue]) -> bool {
    issue
        .parent
        .as_ref()
        .is_some_and(|p| issues.iter().any(|i| i.key == p.key))
}

/// Gives each sub-task with no epic its parent's, finding the parent in the list
/// or in `others`; fills in the parent's title on the way if it was missing.
fn inherit_epics(issues: &mut [JiraIssue], others: &[JiraIssue]) {
    let parents: HashMap<String, (Option<JiraEpic>, String)> = issues
        .iter()
        .chain(others)
        .map(|i| (i.key.clone(), (i.epic.clone(), i.summary.clone())))
        .collect();
    for issue in issues.iter_mut().filter(|i| i.epic.is_none()) {
        let Some(parent) = issue.parent.as_mut() else {
            continue;
        };
        if let Some((epic, summary)) = parents.get(&parent.key) {
            issue.epic = epic.clone();
            parent.summary.get_or_insert_with(|| summary.clone());
        }
    }
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

    #[tokio::test]
    async fn subtasks_of_a_parent_include_every_assignee_and_status() {
        let jira = JiraClient::sample();
        let kids = jira.subtasks("PROJ-1242").await.unwrap();
        let keys: Vec<&str> = kids.iter().map(|i| i.key.as_str()).collect();
        assert_eq!(keys, ["PROJ-1250", "PROJ-1251"]);
        assert!(kids.iter().all(|i| i.subtask));
        // Mine and someone else's, in progress and to do.
        assert_eq!(kids[0].assignee.as_deref(), Some("Fran"));
        assert_eq!(kids[1].assignee.as_deref(), Some("Ana"));
        assert_ne!(kids[0].status, kids[1].status);

        assert!(jira.subtasks("PROJ-1238").await.unwrap().is_empty());
        assert!(matches!(
            jira.subtasks("NOPE-1").await,
            Err(JiraError::NotInSample(_))
        ));
        assert!(matches!(
            jira.subtasks("PROJ-1 OR x").await,
            Err(JiraError::BadKey(_))
        ));
    }

    #[tokio::test]
    async fn my_subtask_keys_are_only_those_assigned_to_me() {
        let jira = JiraClient::sample();
        let mine = jira.my_subtask_keys("PROJ-1246").await.unwrap();
        assert_eq!(mine, HashSet::from(["PROJ-1256".to_string()]));

        // The parent also has an unassigned, someone else's and a done sub-task.
        let kids = jira.subtasks("PROJ-1246").await.unwrap();
        let by = |k: &str| kids.iter().find(|i| i.key == k).unwrap();
        assert_eq!(by("PROJ-1255").assignee, None);
        assert_eq!(by("PROJ-1256").assignee.as_deref(), Some(SAMPLE_ME));
        assert_eq!(by("PROJ-1257").assignee.as_deref(), Some("Ana"));
        assert_eq!(by("PROJ-1252").status_category, "done");
        for open in ["PROJ-1255", "PROJ-1256", "PROJ-1257"] {
            assert_ne!(by(open).status_category, "done");
        }

        assert!(jira.my_subtask_keys("PROJ-1238").await.unwrap().is_empty());
        assert!(matches!(
            jira.my_subtask_keys("X; DROP").await,
            Err(JiraError::BadKey(_))
        ));
    }

    #[tokio::test]
    async fn parents_carry_subtask_and_done_counts() {
        let jira = JiraClient::sample();
        let parent = jira.issue("PROJ-1242").await.unwrap();
        assert_eq!((parent.subtask_count, parent.subtasks_done), (2, 0));
        let story = jira.issue("PROJ-1246").await.unwrap();
        assert_eq!((story.subtask_count, story.subtasks_done), (4, 1));
        let bare = jira.issue("PROJ-1238").await.unwrap();
        assert_eq!((bare.subtask_count, bare.subtasks_done), (0, 0));

        // Jira's own `subtasks` field gives the same counts.
        let raw = json!({ "key": "P-1", "fields": { "summary": "S",
            "issuetype": { "name": "Story" }, "status": { "name": "x" }, "components": [],
            "subtasks": [
                { "key": "P-2", "fields": { "status": { "statusCategory": { "key": "done" } } } },
                { "key": "P-3", "fields": { "status": { "statusCategory": { "key": "new" } } } },
                { "key": "P-4", "fields": { "status": { "statusCategory": { "key": "done" } } } }
            ] }});
        let issue = map_issue(&raw, None, &none()).unwrap();
        assert_eq!((issue.subtask_count, issue.subtasks_done), (3, 2));
    }

    #[tokio::test]
    async fn the_sample_has_a_parent_with_twenty_children() {
        let jira = JiraClient::sample();
        let big = jira.issue("PROJ-1244").await.unwrap();
        assert_eq!(big.subtask_count, 20);
        assert!(big.subtasks_done > 0 && big.subtasks_done < 20);
        let kids = jira.subtasks("PROJ-1244").await.unwrap();
        assert_eq!(kids.len(), 20);
        assert_eq!(
            kids.iter().filter(|k| k.status_category == "done").count() as u32,
            big.subtasks_done
        );
    }

    #[test]
    fn maps_cloud_and_data_center_issues() {
        let cloud = json!({ "key": "PROJ-1", "fields": {
            "summary": "S", "issuetype": { "name": "Bug" }, "priority": { "name": "Alta" },
            "status": { "name": "Por hacer" }, "assignee": null, "components": [],
            "description": { "type": "doc", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "ADF" }] }] }
        }});
        let issue = map_issue(&cloud, Some("https://acme.atlassian.net"), &none()).unwrap();
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
        let issue = map_issue(&dc, None, &none()).unwrap();
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
        let issue = map_issue(&cloud, None, &none()).unwrap();
        assert_eq!(issue.status_category, "indeterminate");
        assert_eq!(issue.reporter.as_deref(), Some("Ana"));
        assert_eq!(
            issue.epic,
            Some(JiraEpic {
                key: "PROJ-9".into(),
                summary: Some("Pagos".into())
            })
        );

        assert_eq!(issue.parent, None, "an epic parent is not the parent");
        assert!(!issue.subtask);

        // A sub-task's parent is a story, not an epic.
        let subtask = json!({ "key": "PROJ-2", "fields": {
            "issuetype": { "name": "Subtarea", "subtask": true, "hierarchyLevel": -1 },
            "parent": { "key": "PROJ-1", "fields": { "summary": "Historia",
                "issuetype": { "name": "Story", "hierarchyLevel": 0 } } }
        }});
        let subtask = map_issue(&subtask, None, &none()).unwrap();
        assert_eq!(subtask.epic, None);
        assert!(subtask.subtask);
        assert_eq!(
            subtask.parent,
            Some(JiraParent {
                key: "PROJ-1".into(),
                summary: Some("Historia".into())
            })
        );

        let dc = json!({ "key": "OPS-2", "fields": { "customfield_10100": "OPS-1" } });
        let custom = CustomFields {
            epic_link: Some("customfield_10100".into()),
            sprint: None,
        };
        let issue = map_issue(&dc, None, &custom).unwrap();
        assert_eq!(issue.epic.unwrap().key, "OPS-1");
    }

    #[test]
    fn builds_the_jql_for_each_filter() {
        let f = |who, progress| Filter {
            who,
            progress,
            sprint: SprintFilter::All,
        };
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

        let current = |who| Filter {
            who,
            progress: Progress::Open,
            sprint: SprintFilter::Current,
        };
        assert_eq!(
            current(Who::Assigned).jql(saved),
            "assignee = currentUser() AND statusCategory != Done AND sprint in openSprints() AND sprint not in futureSprints() ORDER BY updated DESC"
        );
        assert_eq!(
            current(Who::Query).jql(saved),
            "(project = OPS and labels = api) AND statusCategory != Done AND sprint in openSprints() AND sprint not in futureSprints() order by priority desc"
        );
        assert_eq!(
            current(Who::Query).jql(""),
            "statusCategory != Done AND sprint in openSprints() AND sprint not in futureSprints() ORDER BY updated DESC"
        );
    }

    #[tokio::test]
    async fn the_sample_answers_each_filter() {
        let jira = JiraClient::sample();
        let keys = |issues: Vec<JiraIssue>| issues.into_iter().map(|i| i.key).collect::<Vec<_>>();
        let f = |who, progress| Filter {
            who,
            progress,
            sprint: SprintFilter::All,
        };
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

        let sprint = keys(
            jira.list(
                &Filter {
                    sprint: SprintFilter::Current,
                    ..f(Who::Assigned, Progress::Open)
                },
                "",
            )
            .await
            .unwrap(),
        );
        assert!(sprint.contains(&"PROJ-1242".to_string()));
        assert!(sprint.contains(&"PROJ-1251".to_string()), "its sub-task");
        assert!(!sprint.contains(&"PROJ-1244".to_string()), "next sprint");
        let everyone = |sprint| Filter {
            sprint,
            ..f(Who::Query, Progress::Open)
        };
        let all = keys(jira.list(&everyone(SprintFilter::All), "").await.unwrap());
        let current = keys(
            jira.list(&everyone(SprintFilter::Current), "")
                .await
                .unwrap(),
        );
        assert!(all.contains(&"PROJ-1245".to_string()));
        assert!(!current.contains(&"PROJ-1245".to_string()), "backlog");
    }

    #[tokio::test]
    async fn the_sample_nests_subtasks_under_their_parent_epic() {
        let jira = JiraClient::sample();
        let issues = jira.list(&Filter::default(), "").await.unwrap();
        let find = |key: &str| issues.iter().find(|i| i.key == key);

        // Ana's sub-task of one of mine comes along, in my ticket's epic.
        let ana = find("PROJ-1251").expect("sub-task assigned to someone else");
        assert_eq!(ana.assignee.as_deref(), Some("Ana"));
        assert_eq!(ana.parent.as_ref().unwrap().key, "PROJ-1242");
        assert_eq!(ana.epic.as_ref().unwrap().key, "PROJ-1200");

        // Mine, under a ticket that is not listed: its epic comes from the parent.
        let orphan = find("PROJ-1253").expect("my sub-task of someone else's story");
        assert!(find("PROJ-1190").is_none());
        assert_eq!(orphan.epic.as_ref().unwrap().key, "PROJ-1210");

        // Someone else's, under someone else's ticket: not mine at all.
        assert!(find("PROJ-1254").is_none());
        let first_subtask = issues.iter().position(|i| i.subtask).unwrap();
        assert!(
            issues[first_subtask..].iter().all(|i| i.subtask),
            "parents first"
        );
    }

    #[test]
    fn reads_the_sprint_in_each_format() {
        let custom = CustomFields {
            epic_link: None,
            sprint: Some("customfield_10020".into()),
        };
        let with = |sprints: Value| {
            let issue = json!({ "key": "P-1", "fields": { "customfield_10020": sprints } });
            map_issue(&issue, None, &custom).unwrap().sprint
        };
        let cloud = |name: &str, state: &str| json!({ "id": 1, "name": name, "state": state });
        let sprint = |name: &str, state: &str| {
            Some(JiraSprint {
                name: name.into(),
                state: state.into(),
            })
        };
        assert_eq!(
            with(json!([cloud("S41", "closed"), cloud("S42", "active")])),
            sprint("S42", "active")
        );
        assert_eq!(
            with(json!([cloud("S41", "closed"), cloud("S43", "future")])),
            sprint("S43", "future")
        );
        assert_eq!(
            with(json!([cloud("S40", "closed"), cloud("S41", "closed")])),
            sprint("S41", "closed")
        );
        assert_eq!(
            with(json!([
                "com.atlassian.greenhopper.service.sprint.Sprint@1a2b[id=7,rapidViewId=3,state=ACTIVE,name=Sprint 42, pagos,startDate=2026-10-13T09:00:00.000+02:00,endDate=<null>]"
            ])),
            sprint("Sprint 42, pagos", "active")
        );
        assert_eq!(with(Value::Null), None);
        assert_eq!(
            with(json!(["Sprint@1[id=7,state=ACTIVE]", "no brackets"])),
            None
        );
        assert_eq!(
            with(json!(["Sprint@1[id=7,name=Sin estado]"])),
            sprint("Sin estado", "")
        );
        let none = json!({ "key": "P-1", "fields": {} });
        assert_eq!(map_issue(&none, None, &custom).unwrap().sprint, None);
    }

    fn none() -> CustomFields {
        CustomFields::default()
    }

    #[test]
    fn only_issue_keys_reach_the_jql() {
        let keys = [
            "PROJ-2",
            "PROJ-1",
            "PROJ-2",
            "X) OR (project = SECRET",
            "a_b9-10",
            "-1",
            "P-",
        ];
        assert_eq!(
            keys_jql(keys.into_iter()).as_deref(),
            Some("(PROJ-1,PROJ-2,a_b9-10)")
        );
        assert_eq!(keys_jql(["1A-2"].into_iter()), None);
    }

    #[test]
    fn only_a_subtask_has_a_parent() {
        // A Cloud epic under an initiative: the initiative is not its "parent" row.
        let epic = json!({ "key": "PROJ-9", "fields": {
            "issuetype": { "name": "Epic", "hierarchyLevel": 1 },
            "parent": { "key": "INIT-1", "fields": { "issuetype": { "name": "Initiative", "hierarchyLevel": 2 } } }
        }});
        let epic = map_issue(&epic, None, &none()).unwrap();
        assert_eq!(epic.parent, None);
        assert_eq!(epic.epic, None);
    }

    type Route = (&'static str, u16, Value);

    /// A fake Jira: each request gets the first route whose every `&`-separated
    /// needle is in its decoded path and query (`!needle` = must be absent).
    /// Returns the site and the requests it got.
    async fn fake_jira(
        routes: Vec<Route>,
    ) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let site = format!("http://{}", listener.local_addr().unwrap());
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = seen.clone();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut request = Vec::new();
                let mut buf = [0; 4096];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = socket.read(&mut buf).await.unwrap();
                    if n == 0 {
                        break;
                    }
                    request.extend_from_slice(&buf[..n]);
                }
                let line = String::from_utf8_lossy(&request);
                let target = decode(line.split(' ').nth(1).unwrap_or_default());
                log.lock().unwrap().push(target.clone());
                let (status, body) = routes
                    .iter()
                    .find(|(needles, ..)| {
                        needles.split('&').all(|n| match n.strip_prefix('!') {
                            Some(absent) => !target.contains(absent),
                            None => target.contains(n),
                        })
                    })
                    .map(|(_, status, body)| (*status, body.to_string()))
                    .unwrap_or((404, r#"{"errorMessages":["no route"]}"#.into()));
                let response = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        (site, seen)
    }

    /// Percent-decoding, enough for the JQL in a query string.
    fn decode(text: &str) -> String {
        let bytes = text.replace('+', " ").into_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap();
                out.push(u8::from_str_radix(hex, 16).unwrap());
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        String::from_utf8(out).unwrap()
    }

    fn story(key: &str, extra: Value) -> Value {
        let mut fields = json!({
            "summary": format!("Ticket {key}"),
            "issuetype": { "name": "Story", "hierarchyLevel": 0 },
            "status": { "name": "To Do", "statusCategory": { "key": "new" } },
        });
        fields
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        json!({ "key": key, "fields": fields })
    }

    fn subtask_of(key: &str, parent: &str) -> Value {
        story(
            key,
            json!({
                "issuetype": { "name": "Sub-task", "subtask": true, "hierarchyLevel": -1 },
                "parent": { "key": parent, "fields": { "summary": format!("Ticket {parent}"),
                    "issuetype": { "name": "Story", "hierarchyLevel": 0 } } }
            }),
        )
    }

    const EPIC: &str = r#"{ "key": "PROJ-9", "fields": { "summary": "Pagos", "issuetype": { "name": "Epic", "hierarchyLevel": 1 } } }"#;

    #[tokio::test]
    async fn cloud_lists_the_subtasks_of_its_tickets_with_their_epic_and_sprint() {
        let epic: Value = serde_json::from_str(EPIC).unwrap();
        let (site, seen) = fake_jira(vec![
            (
                "/field",
                200,
                json!([
                    { "id": "customfield_10020", "schema": { "custom": SPRINT_SCHEMA } }
                ]),
            ),
            // This site has no `subTaskIssueTypes()`: the plain `parent in` takes over.
            (
                "subTaskIssueTypes",
                400,
                json!({ "errorMessages": ["unknown function"] }),
            ),
            (
                "parent in (PROJ-1)",
                200,
                json!({ "issues": [
                subtask_of("PROJ-2", "PROJ-1"),
                story("PROJ-3", json!({ "parent": epic })),
            ], "isLast": true }),
            ),
            (
                "assignee = currentUser()&sprint not in futureSprints()",
                200,
                json!({ "issues": [
                story("PROJ-1", json!({
                    "parent": epic,
                    "customfield_10020": [{ "name": "Sprint 42", "state": "active" }]
                })),
            ], "isLast": true }),
            ),
        ])
        .await;
        let jira = JiraClient::cloud(&site, "fran@example.com", "token");
        let filter = Filter {
            sprint: SprintFilter::Current,
            ..Filter::default()
        };
        let issues = jira.list(&filter, "").await.unwrap();

        let keys: Vec<&str> = issues.iter().map(|i| i.key.as_str()).collect();
        assert_eq!(
            keys,
            ["PROJ-1", "PROJ-2"],
            "an epic's story is not a sub-task"
        );
        assert_eq!(issues[0].sprint.as_ref().unwrap().name, "Sprint 42");
        assert_eq!(issues[1].parent.as_ref().unwrap().key, "PROJ-1");
        assert_eq!(issues[1].epic.as_ref().unwrap().key, "PROJ-9");
        let fields = seen
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.contains("/field"))
            .count();
        assert_eq!(fields, 1, "the fields are looked up once per listing");
    }

    #[tokio::test]
    async fn the_tickets_survive_when_jira_refuses_the_extras() {
        let (site, _) = fake_jira(vec![
            ("/field", 200, json!([])),
            (
                "parent in",
                400,
                json!({ "errorMessages": ["Field 'parent' does not exist"] }),
            ),
            (
                "key in",
                400,
                json!({ "errorMessages": ["An issue with key 'PROJ-4' does not exist"] }),
            ),
            (
                "currentUser()",
                200,
                json!({ "issues": [
                story("PROJ-1", json!({})),
                subtask_of("PROJ-5", "PROJ-4"),
            ], "isLast": true }),
            ),
        ])
        .await;
        let jira = JiraClient::cloud(&site, "fran@example.com", "token");
        let issues = jira.list(&Filter::default(), "").await.unwrap();
        let keys: Vec<&str> = issues.iter().map(|i| i.key.as_str()).collect();
        assert_eq!(keys, ["PROJ-1", "PROJ-5"]);
        assert_eq!(issues[1].epic, None);
    }

    #[tokio::test]
    async fn data_center_pages_through_and_names_linked_epics() {
        let epic_link = |key: &str| story(key, json!({ "customfield_10100": "OPS-1" }));
        let (site, seen) = fake_jira(vec![
            (
                "/field",
                200,
                json!([
                    { "id": "customfield_10100", "schema": { "custom": EPIC_LINK_SCHEMA } }
                ]),
            ),
            (
                "parent in",
                200,
                json!({ "startAt": 0, "total": 0, "issues": [] }),
            ),
            (
                "key in (OPS-1)",
                200,
                json!({ "startAt": 0, "total": 1, "issues": [
                { "key": "OPS-1", "fields": { "summary": "Plataforma" } }
            ] }),
            ),
            (
                "currentUser()&startAt=0",
                200,
                json!({ "startAt": 0, "total": 3, "issues": [
                epic_link("OPS-2"), epic_link("OPS-3"),
            ] }),
            ),
            (
                "currentUser()&startAt=2",
                200,
                json!({ "startAt": 2, "total": 3, "issues": [
                epic_link("OPS-4"),
            ] }),
            ),
        ])
        .await;
        let jira = JiraClient::data_center(&site, "token");
        let issues = jira.list(&Filter::default(), "").await.unwrap();
        let keys: Vec<&str> = issues.iter().map(|i| i.key.as_str()).collect();
        assert_eq!(keys, ["OPS-2", "OPS-3", "OPS-4"]);
        assert!(
            issues
                .iter()
                .all(|i| i.epic.as_ref().unwrap().summary.as_deref() == Some("Plataforma"))
        );
        let pages = seen
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.contains("currentUser()"))
            .count();
        assert_eq!(pages, 2);
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
