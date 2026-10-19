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
        Ok(Self {
            home,
            addr,
            allow_file_protocol,
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
