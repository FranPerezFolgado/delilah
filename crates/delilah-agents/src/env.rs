//! The environment agents run with: a clean one (see `KEPT_ENV`), but with the PATH
//! of the user's login shell, so tools installed through brew, nvm, mise or a hook
//! like RTK are found exactly as in their own terminal. Under launchd the daemon
//! itself gets a bare PATH, so it has to be asked of the shell.

use std::ffi::OsString;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::claude::KEPT_ENV;

static LOGIN_PATH: OnceLock<String> = OnceLock::new();
const MARK: &str = "__DELILAH_PATH__";
const TIMEOUT: Duration = Duration::from_secs(10);

/// Reads the login shell's PATH once (call it when the daemon starts). Returns it, or
/// `None` if the shell could not tell, in which case the daemon's own PATH is used.
pub fn init_login_path() -> Option<&'static str> {
    if let Some(path) = LOGIN_PATH.get() {
        return Some(path);
    }
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    // Interactive and login, as a new terminal tab: .zprofile and .zshrc both count.
    let mut child = Command::new(&shell)
        .args(["-lic", &format!("printf '{MARK}%s{MARK}' \"$PATH\"")])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let start = Instant::now();
    while child.try_wait().ok()?.is_none() {
        if start.elapsed() > TIMEOUT {
            let _ = child.kill();
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let output = child.wait_with_output().ok()?;
    let path = between_marks(&String::from_utf8_lossy(&output.stdout))?;
    Some(LOGIN_PATH.get_or_init(|| path))
}

/// Shell start-up files may print anything around it (ssh-agent, banners): keep only
/// what sits between the marks.
fn between_marks(out: &str) -> Option<String> {
    let start = out.find(MARK)? + MARK.len();
    let len = out[start..].find(MARK)?;
    let path = &out[start..start + len];
    (!path.is_empty()).then(|| path.to_string())
}

/// The variables an agent gets.
pub fn agent_env() -> Vec<(String, OsString)> {
    KEPT_ENV
        .iter()
        .filter_map(|key| {
            let value = match (*key, LOGIN_PATH.get()) {
                ("PATH", Some(login)) => Some(OsString::from(login)),
                _ => std::env::var_os(key),
            };
            value.map(|v| (key.to_string(), v))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_the_path_between_the_marks() {
        let noisy = format!("Agent pid 46089\n{MARK}/opt/homebrew/bin:/usr/bin{MARK}");
        assert_eq!(
            between_marks(&noisy).as_deref(),
            Some("/opt/homebrew/bin:/usr/bin")
        );
        assert_eq!(between_marks("nothing here"), None);
        assert_eq!(between_marks(&format!("{MARK}{MARK}")), None);
    }
}
