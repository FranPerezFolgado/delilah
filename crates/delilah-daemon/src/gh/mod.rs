//! Talking to GitHub through the user's own `gh`: it holds the tokens (Delilah stores
//! no credentials) and resolves the repo from the remotes as the agent's `gh pr create`
//! does. The process sits behind the [`Gh`] trait (one method) so tests answer with
//! canned output; [`GhClient`] puts the typed calls on top of it.

#[cfg(test)]
pub(crate) mod fake;
mod graphql;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures_util::future::BoxFuture;
use serde::Deserialize;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::pr_types::{ListedPr, MergeMethod, Snapshot};

const TIMEOUT: Duration = Duration::from_secs(20);
/// The most of `gh`'s stderr kept in an error: it ends up in the database.
const MAX_STDERR: usize = 2048;

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
}

/// The real `gh`: the binary is `program` (`DELILAH_GH_BIN`, from `Config`) or `gh`,
/// found on the login shell's PATH with a clean environment, as agents get.
#[derive(Default)]
pub struct CliGh {
    pub program: Option<PathBuf>,
}

impl Gh for CliGh {
    fn run(
        &self,
        dir: Option<&Path>,
        args: Vec<String>,
        stdin: Option<String>,
    ) -> BoxFuture<'_, Result<String, GhError>> {
        let dir = dir.map(Path::to_path_buf);
        let bin = self.program.clone().unwrap_or_else(|| "gh".into());
        Box::pin(async move {
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
                    stderr: truncate_stderr(&String::from_utf8_lossy(&output.stderr)),
                })
            }
        })
    }
}

/// `stderr` trimmed and cut to [`MAX_STDERR`] bytes on a char boundary.
fn truncate_stderr(stderr: &str) -> String {
    let stderr = stderr.trim();
    if stderr.len() <= MAX_STDERR {
        return stderr.to_string();
    }
    let mut end = MAX_STDERR;
    while !stderr.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &stderr[..end])
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

impl PrRef {
    /// The URL without any trailing path, query or fragment.
    pub fn url(&self) -> String {
        format!(
            "https://{}/{}/pull/{}",
            self.host, self.owner_name, self.number
        )
    }
}

/// `https://<host>/<owner>/<name>/pull/<n>` (a trailing path or query is ignored). The
/// host is a plain name (with a port): no userinfo, no odd characters; owner and name
/// are what GitHub allows.
pub fn parse_pr_url(url: &str) -> Option<PrRef> {
    let rest = url.trim().strip_prefix("https://")?;
    let mut parts = rest.split(['?', '#']).next()?.split('/');
    let host = parts.next().filter(|h| valid_host(h))?;
    let (owner, name) = (parts.next()?, parts.next()?);
    if parts.next()? != "pull" || !valid_segment(owner) || !valid_segment(name) {
        return None;
    }
    let number = parts.next()?;
    // Digits only: `parse` would take a leading `+`.
    let number: u64 = number
        .chars()
        .all(|c| c.is_ascii_digit())
        .then(|| number.parse().ok())
        .flatten()
        .filter(|n| *n > 0)?;
    Some(PrRef {
        host: host.to_string(),
        owner_name: format!("{owner}/{name}"),
        number: i64::try_from(number).ok()?,
    })
}

/// `^[A-Za-z0-9._-]{1,100}$`
fn valid_segment(s: &str) -> bool {
    (1..=100).contains(&s.len())
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// `^[A-Za-z0-9.-]+(:\d+)?$`
fn valid_host(host: &str) -> bool {
    let (name, port) = match host.split_once(':') {
        Some((name, port)) => (name, Some(port)),
        None => (host, None),
    };
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && port.is_none_or(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

fn host_of(url: &str) -> Option<&str> {
    url.split("://")
        .nth(1)?
        .split('/')
        .next()
        .filter(|h| !h.is_empty())
}

/// The typed calls over a [`Gh`], with the login of each host remembered.
pub struct GhClient {
    inner: Arc<dyn Gh>,
    users: Mutex<HashMap<String, String>>,
    /// The host of each repo folder, asked once (see [`GhClient::host_of_dir`]).
    hosts: Mutex<HashMap<PathBuf, String>>,
}

impl GhClient {
    pub fn new(inner: Arc<dyn Gh>) -> Self {
        Self {
            inner,
            users: Mutex::default(),
            hosts: Mutex::default(),
        }
    }

    /// `gh repo view` in the repo's folder; honours `gh repo set-default` and the remotes.
    pub async fn repo_view(&self, dir: &Path) -> Result<RepoInfo, GhError> {
        let args = [
            "repo",
            "view",
            "--json",
            "nameWithOwner,url,defaultBranchRef",
        ];
        let out = self.inner.run(Some(dir), strings(&args), None).await?;
        parse_repo_view(&out)
    }

    /// The host of the repo in `dir`, remembered per folder.
    pub async fn host_of_dir(&self, dir: &Path) -> Result<String, GhError> {
        if let Some(host) = self
            .hosts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(dir)
        {
            return Ok(host.clone());
        }
        let host = self.repo_view(dir).await?.host;
        self.hosts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(dir.to_path_buf(), host.clone());
        Ok(host)
    }

    /// The login `gh` is signed in as on `host`, asked once per host.
    pub async fn user(&self, host: &str) -> Result<String, GhError> {
        if let Some(login) = self
            .users
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(host)
        {
            return Ok(login.clone());
        }
        let args = ["api", "user", "--hostname", host, "--jq", ".login"];
        let login = self
            .inner
            .run(None, strings(&args), None)
            .await?
            .trim()
            .to_string();
        if login.is_empty() {
            return Err(GhError::Parse("empty login".into()));
        }
        self.users
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(host.to_string(), login.clone());
        Ok(login)
    }

    /// The login remembered for `host`, without asking `gh`.
    pub fn cached_user(&self, host: &str) -> Option<String> {
        self.users
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(host)
            .cloned()
    }

    /// Asks `gh` who is signed in on `host` right now (the remembered login may be
    /// stale after `gh auth switch`) and remembers the answer.
    pub async fn user_fresh(&self, host: &str) -> Result<String, GhError> {
        self.users
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(host);
        self.user(host).await
    }

    /// The PRs whose head is `branch`, in any state.
    pub async fn pr_list(&self, dir: &Path, branch: &str) -> Result<Vec<ListedPr>, GhError> {
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
        let out = self.inner.run(Some(dir), strings(&args), None).await?;
        serde_json::from_str(&out).map_err(|e| GhError::Parse(e.to_string()))
    }

    /// Everything one check needs about a PR, in a single call (research R2).
    pub async fn pr_snapshot(
        &self,
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
            format!("query={}", graphql::SNAPSHOT_QUERY),
            // Strings with -f so a numeric owner is not turned into a number.
            "-f".into(),
            format!("owner={owner}"),
            "-f".into(),
            format!("name={name}"),
            "-F".into(),
            format!("number={number}"),
        ];
        let out = self.inner.run(None, args, None).await?;
        graphql::parse_snapshot(&out)
    }

    /// Reads a PR by its URL, without a repo folder (the worktree may be gone).
    pub async fn pr_view_url(&self, url: &str) -> Result<(PrRef, Snapshot), GhError> {
        let pr = parse_pr_url(url).ok_or_else(|| GhError::Parse(format!("not a PR URL: {url}")))?;
        let snapshot = self
            .pr_snapshot(&pr.host, &pr.owner_name, pr.number)
            .await?;
        Ok((pr, snapshot))
    }

    /// Merges the PR at `url` with `method` (`gh pr merge`). Runs outside any repo folder:
    /// the URL names the PR.
    pub async fn pr_merge(&self, url: &str, method: MergeMethod) -> Result<(), GhError> {
        // `--` so a URL can never be read as an option.
        if !url.starts_with("https://") {
            return Err(GhError::Parse(format!("`{url}` is not an https URL")));
        }
        let args = strings(&["pr", "merge", method.flag(), "--", url]);
        self.inner.run(None, args, None).await.map(drop)
    }

    /// Marks the draft PR at `url` ready for review (`gh pr ready`). Only ever called
    /// from the user's explicit request, never by the daemon on its own.
    pub async fn pr_ready(&self, url: &str) -> Result<(), GhError> {
        if !url.starts_with("https://") {
            return Err(GhError::Parse(format!("`{url}` is not an https URL")));
        }
        let args = strings(&["pr", "ready", "--", url]);
        self.inner.run(None, args, None).await.map(drop)
    }

    /// Creates the PR and returns its URL. The body goes through stdin.
    pub async fn pr_create(
        &self,
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
        let out = self
            .inner
            .run(Some(dir), args, Some(body.to_string()))
            .await?;
        out.lines()
            .rev()
            .map(str::trim)
            .find(|l| l.starts_with("http"))
            .map(str::to_string)
            .ok_or_else(|| GhError::Parse(format!("no URL in `{}`", out.trim())))
    }
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

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| a.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::fake::FakeGh;
    use super::graphql::tests::GRAPHQL;
    use super::*;
    use crate::pr_types::PrState;

    fn client(fake: FakeGh) -> (Arc<FakeGh>, GhClient) {
        let fake = Arc::new(fake);
        (fake.clone(), GhClient::new(fake))
    }

    #[tokio::test]
    async fn pr_merge_takes_only_https_urls_after_a_double_dash() {
        let (fake, gh) = client(FakeGh::new().on(&["pr", "merge"], Ok("")));
        let bad = gh.pr_merge("--admin", MergeMethod::Merge).await;
        assert!(matches!(bad, Err(GhError::Parse(_))));
        assert!(fake.calls().is_empty());
        gh.pr_merge("https://github.com/o/r/pull/7", MergeMethod::Squash)
            .await
            .unwrap();
        assert_eq!(
            fake.calls()[0],
            [
                "pr",
                "merge",
                "--squash",
                "--",
                "https://github.com/o/r/pull/7"
            ]
        );
    }

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
        assert_eq!(
            parse_pr_url("https://git.corp.example/org/svc/pull/42/files?x=1#top")
                .unwrap()
                .url(),
            "https://git.corp.example/org/svc/pull/42"
        );
        assert_eq!(parse_pr_url("https://github.com/me/repo/issues/4"), None);
        assert_eq!(parse_pr_url("https://github.com/me/repo/pull/x"), None);
        assert_eq!(parse_pr_url("not a url"), None);
    }

    #[test]
    fn pr_urls_with_odd_hosts_or_numbers_are_refused() {
        let url = |host: &str, n: &str| format!("https://{host}/me/repo/pull/{n}");
        for host in [
            "user@github.com",
            "user:pw@github.com",
            "git hub.com",
            "github.com:",
            "github.com:80a",
            "gi_thub.com",
            "[::1]",
            "",
        ] {
            assert_eq!(parse_pr_url(&url(host, "1")), None, "{host}");
        }
        assert_eq!(
            parse_pr_url(&url("ghe.corp:8443", "1")).unwrap().host,
            "ghe.corp:8443"
        );
        for n in ["0", "-3", "+4", "", "99999999999999999999"] {
            assert_eq!(parse_pr_url(&url("github.com", n)), None, "{n}");
        }
    }

    #[test]
    fn pr_urls_with_odd_owners_or_names_are_refused() {
        for (owner, name) in [
            ("me%2f", "repo"),
            ("me", "re po"),
            ("me", "repo;ls"),
            ("m\u{e9}", "repo"),
            ("me", "re\\po"),
            ("", "repo"),
        ] {
            let url = format!("https://github.com/{owner}/{name}/pull/1");
            assert_eq!(parse_pr_url(&url), None, "{url}");
        }
        let long = "a".repeat(101);
        assert_eq!(
            parse_pr_url(&format!("https://github.com/{long}/r/pull/1")),
            None
        );
        let ok = parse_pr_url("https://github.com/Me_1/re.po-x/pull/1").unwrap();
        assert_eq!(ok.owner_name, "Me_1/re.po-x");
    }

    #[test]
    fn long_stderr_is_cut_on_a_char_boundary() {
        assert_eq!(truncate_stderr("  short \n"), "short");
        let long = "é".repeat(2000);
        let cut = truncate_stderr(&long);
        assert!(cut.ends_with('…'));
        assert!(cut.len() <= MAX_STDERR + '…'.len_utf8());
        assert!(cut.starts_with("éé"));
    }

    #[tokio::test]
    async fn pr_list_parses_gh_json() {
        let out = r#"[{"number":3,"url":"https://github.com/me/repo/pull/3","state":"OPEN","updatedAt":"2026-10-20T10:00:00Z"},
                      {"number":2,"url":"https://github.com/me/repo/pull/2","state":"CLOSED","updatedAt":"2026-10-19T10:00:00Z"}]"#;
        let (gh, client) = client(FakeGh::new().on(&["pr", "list"], Ok(out)));
        let prs = client
            .pr_list(Path::new("/tmp"), "feature/x")
            .await
            .unwrap();
        assert_eq!(prs.len(), 2);
        assert_eq!(prs[0].state, PrState::Open);
        assert_eq!(prs[1].number, 2);
        assert_eq!(gh.calls()[0][..4], ["pr", "list", "--head", "feature/x"]);
    }

    #[tokio::test]
    async fn pr_view_url_queries_the_graphql_api_for_the_pr_in_the_url() {
        let (gh, client) = client(FakeGh::new().on(&["api", "graphql"], Ok(GRAPHQL)));
        let (pr, snap) = client
            .pr_view_url("https://git.corp.example/org/svc/pull/42")
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
        assert!(client.pr_view_url("https://example.com/x").await.is_err());
    }

    #[tokio::test]
    async fn user_is_asked_once_per_host() {
        let (gh, client) = client(FakeGh::new().on(&["api", "user"], Ok("FranPerezFolgado\n")));
        assert_eq!(client.user("github.com").await.unwrap(), "FranPerezFolgado");
        assert_eq!(client.user("github.com").await.unwrap(), "FranPerezFolgado");
        assert_eq!(gh.calls().len(), 1);
        client.user("git.corp.example").await.unwrap();
        assert_eq!(gh.calls().len(), 2);
    }

    #[tokio::test]
    async fn the_host_of_a_folder_is_asked_once() {
        let (gh, client) = client(FakeGh::happy());
        assert_eq!(
            client.host_of_dir(Path::new("/a")).await.unwrap(),
            "github.com"
        );
        assert_eq!(
            client.host_of_dir(Path::new("/a")).await.unwrap(),
            "github.com"
        );
        assert_eq!(gh.calls().len(), 1);
        client.host_of_dir(Path::new("/b")).await.unwrap();
        assert_eq!(gh.calls().len(), 2);
    }

    #[tokio::test]
    async fn pr_create_returns_the_url_it_printed() {
        let (gh, client) = client(FakeGh::new().on(
            &["pr", "create"],
            Ok("Creating pull request...\nhttps://github.com/me/repo/pull/9\n"),
        ));
        let url = client
            .pr_create(Path::new("/tmp"), "feature/x", "main", "T", "B", true)
            .await
            .unwrap();
        assert_eq!(url, "https://github.com/me/repo/pull/9");
        assert!(gh.calls()[0].contains(&"--draft".to_string()));
    }
}
