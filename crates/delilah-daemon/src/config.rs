use std::net::SocketAddr;
use std::path::PathBuf;

pub const DEFAULT_ADDR: &str = "127.0.0.1:7420";

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
    /// 127.0.0.1:7420) and `DELILAH_ALLOW_FILE_PROTOCOL=1`.
    pub fn from_env() -> Result<Self, String> {
        let home = match std::env::var_os("DELILAH_HOME") {
            Some(dir) => PathBuf::from(dir),
            None => {
                let user_home = std::env::var_os("HOME").ok_or("HOME is not set")?;
                PathBuf::from(user_home).join(".delilah")
            }
        };
        let addr = std::env::var("DELILAH_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string());
        let addr: SocketAddr = addr
            .parse()
            .map_err(|e| format!("invalid DELILAH_ADDR `{addr}`: {e}"))?;
        if !addr.ip().is_loopback() {
            return Err(format!(
                "DELILAH_ADDR must be a loopback address, got {addr}"
            ));
        }
        let allow_file_protocol =
            std::env::var("DELILAH_ALLOW_FILE_PROTOCOL").is_ok_and(|v| v == "1");
        let notify = std::env::var("DELILAH_NOTIFY").map_or(true, |v| v != "0");
        let ui_origin = std::env::var("DELILAH_UI_ORIGIN")
            .ok()
            .filter(|o| !o.is_empty());
        let claude_bin = claude_bin_from(std::env::var_os("DELILAH_CLAUDE_BIN"));
        Ok(Self {
            home,
            addr,
            allow_file_protocol,
            notify,
            ui_origin,
            claude_bin,
        })
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

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
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
}
