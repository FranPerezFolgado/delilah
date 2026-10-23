//! The `delilah` CLI, run against an Env's daemon with a clean environment.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

use crate::env::target_dir;

pub struct Cli {
    home: PathBuf,
    api: String,
}

impl Cli {
    pub fn new(home: &Path, api: &str) -> Cli {
        Cli {
            home: home.to_path_buf(),
            api: api.to_string(),
        }
    }

    /// Runs `delilah <args>`; returns (status, stdout, stderr).
    pub fn run(&self, args: &[&str]) -> (ExitStatus, String, String) {
        let bin = target_dir().join("delilah");
        assert!(
            bin.is_file(),
            "{} not found: run `cargo build -p delilah-cli` first",
            bin.display()
        );
        let out = Command::new(&bin)
            .env_clear()
            .env("HOME", &self.home)
            .env("PATH", "/usr/bin:/bin")
            .env("DELILAH_URL", &self.api)
            .args(args)
            .current_dir(&self.home)
            .stdin(Stdio::null())
            .output()
            .unwrap_or_else(|e| panic!("run {}: {e}", bin.display()));
        (
            out.status,
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }
}

#[cfg(all(test, feature = "e2e"))]
mod tests {
    use crate::{Env, EnvOptions};

    #[test]
    fn ls_on_an_empty_env_exits_zero() {
        let env = Env::start(EnvOptions::default());
        let (status, out, err) = env.cli().run(&["ls"]);
        assert!(status.success(), "ls failed: {status}\n{out}\n{err}");
    }
}
