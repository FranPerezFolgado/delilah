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
    /// Program to run instead of `claude` (`DELILAH_AGENT_BIN`); only tests set it.
    pub agent_program: Option<PathBuf>,
    /// Secrets in memory instead of the Keychain (`DELILAH_SECRETS=memory`); tests only.
    pub secrets_in_memory: bool,
    /// Accept `http://` Jira sites on loopback (`DELILAH_JIRA_ALLOW_HTTP_LOOPBACK=1`); tests only.
    pub jira_http_loopback: bool,
    /// Tick of the PR loop (`DELILAH_PR_POLL_SECS`).
    pub pr_poll: Duration,
}

impl Config {
    /// Reads `DELILAH_HOME` (default `~/.delilah`), `DELILAH_ADDR` (default
    /// 127.0.0.1:7420), `DELILAH_ALLOW_FILE_PROTOCOL=1`, `DELILAH_NOTIFY`,
    /// `DELILAH_UI_ORIGIN` and the test-only switches `DELILAH_AGENT_BIN`,
    /// `DELILAH_SECRETS`, `DELILAH_JIRA_ALLOW_HTTP_LOOPBACK` and `DELILAH_PR_POLL_SECS`.
    pub fn from_env() -> Result<Self, String> {
        Self::from_vars(|k| std::env::var(k).ok())
    }

    /// `from_env` over any source of variables, so tests need not touch the process env.
    pub fn from_vars(var: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let home = match var("DELILAH_HOME") {
            Some(dir) => PathBuf::from(dir),
            None => {
                let user_home = var("HOME").ok_or("HOME is not set")?;
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
        let agent_program = match var("DELILAH_AGENT_BIN") {
            None => None,
            Some(bin) => {
                let path = PathBuf::from(&bin);
                if !path.is_absolute() || !is_executable(&path) {
                    return Err(format!(
                        "DELILAH_AGENT_BIN must be an absolute path to an executable, got {bin}"
                    ));
                }
                Some(path)
            }
        };
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
        Ok(Self {
            home,
            addr,
            allow_file_protocol,
            notify,
            ui_origin,
            agent_program,
            secrets_in_memory,
            jira_http_loopback,
            pr_poll,
        })
    }

    /// What to log at start-up about the test-only switches that are set.
    pub fn override_notes(var: impl Fn(&str) -> Option<String>) -> Vec<String> {
        let mut notes = Vec::new();
        if let Some(bin) = var("DELILAH_AGENT_BIN") {
            notes.push(format!("agent program overridden: {bin}"));
        }
        if var("DELILAH_SECRETS").is_some() {
            notes.push("secrets in memory".to_string());
        }
        if var("DELILAH_JIRA_ALLOW_HTTP_LOOPBACK").is_some_and(|v| v == "1") {
            notes.push("jira over http on loopback allowed".to_string());
        }
        if let Some(secs) = var("DELILAH_PR_POLL_SECS") {
            notes.push(format!("pr poll every {} s", secs.trim()));
        }
        notes
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

fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let mut map: std::collections::HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        map.entry("HOME".into()).or_insert("/home/x".into());
        move |k| map.get(k).cloned()
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
    fn override_notes_log_every_switch_that_is_set() {
        let none = |_: &str| None;
        assert!(Config::override_notes(none).is_empty());
        let notes = Config::override_notes(|k| match k {
            "DELILAH_SECRETS" => Some("memory".into()),
            "DELILAH_PR_POLL_SECS" => Some("30".into()),
            _ => None,
        });
        assert_eq!(notes, ["secrets in memory", "pr poll every 30 s"]);
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
