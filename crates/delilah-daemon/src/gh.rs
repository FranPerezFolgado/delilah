//! Talking to GitHub through the user's own `gh`: it holds the tokens (Delilah stores
//! no credentials) and resolves the repo from the remotes as the agent's `gh pr create`
//! does. Everything goes behind the [`Gh`] trait so tests answer with canned output.

// The loop and the API that use these land in the next packages.
#![allow(dead_code)]

use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures_util::future::BoxFuture;
use serde::Deserialize;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::prs::{
    Check, Comment, CommentKind, ListedPr, Outcome, PrState, Review, ReviewDecision, ReviewState,
    Snapshot,
};

const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, thiserror::Error)]
pub enum GhError {
    #[error("gh is not installed")]
    NotFound,
    #[error("gh took too long to answer")]
    Timeout,
    #[error("{stderr}")]
    Failed { code: Option<i32>, stderr: String },
    #[error("unexpected answer from gh: {0}")]
    Parse(String),
}

impl GhError {
    fn failed(stderr: impl Into<String>) -> Self {
        Self::Failed {
            code: None,
            stderr: stderr.into(),
        }
    }
}

/// Why GitHub could not be reached, for the PR tab and the one-off notice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessProblem {
    Missing,
    SignedOut,
    NoAccess,
    UnsupportedRemote,
    Network,
    RateLimited { reset: Option<DateTime<Utc>> },
}

impl AccessProblem {
    /// The name the API shows (`problem.kind`).
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::SignedOut => "signed-out",
            Self::NoAccess => "no-access",
            Self::UnsupportedRemote => "unsupported-remote",
            Self::Network => "network",
            Self::RateLimited { .. } => "rate-limited",
        }
    }
}

/// Reads what went wrong from `gh`'s exit and stderr. Anything unknown counts as the
/// network: it is retried with back-off and never blames the project.
pub fn classify(err: &GhError) -> AccessProblem {
    let text = match err {
        GhError::NotFound => return AccessProblem::Missing,
        GhError::Timeout | GhError::Parse(_) => return AccessProblem::Network,
        GhError::Failed { stderr, .. } => stderr.to_lowercase(),
    };
    // A rate limit is a 403 too: look for it first.
    if text.contains("rate limit") {
        return AccessProblem::RateLimited {
            reset: rate_limit_reset(&text),
        };
    }
    if text.contains("gh auth login") || text.contains("not logged") {
        AccessProblem::SignedOut
    } else if text.contains("none of the git remotes") {
        AccessProblem::UnsupportedRemote
    } else if text.contains("could not resolve to a repository")
        || text.contains("http 404")
        || text.contains("http 403")
    {
        AccessProblem::NoAccess
    } else {
        AccessProblem::Network
    }
}

/// `x-ratelimit-reset: <epoch>` when `gh` printed the headers.
fn rate_limit_reset(text: &str) -> Option<DateTime<Utc>> {
    let rest = &text[text.find("x-ratelimit-reset:")? + "x-ratelimit-reset:".len()..];
    let secs: i64 = rest
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()?;
    DateTime::from_timestamp(secs, 0)
}

pub trait Gh: Send + Sync {
    /// Runs `gh <args>` (in `dir` if given, feeding `stdin`) and returns its stdout.
    fn run(
        &self,
        dir: Option<&Path>,
        args: Vec<String>,
        stdin: Option<String>,
    ) -> BoxFuture<'_, Result<String, GhError>>;

    /// Logins already asked for, by host.
    fn users(&self) -> &Mutex<HashMap<String, String>>;
}

/// The real `gh`: the binary is `DELILAH_GH_BIN` or `gh`, found on the login shell's
/// PATH with a clean environment, as agents get.
#[derive(Default)]
pub struct CliGh {
    users: Mutex<HashMap<String, String>>,
}

impl Gh for CliGh {
    fn run(
        &self,
        dir: Option<&Path>,
        args: Vec<String>,
        stdin: Option<String>,
    ) -> BoxFuture<'_, Result<String, GhError>> {
        let dir = dir.map(Path::to_path_buf);
        Box::pin(async move {
            let bin = std::env::var_os("DELILAH_GH_BIN").unwrap_or_else(|| "gh".into());
            let mut cmd = Command::new(bin);
            cmd.args(&args).env_clear();
            for (k, v) in delilah_agents::env::agent_env() {
                cmd.env(k, v);
            }
            cmd.env("GH_PROMPT_DISABLED", "1").env("NO_COLOR", "1");
            // Previews answer from a folder of JSON (scripts/fake-gh.sh).
            if let Some(fake) = std::env::var_os("FAKE_GH_DIR") {
                cmd.env("FAKE_GH_DIR", fake);
            }
            if let Some(dir) = &dir {
                cmd.current_dir(dir);
            }
            cmd.stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
            let mut child = cmd.spawn().map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => GhError::NotFound,
                _ => GhError::failed(e.to_string()),
            })?;
            let work = async {
                if let (Some(text), Some(mut pipe)) = (stdin, child.stdin.take()) {
                    // A `gh` that exits early closes the pipe: its exit says why.
                    let _ = pipe.write_all(text.as_bytes()).await;
                }
                child.wait_with_output().await
            };
            let output = match tokio::time::timeout(TIMEOUT, work).await {
                Err(_) => return Err(GhError::Timeout),
                Ok(Err(e)) => return Err(GhError::failed(e.to_string())),
                Ok(Ok(output)) => output,
            };
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).into_owned())
            } else {
                Err(GhError::Failed {
                    code: output.status.code(),
                    stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
                })
            }
        })
    }

    fn users(&self) -> &Mutex<HashMap<String, String>> {
        &self.users
    }
}

/// Where a repo lives and its default branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoInfo {
    pub host: String,
    pub owner_name: String,
    pub default_branch: String,
}

/// A PR located by its URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrRef {
    pub host: String,
    pub owner_name: String,
    pub number: i64,
}

/// `https://<host>/<owner>/<name>/pull/<n>` (a trailing path or query is ignored).
pub fn parse_pr_url(url: &str) -> Option<PrRef> {
    let rest = url.trim().strip_prefix("https://")?;
    let mut parts = rest.split(['?', '#']).next()?.split('/');
    let host = parts.next().filter(|h| !h.is_empty())?;
    let (owner, name) = (parts.next()?, parts.next()?);
    if parts.next()? != "pull" || owner.is_empty() || name.is_empty() {
        return None;
    }
    let number = parts.next()?.parse().ok()?;
    Some(PrRef {
        host: host.to_string(),
        owner_name: format!("{owner}/{name}"),
        number,
    })
}

fn host_of(url: &str) -> Option<&str> {
    url.split("://")
        .nth(1)?
        .split('/')
        .next()
        .filter(|h| !h.is_empty())
}

/// `gh repo view` in the repo's folder; honours `gh repo set-default` and the remotes.
pub async fn repo_view(gh: &dyn Gh, dir: &Path) -> Result<RepoInfo, GhError> {
    let args = [
        "repo",
        "view",
        "--json",
        "nameWithOwner,url,defaultBranchRef",
    ];
    let out = gh.run(Some(dir), strings(&args), None).await?;
    parse_repo_view(&out)
}

fn parse_repo_view(out: &str) -> Result<RepoInfo, GhError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Raw {
        name_with_owner: String,
        url: String,
        default_branch_ref: Option<Branch>,
    }
    #[derive(Deserialize)]
    struct Branch {
        name: String,
    }
    let raw: Raw = serde_json::from_str(out).map_err(|e| GhError::Parse(e.to_string()))?;
    let host =
        host_of(&raw.url).ok_or_else(|| GhError::Parse(format!("no host in {}", raw.url)))?;
    Ok(RepoInfo {
        host: host.to_string(),
        owner_name: raw.name_with_owner,
        default_branch: raw
            .default_branch_ref
            .map_or_else(|| "main".into(), |b| b.name),
    })
}

/// The login `gh` is signed in as on `host`, asked once per host.
pub async fn user(gh: &dyn Gh, host: &str) -> Result<String, GhError> {
    if let Some(login) = gh.users().lock().unwrap().get(host) {
        return Ok(login.clone());
    }
    let args = ["api", "user", "--hostname", host, "--jq", ".login"];
    let login = gh.run(None, strings(&args), None).await?.trim().to_string();
    if login.is_empty() {
        return Err(GhError::Parse("empty login".into()));
    }
    gh.users()
        .lock()
        .unwrap()
        .insert(host.to_string(), login.clone());
    Ok(login)
}

/// The PRs whose head is `branch`, in any state.
pub async fn pr_list(gh: &dyn Gh, dir: &Path, branch: &str) -> Result<Vec<ListedPr>, GhError> {
    let args = [
        "pr",
        "list",
        "--head",
        branch,
        "--state",
        "all",
        "--json",
        "number,url,state,updatedAt",
    ];
    let out = gh.run(Some(dir), strings(&args), None).await?;
    serde_json::from_str(&out).map_err(|e| GhError::Parse(e.to_string()))
}

/// Everything one check needs about a PR, in a single call (research R2).
const SNAPSHOT_QUERY: &str = "query($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      state isDraft merged headRefOid baseRefName title url reviewDecision
      commits(last: 1) { nodes { commit { statusCheckRollup { contexts(first: 100) { nodes {
        __typename
        ... on CheckRun { name status conclusion detailsUrl }
        ... on StatusContext { context state targetUrl }
      } } } } } }
      reviews(last: 50) { nodes { databaseId state author { login } body submittedAt } }
      comments(last: 100) { nodes { databaseId author { login } body createdAt } }
      reviewThreads(last: 100) { nodes { comments(first: 50) { nodes {
        databaseId author { login } path line body createdAt
      } } } }
    }
  }
}";

pub async fn pr_snapshot(
    gh: &dyn Gh,
    host: &str,
    owner_name: &str,
    number: i64,
) -> Result<Snapshot, GhError> {
    let (owner, name) = owner_name
        .split_once('/')
        .ok_or_else(|| GhError::Parse(format!("bad repo `{owner_name}`")))?;
    let args = vec![
        "api".to_string(),
        "graphql".into(),
        "--hostname".into(),
        host.into(),
        "-f".into(),
        format!("query={SNAPSHOT_QUERY}"),
        // Strings with -f so a numeric owner is not turned into a number.
        "-f".into(),
        format!("owner={owner}"),
        "-f".into(),
        format!("name={name}"),
        "-F".into(),
        format!("number={number}"),
    ];
    let out = gh.run(None, args, None).await?;
    parse_snapshot(&out)
}

/// Reads a PR by its URL, without a repo folder (the worktree may be gone).
pub async fn pr_view_url(gh: &dyn Gh, url: &str) -> Result<(PrRef, Snapshot), GhError> {
    let pr = parse_pr_url(url).ok_or_else(|| GhError::Parse(format!("not a PR URL: {url}")))?;
    let snapshot = pr_snapshot(gh, &pr.host, &pr.owner_name, pr.number).await?;
    Ok((pr, snapshot))
}

/// Creates the PR and returns its URL. The body goes through stdin.
pub async fn pr_create(
    gh: &dyn Gh,
    dir: &Path,
    head: &str,
    base: &str,
    title: &str,
    body: &str,
    draft: bool,
) -> Result<String, GhError> {
    let mut args = strings(&[
        "pr",
        "create",
        "--head",
        head,
        "--base",
        base,
        "--title",
        title,
        "--body-file",
        "-",
    ]);
    if draft {
        args.push("--draft".into());
    }
    let out = gh.run(Some(dir), args, Some(body.to_string())).await?;
    out.lines()
        .rev()
        .map(str::trim)
        .find(|l| l.starts_with("http"))
        .map(str::to_string)
        .ok_or_else(|| GhError::Parse(format!("no URL in `{}`", out.trim())))
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| a.to_string()).collect()
}

#[derive(Deserialize)]
struct Nodes<T> {
    #[serde(default = "Vec::new")]
    nodes: Vec<Option<T>>,
}

impl<T> Nodes<T> {
    fn into_items(self) -> impl Iterator<Item = T> {
        self.nodes.into_iter().flatten()
    }
}

#[derive(Deserialize)]
struct Author {
    login: String,
}

fn login(author: Option<Author>) -> String {
    // A deleted account comes back as null.
    author.map_or_else(|| "ghost".into(), |a| a.login)
}

#[derive(Deserialize)]
struct Envelope {
    data: Option<Data>,
    #[serde(default)]
    errors: Vec<GqlError>,
}

#[derive(Deserialize)]
struct GqlError {
    message: String,
}

#[derive(Deserialize)]
struct Data {
    repository: Option<Repository>,
}

#[derive(Deserialize)]
struct Repository {
    #[serde(rename = "pullRequest")]
    pull_request: Option<RawPr>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPr {
    state: PrState,
    is_draft: bool,
    head_ref_oid: String,
    base_ref_name: String,
    title: String,
    review_decision: Option<ReviewDecision>,
    commits: Nodes<RawCommitNode>,
    reviews: Nodes<RawReview>,
    comments: Nodes<RawComment>,
    review_threads: Nodes<RawThread>,
}

#[derive(Deserialize)]
struct RawCommitNode {
    commit: RawCommit,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCommit {
    status_check_rollup: Option<RawRollup>,
}

#[derive(Deserialize)]
struct RawRollup {
    contexts: Nodes<RawContext>,
}

/// A check run or a commit status, told apart by which fields it has.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawContext {
    name: Option<String>,
    status: Option<String>,
    conclusion: Option<String>,
    details_url: Option<String>,
    context: Option<String>,
    state: Option<String>,
    target_url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawReview {
    database_id: i64,
    state: ReviewState,
    author: Option<Author>,
    body: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawComment {
    database_id: i64,
    author: Option<Author>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    line: Option<i64>,
    body: String,
}

#[derive(Deserialize)]
struct RawThread {
    comments: Nodes<RawComment>,
}

fn check_of(c: RawContext) -> Option<Check> {
    if let Some(name) = c.name {
        let outcome = match (c.status.as_deref(), c.conclusion.as_deref()) {
            (Some("COMPLETED"), Some(conclusion)) => match conclusion {
                "SUCCESS" => Outcome::Success,
                "FAILURE" | "TIMED_OUT" | "ACTION_REQUIRED" | "STARTUP_FAILURE" => Outcome::Failure,
                // CANCELLED, SKIPPED, NEUTRAL, STALE
                _ => Outcome::Neutral,
            },
            _ => Outcome::Pending,
        };
        return Some(Check {
            name,
            outcome,
            url: c.details_url,
        });
    }
    let name = c.context?;
    let outcome = match c.state.as_deref() {
        Some("SUCCESS") => Outcome::Success,
        Some("FAILURE" | "ERROR") => Outcome::Failure,
        _ => Outcome::Pending,
    };
    Some(Check {
        name,
        outcome,
        url: c.target_url,
    })
}

/// Reads the GraphQL answer of [`SNAPSHOT_QUERY`].
pub(crate) fn parse_snapshot(out: &str) -> Result<Snapshot, GhError> {
    let env: Envelope = serde_json::from_str(out).map_err(|e| GhError::Parse(e.to_string()))?;
    let Some(pr) = env
        .data
        .and_then(|d| d.repository)
        .and_then(|r| r.pull_request)
    else {
        // GitHub answers 200 with the problem in `errors`: keep its words so
        // `classify` can read them.
        let message = env.errors.first().map_or_else(
            || "pull request not found".to_string(),
            |e| e.message.clone(),
        );
        return Err(GhError::failed(message));
    };
    let checks = pr
        .commits
        .into_items()
        .next()
        .and_then(|c| c.commit.status_check_rollup)
        .map(|r| r.contexts.into_items().filter_map(check_of).collect())
        .unwrap_or_default();
    let reviews = pr
        .reviews
        .into_items()
        .map(|r| Review {
            id: r.database_id,
            author: login(r.author),
            state: r.state,
            body: r.body,
        })
        .collect();
    let comment = |c: RawComment, kind| Comment {
        id: c.database_id,
        kind,
        author: login(c.author),
        path: c.path,
        line: c.line,
        body: c.body,
    };
    let mut comments: Vec<Comment> = pr
        .comments
        .into_items()
        .map(|c| comment(c, CommentKind::Issue))
        .collect();
    for thread in pr.review_threads.into_items() {
        comments.extend(
            thread
                .comments
                .into_items()
                .map(|c| comment(c, CommentKind::Review)),
        );
    }
    Ok(Snapshot {
        state: pr.state,
        draft: pr.is_draft,
        title: pr.title,
        head: pr.head_ref_oid,
        base: pr.base_ref_name,
        checks,
        review_decision: pr.review_decision,
        reviews,
        comments,
    })
}

#[cfg(test)]
type Reply = (Vec<String>, Result<String, GhError>);

/// A `Gh` answering from canned replies, for tests of any module.
#[cfg(test)]
pub(crate) struct FakeGh {
    replies: Mutex<Vec<Reply>>,
    calls: Mutex<Vec<Vec<String>>>,
    users: Mutex<HashMap<String, String>>,
}

#[cfg(test)]
impl FakeGh {
    pub(crate) fn new() -> Self {
        Self {
            replies: Mutex::default(),
            calls: Mutex::default(),
            users: Mutex::default(),
        }
    }

    /// Answers calls whose arguments start with `prefix` (the first match wins).
    pub(crate) fn on(self, prefix: &[&str], reply: Result<&str, GhError>) -> Self {
        self.replies
            .lock()
            .unwrap()
            .push((strings(prefix), reply.map(str::to_string)));
        self
    }

    /// Every call made, in order.
    pub(crate) fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }
}

#[cfg(test)]
impl Gh for FakeGh {
    fn run(
        &self,
        _dir: Option<&Path>,
        args: Vec<String>,
        _stdin: Option<String>,
    ) -> BoxFuture<'_, Result<String, GhError>> {
        let reply = self
            .replies
            .lock()
            .unwrap()
            .iter()
            .find(|(prefix, _)| args.starts_with(prefix))
            .map(|(_, r)| r.clone());
        let unsupported = format!("fake gh: unsupported: {}", args.join(" "));
        self.calls.lock().unwrap().push(args);
        Box::pin(async move { reply.unwrap_or_else(|| Err(GhError::failed(unsupported))) })
    }

    fn users(&self) -> &Mutex<HashMap<String, String>> {
        &self.users
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed(stderr: &str) -> GhError {
        GhError::failed(stderr)
    }

    #[test]
    fn classify_reads_gh_failures() {
        let cases = [
            (GhError::NotFound, "missing"),
            (
                failed("To get started with GitHub CLI, please run:  gh auth login"),
                "signed-out",
            ),
            (
                failed("You are not logged into any GitHub hosts."),
                "signed-out",
            ),
            (
                failed("GraphQL: Could not resolve to a Repository with the name 'a/b'."),
                "no-access",
            ),
            (failed("gh: Not Found (HTTP 404)"), "no-access"),
            (
                failed("gh: Resource not accessible (HTTP 403)"),
                "no-access",
            ),
            (
                failed(
                    "none of the git remotes configured for this repository point to a known GitHub host.",
                ),
                "unsupported-remote",
            ),
            (
                failed("gh: API rate limit exceeded for user ID 1. (HTTP 403)"),
                "rate-limited",
            ),
            (GhError::Timeout, "network"),
            (
                failed("error connecting to api.github.com: could not connect"),
                "network",
            ),
            (failed("something nobody foresaw"), "network"),
        ];
        for (err, kind) in cases {
            assert_eq!(classify(&err).kind(), kind, "{err:?}");
        }
    }

    #[test]
    fn rate_limit_reset_is_read_when_gh_prints_it() {
        let err = failed("API rate limit exceeded (HTTP 403)\nX-RateLimit-Reset: 1700000000");
        let AccessProblem::RateLimited { reset } = classify(&err) else {
            panic!("not rate limited")
        };
        assert_eq!(reset, DateTime::from_timestamp(1_700_000_000, 0));
        let AccessProblem::RateLimited { reset } = classify(&failed("API rate limit exceeded"))
        else {
            panic!("not rate limited")
        };
        assert_eq!(reset, None);
    }

    #[test]
    fn repo_view_gives_the_host_from_the_url() {
        let out = r#"{"nameWithOwner":"me/repo","url":"https://github.com/me/repo","defaultBranchRef":{"name":"develop"}}"#;
        assert_eq!(
            parse_repo_view(out).unwrap(),
            RepoInfo {
                host: "github.com".into(),
                owner_name: "me/repo".into(),
                default_branch: "develop".into()
            }
        );
        let ghe = r#"{"nameWithOwner":"org/svc","url":"https://git.corp.example/org/svc","defaultBranchRef":{"name":"main"}}"#;
        assert_eq!(parse_repo_view(ghe).unwrap().host, "git.corp.example");
        assert!(matches!(parse_repo_view("nope"), Err(GhError::Parse(_))));
    }

    #[test]
    fn pr_urls_are_taken_apart() {
        assert_eq!(
            parse_pr_url("https://git.corp.example/org/svc/pull/42/files?x=1"),
            Some(PrRef {
                host: "git.corp.example".into(),
                owner_name: "org/svc".into(),
                number: 42
            })
        );
        assert_eq!(parse_pr_url("https://github.com/me/repo/issues/4"), None);
        assert_eq!(parse_pr_url("https://github.com/me/repo/pull/x"), None);
        assert_eq!(parse_pr_url("not a url"), None);
    }

    #[tokio::test]
    async fn pr_list_parses_gh_json() {
        let out = r#"[{"number":3,"url":"https://github.com/me/repo/pull/3","state":"OPEN","updatedAt":"2026-10-20T10:00:00Z"},
                      {"number":2,"url":"https://github.com/me/repo/pull/2","state":"CLOSED","updatedAt":"2026-10-19T10:00:00Z"}]"#;
        let gh = FakeGh::new().on(&["pr", "list"], Ok(out));
        let prs = pr_list(&gh, Path::new("/tmp"), "feature/x").await.unwrap();
        assert_eq!(prs.len(), 2);
        assert_eq!(prs[0].state, PrState::Open);
        assert_eq!(prs[1].number, 2);
        assert_eq!(gh.calls()[0][..4], ["pr", "list", "--head", "feature/x"]);
    }

    const GRAPHQL: &str = r#"{"data":{"repository":{"pullRequest":{
      "state":"OPEN","isDraft":true,"merged":false,"headRefOid":"abc123","baseRefName":"main",
      "title":"Add things","url":"https://github.com/me/repo/pull/42","reviewDecision":"CHANGES_REQUESTED",
      "commits":{"nodes":[{"commit":{"statusCheckRollup":{"contexts":{"nodes":[
        {"__typename":"CheckRun","name":"build","status":"COMPLETED","conclusion":"SUCCESS","detailsUrl":"https://ci/1"},
        {"__typename":"CheckRun","name":"test","status":"COMPLETED","conclusion":"FAILURE","detailsUrl":"https://ci/2"},
        {"__typename":"CheckRun","name":"cancelled","status":"COMPLETED","conclusion":"CANCELLED","detailsUrl":null},
        {"__typename":"CheckRun","name":"skipped","status":"COMPLETED","conclusion":"SKIPPED","detailsUrl":null},
        {"__typename":"CheckRun","name":"neutral","status":"COMPLETED","conclusion":"NEUTRAL","detailsUrl":null},
        {"__typename":"CheckRun","name":"slow","status":"COMPLETED","conclusion":"TIMED_OUT","detailsUrl":null},
        {"__typename":"CheckRun","name":"queued","status":"QUEUED","conclusion":null,"detailsUrl":null},
        {"__typename":"StatusContext","context":"legacy/ci","state":"ERROR","targetUrl":"https://legacy/1"},
        {"__typename":"StatusContext","context":"legacy/lint","state":"SUCCESS","targetUrl":null}
      ]}}}}]},
      "reviews":{"nodes":[
        {"databaseId":11,"state":"CHANGES_REQUESTED","author":{"login":"alice"},"body":"Please fix","submittedAt":"2026-10-20T10:00:00Z"},
        {"databaseId":12,"state":"DISMISSED","author":null,"body":"","submittedAt":"2026-10-20T11:00:00Z"}]},
      "comments":{"nodes":[{"databaseId":21,"author":{"login":"bob"},"body":"Looks odd","createdAt":"2026-10-20T10:30:00Z"}]},
      "reviewThreads":{"nodes":[{"comments":{"nodes":[
        {"databaseId":31,"author":{"login":"alice"},"path":"src/lib.rs","line":7,"body":"Rename","createdAt":"2026-10-20T10:00:00Z"}]}}]}
    }}}}"#;

    #[test]
    fn graphql_answer_becomes_a_snapshot() {
        let s = parse_snapshot(GRAPHQL).unwrap();
        assert_eq!(s.state, PrState::Open);
        assert!(s.draft);
        assert_eq!((s.head.as_str(), s.base.as_str()), ("abc123", "main"));
        assert_eq!(s.review_decision, Some(ReviewDecision::ChangesRequested));
        let outcomes: Vec<_> = s
            .checks
            .iter()
            .map(|c| (c.name.as_str(), c.outcome))
            .collect();
        assert_eq!(
            outcomes,
            [
                ("build", Outcome::Success),
                ("test", Outcome::Failure),
                ("cancelled", Outcome::Neutral),
                ("skipped", Outcome::Neutral),
                ("neutral", Outcome::Neutral),
                ("slow", Outcome::Failure),
                ("queued", Outcome::Pending),
                ("legacy/ci", Outcome::Failure),
                ("legacy/lint", Outcome::Success),
            ]
        );
        assert_eq!(s.checks[1].url.as_deref(), Some("https://ci/2"));
        assert_eq!(s.reviews.len(), 2);
        assert_eq!(s.reviews[0].state, ReviewState::ChangesRequested);
        assert_eq!(s.reviews[1].state, ReviewState::Other);
        assert_eq!(s.reviews[1].author, "ghost");
        let ids: Vec<_> = s.comments.iter().map(|c| (c.id, c.kind)).collect();
        assert_eq!(ids, [(21, CommentKind::Issue), (31, CommentKind::Review)]);
        assert_eq!(s.comments[1].path.as_deref(), Some("src/lib.rs"));
        assert_eq!(s.comments[1].line, Some(7));
    }

    #[test]
    fn a_pr_without_checks_parses_and_a_missing_one_is_an_error() {
        let bare = r#"{"data":{"repository":{"pullRequest":{"state":"MERGED","isDraft":false,"merged":true,
          "headRefOid":"d","baseRefName":"main","title":"t","url":"u","reviewDecision":null,
          "commits":{"nodes":[{"commit":{"statusCheckRollup":null}}]},
          "reviews":{"nodes":[]},"comments":{"nodes":[]},"reviewThreads":{"nodes":[]}}}}}"#;
        let s = parse_snapshot(bare).unwrap();
        assert_eq!(s.state, PrState::Merged);
        assert!(s.checks.is_empty());
        assert_eq!(s.review_decision, None);

        let missing = r#"{"data":{"repository":null},"errors":[{"message":"Could not resolve to a Repository with the name 'a/b'."}]}"#;
        let err = parse_snapshot(missing).unwrap_err();
        assert_eq!(classify(&err), AccessProblem::NoAccess);
    }

    #[tokio::test]
    async fn pr_view_url_queries_the_graphql_api_for_the_pr_in_the_url() {
        let gh = FakeGh::new().on(&["api", "graphql"], Ok(GRAPHQL));
        let (pr, snap) = pr_view_url(&gh, "https://git.corp.example/org/svc/pull/42")
            .await
            .unwrap();
        assert_eq!(pr.number, 42);
        assert_eq!(snap.title, "Add things");
        let call = &gh.calls()[0];
        assert!(
            call.windows(2)
                .any(|w| w == ["--hostname", "git.corp.example"])
        );
        assert!(call.contains(&"owner=org".to_string()));
        assert!(call.contains(&"name=svc".to_string()));
        assert!(call.contains(&"number=42".to_string()));
        assert!(pr_view_url(&gh, "https://example.com/x").await.is_err());
    }

    #[tokio::test]
    async fn user_is_asked_once_per_host() {
        let gh = FakeGh::new().on(&["api", "user"], Ok("FranPerezFolgado\n"));
        assert_eq!(user(&gh, "github.com").await.unwrap(), "FranPerezFolgado");
        assert_eq!(user(&gh, "github.com").await.unwrap(), "FranPerezFolgado");
        assert_eq!(gh.calls().len(), 1);
        user(&gh, "git.corp.example").await.unwrap();
        assert_eq!(gh.calls().len(), 2);
    }

    #[tokio::test]
    async fn pr_create_returns_the_url_it_printed() {
        let gh = FakeGh::new().on(
            &["pr", "create"],
            Ok("Creating pull request...\nhttps://github.com/me/repo/pull/9\n"),
        );
        let url = pr_create(&gh, Path::new("/tmp"), "feature/x", "main", "T", "B", true)
            .await
            .unwrap();
        assert_eq!(url, "https://github.com/me/repo/pull/9");
        assert!(gh.calls()[0].contains(&"--draft".to_string()));
    }
}
