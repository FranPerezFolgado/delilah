use std::ffi::OsString;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

pub const DEFAULT_ADDR: &str = "127.0.0.1:7420";
/// How often the PR loop checks, unless `DELILAH_PR_POLL_SECS` says otherwise.
pub const DEFAULT_PR_POLL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct Config {
    /// Where Delilah keeps its state: the database, workspaces and logs.
    pub home: PathBuf,
    /// Loopback only: the API has no authentication yet.
    pub addr: SocketAddr,
    /// Submodules over `file://`, needed only by the local fixture repos.
    pub allow_file_protocol: bool,
    /// Desktop notifications; `DELILAH_NOTIFY=0` turns them off.
    pub notify: bool,
    /// One more browser origin allowed to call the API (`DELILAH_UI_ORIGIN`): the UI
    /// of a preview (`scripts/preview.sh`), served on its own port.
    pub ui_origin: Option<String>,
    /// The `claude` binary for one-shot calls (`DELILAH_CLAUDE_BIN`, default `claude`);
    /// tests point it at a fake script.
    pub claude_bin: PathBuf,
    /// Program to run instead of `claude` (`DELILAH_AGENT_BIN`); only tests set it.
    pub agent_program: Option<PathBuf>,
    /// Program to run instead of `gh` (`DELILAH_GH_BIN`): tests and previews with a fake.
    pub gh_program: Option<PathBuf>,
    /// Secrets in memory instead of the Keychain (`DELILAH_SECRETS=memory`); tests only.
    pub secrets_in_memory: bool,
    /// Accept `http://` Jira sites on loopback (`DELILAH_JIRA_ALLOW_HTTP_LOOPBACK=1`); tests only.
    pub jira_http_loopback: bool,
    /// Tick of the PR loop (`DELILAH_PR_POLL_SECS`).
    pub pr_poll: Duration,
}

/// The binary named by `DELILAH_CLAUDE_BIN`, or `claude` when unset or empty.
fn claude_bin_from(value: Option<std::ffi::OsString>) -> PathBuf {
    match value {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => PathBuf::from("claude"),
    }
}

impl Config {
    /// Reads `DELILAH_HOME` (default `~/.delilah`), `DELILAH_ADDR` (default
    /// 127.0.0.1:7420), `DELILAH_ALLOW_FILE_PROTOCOL=1`, `DELILAH_NOTIFY`,
    /// `DELILAH_UI_ORIGIN`, `DELILAH_CLAUDE_BIN` and the test-only switches `DELILAH_AGENT_BIN`,
    /// `DELILAH_SECRETS`, `DELILAH_JIRA_ALLOW_HTTP_LOOPBACK` and `DELILAH_PR_POLL_SECS`.
    pub fn from_env() -> Result<Self, String> {
        Self::from_vars(|k| std::env::var_os(k))
    }

    /// `from_env` over any source of variables, so tests need not touch the process env.
    /// Paths stay `OsString` (a non-UTF-8 `DELILAH_HOME` is still a path); the other
    /// variables are read as text.
    pub fn from_vars(var_os: impl Fn(&str) -> Option<OsString>) -> Result<Self, String> {
        let var = |k: &str| var_os(k).map(|v| v.to_string_lossy().into_owned());
        let home = match var_os("DELILAH_HOME") {
            Some(dir) => PathBuf::from(dir),
            None => {
                let user_home = var_os("HOME").ok_or("HOME is not set")?;
                PathBuf::from(user_home).join(".delilah")
            }
        };
        let addr = var("DELILAH_ADDR").unwrap_or_else(|| DEFAULT_ADDR.to_string());
        let addr: SocketAddr = addr
            .parse()
            .map_err(|e| format!("invalid DELILAH_ADDR `{addr}`: {e}"))?;
        if !addr.ip().is_loopback() {
            return Err(format!(
                "DELILAH_ADDR must be a loopback address, got {addr}"
            ));
        }
        let allow_file_protocol = var("DELILAH_ALLOW_FILE_PROTOCOL").is_some_and(|v| v == "1");
        let notify = var("DELILAH_NOTIFY").is_none_or(|v| v != "0");
        let ui_origin = var("DELILAH_UI_ORIGIN").filter(|o| !o.is_empty());
        let agent_program = program(&var_os, "DELILAH_AGENT_BIN")?;
        let gh_program = program(&var_os, "DELILAH_GH_BIN")?;
        let secrets_in_memory = match var("DELILAH_SECRETS").as_deref() {
            None => false,
            Some("memory") => true,
            Some(_) => return Err(r#"DELILAH_SECRETS must be "memory" or unset"#.to_string()),
        };
        let jira_http_loopback = var("DELILAH_JIRA_ALLOW_HTTP_LOOPBACK").is_some_and(|v| v == "1");
        let pr_poll = match var("DELILAH_PR_POLL_SECS") {
            None => DEFAULT_PR_POLL,
            Some(v) => match v.trim().parse::<u64>() {
                Ok(n) if n >= 1 => Duration::from_secs(n),
                _ => {
                    return Err(format!(
                        "DELILAH_PR_POLL_SECS must be an integer of 1 or more, got {v}"
                    ));
                }
            },
        };
        let claude_bin = claude_bin_from(var_os("DELILAH_CLAUDE_BIN"));
        Ok(Self {
            home,
            addr,
            allow_file_protocol,
            notify,
            ui_origin,
            claude_bin,
            agent_program,
            gh_program,
            secrets_in_memory,
            jira_http_loopback,
            pr_poll,
        })
    }

    /// What to warn about at start-up: the test-only switches this config turned on.
    pub fn override_notes(&self) -> Vec<String> {
        let mut notes = Vec::new();
        if let Some(bin) = &self.agent_program {
            notes.push(format!("agent program overridden: {}", bin.display()));
        }
        if let Some(bin) = &self.gh_program {
            notes.push(format!("gh program overridden: {}", bin.display()));
        }
        if self.secrets_in_memory {
            notes.push("secrets in memory".to_string());
        }
        if self.jira_http_loopback {
            notes.push("jira over http on loopback allowed".to_string());
        }
        if self.pr_poll != DEFAULT_PR_POLL {
            notes.push(format!("pr poll every {} s", self.pr_poll.as_secs()));
        }
        notes
    }

    /// Defaults for unit tests to fill in with `..Config::test_defaults()`, so a new
    /// field does not touch every test that builds a `Config`.
    #[cfg(test)]
    pub(crate) fn test_defaults() -> Self {
        Self {
            home: PathBuf::from("/nonexistent/delilah-test-home"),
            addr: DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            claude_bin: PathBuf::from("claude"),
            agent_program: None,
            gh_program: None,
            secrets_in_memory: false,
            jira_http_loopback: false,
            pr_poll: DEFAULT_PR_POLL,
        }
    }

    pub fn database_path(&self) -> PathBuf {
        self.home.join("delilah.db")
    }

    pub fn workspaces_dir(&self) -> PathBuf {
        self.home.join("workspaces")
    }

    /// Per-task files that must stay out of the repo: agent settings, logs, raw hooks.
    pub fn task_dir(&self, key: &str) -> PathBuf {
        self.home.join("tasks").join(key)
    }

    /// The Claude Code plugin Delilah writes at start-up (the idea-conversation skill).
    pub fn plugin_dir(&self) -> PathBuf {
        self.home.join("plugins/delilah")
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

/// An optional program override: unset, or an absolute path to an executable file.
fn program(
    var_os: &impl Fn(&str) -> Option<OsString>,
    name: &str,
) -> Result<Option<PathBuf>, String> {
    let Some(bin) = var_os(name) else {
        return Ok(None);
    };
    let path = PathBuf::from(bin);
    if !path.is_absolute() || !is_executable(&path) {
        return Err(format!(
            "{name} must be an absolute path to an executable, got {}",
            path.display()
        ));
    }
    Ok(Some(path))
}

fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_bin_defaults_to_claude() {
        assert_eq!(claude_bin_from(None), PathBuf::from("claude"));
        assert_eq!(claude_bin_from(Some("".into())), PathBuf::from("claude"));
    }

    #[test]
    fn claude_bin_reads_the_given_path() {
        assert_eq!(
            claude_bin_from(Some("/tmp/fake-claude".into())),
            PathBuf::from("/tmp/fake-claude")
        );
    }

    #[test]
    fn plugin_dir_is_under_home() {
        let config = Config {
            home: PathBuf::from("/tmp/delilah-home"),
            ..Config::from_env().unwrap()
        };
        assert_eq!(
            config.plugin_dir(),
            PathBuf::from("/tmp/delilah-home/plugins/delilah")
        );
    }

    fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let mut map: std::collections::HashMap<String, OsString> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), OsString::from(v)))
            .collect();
        map.entry("HOME".into()).or_insert("/home/x".into());
        move |k| map.get(k).cloned()
    }

    #[test]
    fn a_non_utf8_delilah_home_is_still_used() {
        use std::os::unix::ffi::OsStringExt;
        let dir = OsString::from_vec(b"/tmp/d\xffh".to_vec());
        let expected = PathBuf::from(&dir);
        let c = Config::from_vars(move |k| (k == "DELILAH_HOME").then(|| dir.clone())).unwrap();
        assert_eq!(c.home, expected);
    }

    fn agent_bin_err(bin: &str) -> String {
        Config::from_vars(vars(&[("DELILAH_AGENT_BIN", bin)])).unwrap_err()
    }

    #[test]
    fn agent_bin_unset_is_none() {
        assert_eq!(Config::from_vars(vars(&[])).unwrap().agent_program, None);
    }

    #[test]
    fn agent_bin_absolute_executable_is_accepted() {
        let c = Config::from_vars(vars(&[("DELILAH_AGENT_BIN", "/bin/sh")])).unwrap();
        assert_eq!(c.agent_program, Some(PathBuf::from("/bin/sh")));
    }

    #[test]
    fn agent_bin_relative_missing_or_not_executable_is_rejected() {
        let dir = std::env::temp_dir().join(format!("delilah-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let plain = dir.join("plain");
        std::fs::write(&plain, "x").unwrap();
        let plain = plain.to_str().unwrap().to_string();
        for bin in ["fake-claude", "/no/such/agent", plain.as_str()] {
            assert_eq!(
                agent_bin_err(bin),
                format!("DELILAH_AGENT_BIN must be an absolute path to an executable, got {bin}")
            );
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn secrets_unset_is_keychain_memory_is_memory_other_is_error() {
        assert!(!Config::from_vars(vars(&[])).unwrap().secrets_in_memory);
        let c = Config::from_vars(vars(&[("DELILAH_SECRETS", "memory")])).unwrap();
        assert!(c.secrets_in_memory);
        let e = Config::from_vars(vars(&[("DELILAH_SECRETS", "file")])).unwrap_err();
        assert_eq!(e, r#"DELILAH_SECRETS must be "memory" or unset"#);
    }

    #[test]
    fn override_notes_log_every_switch_that_is_on() {
        assert!(
            Config::from_vars(vars(&[]))
                .unwrap()
                .override_notes()
                .is_empty()
        );
        let c = Config::from_vars(vars(&[
            ("DELILAH_AGENT_BIN", "/bin/sh"),
            ("DELILAH_GH_BIN", "/bin/echo"),
            ("DELILAH_SECRETS", "memory"),
            ("DELILAH_JIRA_ALLOW_HTTP_LOOPBACK", "1"),
            ("DELILAH_PR_POLL_SECS", "2"),
        ]))
        .unwrap();
        assert_eq!(
            c.override_notes(),
            [
                "agent program overridden: /bin/sh",
                "gh program overridden: /bin/echo",
                "secrets in memory",
                "jira over http on loopback allowed",
                "pr poll every 2 s",
            ]
        );
    }

    #[test]
    fn gh_bin_is_checked_like_the_agent_bin() {
        assert_eq!(Config::from_vars(vars(&[])).unwrap().gh_program, None);
        let c = Config::from_vars(vars(&[("DELILAH_GH_BIN", "/bin/sh")])).unwrap();
        assert_eq!(c.gh_program, Some(PathBuf::from("/bin/sh")));
        for bin in ["scripts/fake-gh.sh", "/no/such/gh"] {
            assert_eq!(
                Config::from_vars(vars(&[("DELILAH_GH_BIN", bin)])).unwrap_err(),
                format!("DELILAH_GH_BIN must be an absolute path to an executable, got {bin}")
            );
        }
    }

    #[test]
    fn jira_loopback_flag() {
        assert!(!Config::from_vars(vars(&[])).unwrap().jira_http_loopback);
        let c = Config::from_vars(vars(&[("DELILAH_JIRA_ALLOW_HTTP_LOOPBACK", "1")])).unwrap();
        assert!(c.jira_http_loopback);
    }

    #[test]
    fn pr_poll_default_valid_and_invalid() {
        assert_eq!(
            Config::from_vars(vars(&[])).unwrap().pr_poll,
            Duration::from_secs(30)
        );
        let c = Config::from_vars(vars(&[("DELILAH_PR_POLL_SECS", "1")])).unwrap();
        assert_eq!(c.pr_poll, Duration::from_secs(1));
        for bad in ["0", "-3", "abc"] {
            assert!(Config::from_vars(vars(&[("DELILAH_PR_POLL_SECS", bad)])).is_err());
        }
    }
}
