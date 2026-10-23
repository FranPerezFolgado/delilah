//! One isolated daemon: its own temp home, port, fixtures, fake `claude` and `gh`, and a
//! tripwire that fails the test if the real `claude` or `gh` is ever run (FR-006a).

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::fs::File;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde_json::{Value, json};

use crate::fake::record::Record;
use crate::fixtures::{FakeGh, Fixtures};
use crate::{DEFAULT_TIMEOUT, tail_of, wait_for};

/// Ports the installed app and the previews use; the harness never takes them.
const FORBIDDEN_PORTS: std::ops::RangeInclusive<u16> = 7501..=7509;
const INSTALLED_PORT: u16 = 7420;
const START_ATTEMPTS: usize = 3;

/// Content of `fake/scenarios/default.json`: stop, then wait for input.
pub const DEFAULT_SCENARIO: &str = r#"{"steps":[{"stop":"Fake Claude stopped."}]}"#;

/// What a test can change about its daemon.
#[derive(Debug, Clone, Default)]
pub struct EnvOptions {
    /// The whole `default.json` scenario (`{"steps": [...]}`); `None` = stop then wait.
    pub default_scenario: Option<Value>,
    /// `DELILAH_UI_ORIGIN`, for the UI layer.
    pub ui_origin: Option<String>,
    /// Smoke test only: the real `claude` with the user's HOME; `gh` stays fake and
    /// tripwired. Takes effect only with `DELILAH_E2E_REAL_CLAUDE=1` (else panics).
    pub real_claude: bool,
    /// Start the fake Jira server (set by the layer that provides it).
    pub jira: bool,
    /// Test only: leave `DELILAH_AGENT_BIN` out, so the daemon runs `claude` from PATH
    /// and hits the tripwire (US1 acceptance 6).
    pub no_agent_bin: bool,
}

/// A running helper the Env stops on drop (the fake Jira server).
pub struct JiraHandle {
    pub url: String,
    pub stop: Option<Box<dyn FnOnce() + Send>>,
}

/// One isolated daemon. Dropping it stops everything it started.
pub struct Env {
    dir: Option<tempfile::TempDir>,
    pub root: PathBuf,
    pub home: PathBuf,
    pub api: String,
    pub port: u16,
    /// The `delilahd` child, killed by handle on drop (never by name).
    pub daemon: Option<Child>,
    pub jira: Option<JiraHandle>,
    pub fixtures: Fixtures,
    pub gh: FakeGh,
    pub log: PathBuf,
    pub opts: EnvOptions,
    /// Set by `finish`, which reports the tripwire itself instead of panicking in drop.
    tripwire_reported: bool,
}

/// The directory with the built binaries: `DELILAH_E2E_TARGET` or `<repo>/target/debug`.
pub fn target_dir() -> PathBuf {
    match std::env::var_os("DELILAH_E2E_TARGET") {
        Some(t) => PathBuf::from(t),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug"),
    }
}

fn gh_script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/fake-gh.sh")
        .canonicalize()
        .expect("scripts/fake-gh.sh")
}

/// A free port that is not 7420 nor 7501-7509, taken from `pick` (the OS in `free_port`).
pub fn free_port_with(mut pick: impl FnMut() -> u16) -> u16 {
    loop {
        let p = pick();
        if p != INSTALLED_PORT && !FORBIDDEN_PORTS.contains(&p) {
            return p;
        }
    }
}

pub fn free_port() -> u16 {
    free_port_with(|| {
        std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .expect("a free port")
            .port()
    })
}

fn write_exec(path: &Path, text: &str) -> std::io::Result<()> {
    std::fs::write(path, text)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}

/// `s` as one single-quoted shell word (`'` becomes `'\''`).
pub fn sh_quote(s: impl std::fmt::Display) -> String {
    format!("'{}'", s.to_string().replace('\'', "'\\''"))
}

/// Builds the folder tree under `root` (see data-model.md "Layout of root").
pub fn layout(root: &Path, opts: &EnvOptions) -> std::io::Result<()> {
    let r = root.display();
    let home = root.join("home");
    for d in [
        "home/.claude",
        "delilah",
        "bin",
        "tripwire",
        "tripwire-gh",
        "fake/scenarios",
        "fake/records",
        "gh",
        "fixtures",
    ] {
        std::fs::create_dir_all(root.join(d))?;
    }
    std::fs::write(
        home.join(".zshrc"),
        format!(
            "export PATH={}\":$PATH\"\n",
            sh_quote(format!("{r}/tripwire"))
        ),
    )?;
    std::fs::write(
        home.join(".gitconfig"),
        "[user]\n\tname = Delilah E2E\n\temail = e2e@delilah.invalid\n\
         [protocol \"file\"]\n\tallow = always\n[commit]\n\tgpgsign = false\n",
    )?;
    write_exec(
        &root.join("bin/claude-fake"),
        &format!(
            "#!/bin/sh\nexec {} --fake-dir {} \"$@\"\n",
            sh_quote(target_dir().join("fake-claude").display()),
            sh_quote(format!("{r}/fake"))
        ),
    )?;
    write_exec(
        &root.join("bin/gh-fake"),
        &format!(
            "#!/bin/sh\nFAKE_GH_DIR={} exec {} \"$@\"\n",
            sh_quote(format!("{r}/gh")),
            sh_quote(gh_script().display())
        ),
    )?;
    // The daemon's one-shot Claude calls (branch name, submodule suggestion): logged
    // and failed, so the daemon falls back to its own names, at once and the same each run.
    write_exec(
        &root.join("bin/claude-oneshot"),
        &format!(
            "#!/bin/sh\necho \"claude $*\" >> {}\nexit 1\n",
            sh_quote(format!("{r}/oneshot.log"))
        ),
    )?;
    for (dir, name) in [
        ("tripwire", "claude"),
        ("tripwire", "gh"),
        ("tripwire-gh", "gh"),
    ] {
        write_exec(
            &root.join(dir).join(name),
            &format!(
                "#!/bin/sh\necho \"{name} $*\" >> {}\nexit 1\n",
                sh_quote(format!("{r}/tripwire.log"))
            ),
        )?;
    }
    let scenario = match &opts.default_scenario {
        Some(v) => v.to_string(),
        None => DEFAULT_SCENARIO.to_string(),
    };
    std::fs::write(root.join("fake/scenarios/default.json"), scenario)?;
    FakeGh::init(&root.join("gh")).map_err(std::io::Error::other)?;
    Ok(())
}

fn git_dir() -> Option<String> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .find(|d| d.join("git").is_file())
        .map(|d| d.display().to_string())
}

/// Exactly the variables the daemon gets (research R3); it is started after `env_clear()`.
pub fn daemon_env(root: &Path, port: u16, opts: &EnvOptions) -> BTreeMap<String, String> {
    daemon_env_from(root, port, opts, &|k| std::env::var(k).ok())
}

/// [`daemon_env`] with the host's variables read through `host`.
fn daemon_env_from(
    root: &Path,
    port: u16,
    opts: &EnvOptions,
    host: &dyn Fn(&str) -> Option<String>,
) -> BTreeMap<String, String> {
    let r = root.display().to_string();
    let mut env = BTreeMap::new();
    let mut set = |k: &str, v: String| {
        env.insert(k.to_string(), v);
    };
    set("DELILAH_HOME", format!("{r}/delilah"));
    set("DELILAH_ADDR", format!("127.0.0.1:{port}"));
    set("DELILAH_SECRETS", "memory".into());
    set("DELILAH_JIRA_ALLOW_HTTP_LOOPBACK", "1".into());
    set("DELILAH_ALLOW_FILE_PROTOCOL", "1".into());
    set("DELILAH_NOTIFY", "0".into());
    set("DELILAH_PR_POLL_SECS", "1".into());
    set("SHELL", "/bin/zsh".into());
    if opts.real_claude {
        assert!(
            host("DELILAH_E2E_REAL_CLAUDE").as_deref() == Some("1"),
            "EnvOptions.real_claude runs the real claude with your real HOME: \
             set DELILAH_E2E_REAL_CLAUDE=1 to allow it"
        );
        let home = host("HOME").unwrap_or_default();
        set("HOME", home);
        // Only claude is real: gh is the fake, and a gh tripwire comes first on PATH.
        set(
            "PATH",
            format!(
                "{r}/tripwire-gh:{}",
                std::env::var("PATH").unwrap_or_default()
            ),
        );
        set("DELILAH_GH_BIN", format!("{r}/bin/gh-fake"));
    } else {
        set("HOME", format!("{r}/home"));
        let mut path = format!("{r}/tripwire:/usr/bin:/bin:/usr/sbin:/sbin");
        if let Some(g) = git_dir().filter(|g| !path.split(':').any(|p| p == g)) {
            path = format!("{path}:{g}");
        }
        set("PATH", path);
        if !opts.no_agent_bin {
            set("DELILAH_AGENT_BIN", format!("{r}/bin/claude-fake"));
        }
        set("DELILAH_CLAUDE_BIN", format!("{r}/bin/claude-oneshot"));
        set("DELILAH_GH_BIN", format!("{r}/bin/gh-fake"));
    }
    if let Some(origin) = &opts.ui_origin {
        set("DELILAH_UI_ORIGIN", origin.clone());
    }
    env
}

impl Env {
    /// Builds the layout and fixtures and starts `delilahd` on a free port, retrying with
    /// another port when the address turns out to be in use.
    pub fn start(opts: EnvOptions) -> Env {
        let dir = tempfile::Builder::new()
            .prefix("delilah-e2e-")
            .tempdir()
            .expect("a temp dir");
        let root = dir.path().canonicalize().expect("canonical temp dir");
        layout(&root, &opts).unwrap_or_else(|e| panic!("layout of {}: {e}", root.display()));
        let home = root.join("home");
        let fixtures = Fixtures::make(&root.join("fixtures"), &home)
            .unwrap_or_else(|e| panic!("fixtures: {e}"));
        let gh = FakeGh {
            dir: root.join("gh"),
        };
        let log = root.join("delilahd.log");
        let mut env = Env {
            dir: Some(dir),
            root,
            home,
            api: String::new(),
            port: 0,
            daemon: None,
            jira: None,
            fixtures,
            gh,
            log,
            opts,
            tripwire_reported: false,
        };
        if env.opts.jira {
            env.start_jira();
        }
        env.spawn_daemon();
        env
    }

    /// Starts the fake Jira server (if not running); returns its URL.
    pub fn start_jira(&mut self) -> String {
        if self.jira.is_none() {
            self.jira = Some(crate::jira_server::start());
        }
        self.jira.as_ref().expect("jira").url.clone()
    }

    /// A typed client for this Env's daemon.
    pub fn api(&self) -> crate::Api {
        crate::Api::new(&self.api)
    }

    /// The `delilah` CLI aimed at this Env's daemon.
    pub fn cli(&self) -> crate::Cli {
        crate::Cli::new(&self.home, &self.api)
    }

    /// (Re)starts `delilahd` with this Env's state; panics if it doesn't answer.
    pub fn spawn_daemon(&mut self) {
        let bin = target_dir().join("delilahd");
        assert!(
            bin.is_file(),
            "{} not found: run `cargo build -p delilah-daemon -p delilah-e2e` first",
            bin.display()
        );
        self.spawn_daemon_on(None);
    }

    /// Kills only the daemon child and starts it again on the same home and port
    /// (retrying for a bounded time while the port is still held).
    pub fn restart_daemon(&mut self) {
        self.stop_daemon();
        self.spawn_daemon_on(Some(self.port));
    }

    fn spawn_daemon_on(&mut self, fixed_port: Option<u16>) {
        let bin = target_dir().join("delilahd");
        let attempts = if fixed_port.is_some() {
            20
        } else {
            START_ATTEMPTS
        };
        for attempt in 1..=attempts {
            self.port = fixed_port.unwrap_or_else(free_port);
            self.api = format!("http://127.0.0.1:{}", self.port);
            let log = File::options()
                .create(true)
                .append(true)
                .open(&self.log)
                .expect("daemon log");
            // The log is shared by every start: only this attempt's lines decide a retry.
            let log_start = log.metadata().map_or(0, |m| m.len()) as usize;
            let child = Command::new(&bin)
                .env_clear()
                .envs(daemon_env(&self.root, self.port, &self.opts))
                .current_dir(&self.root)
                .stdin(Stdio::null())
                .stdout(log.try_clone().expect("log handle"))
                .stderr(log)
                .spawn()
                .unwrap_or_else(|e| panic!("start {}: {e}", bin.display()));
            self.daemon = Some(child);
            match self.wait_ready() {
                Ready::Yes => {
                    // Another daemon may have answered on a raced port: ours must be alive.
                    let alive = self
                        .daemon
                        .as_mut()
                        .is_some_and(|c| matches!(c.try_wait(), Ok(None)));
                    if alive {
                        return;
                    }
                    self.daemon = None;
                    if attempt < attempts {
                        continue;
                    }
                    panic!(
                        "delilahd exited right after start\n--- daemon log (tail) ---\n{}",
                        tail_of(&self.log, 40)
                    );
                }
                Ready::Exited(status) => {
                    self.daemon = None;
                    let text = std::fs::read(&self.log).unwrap_or_default();
                    let text = String::from_utf8_lossy(text.get(log_start..).unwrap_or_default());
                    if attempt < attempts && text.to_lowercase().contains("in use") {
                        if fixed_port.is_some() {
                            std::thread::sleep(Duration::from_millis(500));
                        }
                        continue;
                    }
                    panic!(
                        "delilahd exited at start ({status})\n--- daemon log (tail) ---\n{}",
                        tail_of(&self.log, 40)
                    );
                }
            }
        }
    }

    fn wait_ready(&mut self) -> Ready {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .expect("http client");
        let url = format!("{}/api/v1/health", self.api);
        let log = self.log.clone();
        let child = self.daemon.as_mut().expect("daemon");
        wait_for(
            "the daemon's /api/v1/health",
            DEFAULT_TIMEOUT,
            Some(&log),
            || {
                if let Ok(Some(status)) = child.try_wait() {
                    return Ok(Ready::Exited(status.to_string()));
                }
                match client.get(&url).send() {
                    Ok(r) if r.status().is_success() => Ok(Ready::Yes),
                    Ok(r) => Err(format!("HTTP {}", r.status())),
                    Err(e) => Err(e.to_string()),
                }
            },
        )
    }

    /// Stops the daemon by its handle (leaves everything else as it is).
    pub fn stop_daemon(&mut self) {
        if let Some(mut child) = self.daemon.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Polls `f` until `Some`, panicking with `what` and the daemon log tail.
    pub fn wait_until<T: Debug>(
        &self,
        what: &str,
        timeout: Duration,
        mut f: impl FnMut() -> Option<T>,
    ) -> T {
        wait_for(what, timeout, Some(&self.log), || f().ok_or(None::<()>))
    }

    /// Like [`Env::wait_until`], reporting the state `f` saw while not ready.
    pub fn wait_for<T, S: Debug>(
        &self,
        what: &str,
        timeout: Duration,
        f: impl FnMut() -> Result<T, S>,
    ) -> T {
        wait_for(what, timeout, Some(&self.log), f)
    }

    pub fn fake_dir(&self) -> PathBuf {
        self.root.join("fake")
    }

    /// Writes `fake/scenarios/<name>.json` (`name` is a session title marker or a task key).
    pub fn scenario(&self, name: &str, json: &Value) {
        assert!(
            crate::fake::scenario::valid_name(name),
            "bad scenario name {name:?}"
        );
        let path = self
            .fake_dir()
            .join("scenarios")
            .join(format!("{name}.json"));
        std::fs::write(&path, json.to_string())
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    }

    /// The record of a fake session, as written so far.
    pub fn record(&self, session: &str) -> Result<Record, String> {
        Record::load(&self.fake_dir(), session)
    }

    /// The most recent record of a task's fake session.
    pub fn record_for(&self, task: &str) -> Option<Record> {
        Record::for_task(&self.fake_dir(), task).ok().flatten()
    }

    /// Registers `fixtures.repo` as a repo project and `fixtures.parent` as a parent
    /// project; returns their names `(repo, parent)`.
    pub fn register_fixtures(&self) -> (String, String) {
        let client = reqwest::blocking::Client::new();
        let add = |path: &Path, kind: &str, name: &str| {
            let r = client
                .post(format!("{}/api/v1/projects", self.api))
                .json(&json!({"path": path, "kind": kind, "name": name}))
                .send()
                .unwrap_or_else(|e| panic!("add project {name}: {e}"));
            let status = r.status();
            let body = r.text().unwrap_or_default();
            assert!(status.is_success(), "add project {name}: {status} {body}");
            serde_json::from_str::<Value>(&body).expect("project JSON")["name"]
                .as_str()
                .unwrap_or(name)
                .to_string()
        };
        let repo = add(&self.fixtures.repo, "repo", "signal");
        let parent = add(&self.fixtures.parent, "parent", "parent");
        (repo, parent)
    }

    /// The last `n` lines of the daemon's log.
    pub fn log_tail(&self, n: usize) -> String {
        tail_of(&self.log, n)
    }

    /// Keeps `root` after this Env is gone; returns its path (for `e2e-env --keep`).
    pub fn keep_root(&mut self) -> Option<PathBuf> {
        self.dir.take().map(tempfile::TempDir::keep)
    }

    /// Stops everything and says whether the tripwire fired, without panicking.
    pub fn finish(mut self) -> bool {
        let fired = self.tripwire_log().is_some();
        if fired {
            eprintln!(
                "the real claude/gh was run (tripwire.log):\n{}",
                self.tripwire_log().unwrap_or_default()
            );
        }
        self.tripwire_reported = true;
        fired
    }

    /// What the tripwire caught, if anything.
    pub fn tripwire_log(&self) -> Option<String> {
        std::fs::read_to_string(self.root.join("tripwire.log")).ok()
    }
}

enum Ready {
    Yes,
    Exited(String),
}

impl Drop for Env {
    fn drop(&mut self) {
        self.stop_daemon();
        if let Some(mut jira) = self.jira.take()
            && let Some(stop) = jira.stop.take()
        {
            stop();
        }
        let panicking = std::thread::panicking();
        if panicking
            && std::env::var("DELILAH_E2E_KEEP").as_deref() == Ok("1")
            && let Some(dir) = self.dir.take()
        {
            let kept = dir.keep();
            eprintln!("DELILAH_E2E_KEEP: kept {}", kept.display());
        }
        if !self.tripwire_reported
            && let Some(text) = self.tripwire_log()
        {
            let msg = format!("the real claude/gh was run (tripwire.log):\n{text}");
            if panicking {
                eprintln!("{msg}");
            } else {
                panic!("{msg}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_port_skips_the_installed_app_and_previews() {
        let mut picks = vec![7420, 7501, 7505, 7509, 7510, 7420].into_iter();
        assert_eq!(free_port_with(|| picks.next().unwrap()), 7510);
        for _ in 0..20 {
            let p = free_port();
            assert!(p != 7420 && !(7501..=7509).contains(&p));
        }
    }

    #[test]
    fn layout_creates_the_tree() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        layout(root, &EnvOptions::default()).unwrap();
        let r = root.display().to_string();

        let zshrc = std::fs::read_to_string(root.join("home/.zshrc")).unwrap();
        assert!(zshrc.contains(&format!("{r}/tripwire")) && zshrc.contains("PATH"));
        let gitconfig = std::fs::read_to_string(root.join("home/.gitconfig")).unwrap();
        for want in [
            "name = Delilah E2E",
            "email = e2e@delilah.invalid",
            "allow = always",
        ] {
            assert!(gitconfig.contains(want), "{gitconfig}");
        }
        assert!(root.join("home/.claude").is_dir() && root.join("delilah").is_dir());
        assert!(root.join("fake/records").is_dir() && root.join("fixtures").is_dir());

        let exec = |p: &str| {
            std::fs::metadata(root.join(p))
                .unwrap()
                .permissions()
                .mode()
                & 0o111
                != 0
        };
        for p in [
            "bin/claude-fake",
            "bin/gh-fake",
            "tripwire/claude",
            "tripwire/gh",
        ] {
            assert!(exec(p), "{p} is not executable");
        }
        let claude = std::fs::read_to_string(root.join("bin/claude-fake")).unwrap();
        assert!(
            claude.contains("fake-claude") && claude.contains(&format!("--fake-dir '{r}/fake'"))
        );
        assert!(claude.contains("\"$@\""));
        let gh = std::fs::read_to_string(root.join("bin/gh-fake")).unwrap();
        assert!(gh.contains(&format!("FAKE_GH_DIR='{r}/gh'")) && gh.contains("scripts/fake-gh.sh"));

        let scenario = std::fs::read_to_string(root.join("fake/scenarios/default.json")).unwrap();
        let parsed = crate::fake::scenario::Scenario::parse(&scenario).unwrap();
        assert_eq!(parsed.steps.len(), 1);
        assert!(root.join("gh/repo.json").is_file() && root.join("gh/user.json").is_file());
    }

    #[test]
    fn the_tripwire_scripts_log_and_fail() {
        let d = tempfile::tempdir().unwrap();
        layout(d.path(), &EnvOptions::default()).unwrap();
        let st = Command::new(d.path().join("tripwire/claude"))
            .args(["--print", "hi"])
            .status()
            .unwrap();
        assert!(!st.success());
        let log = std::fs::read_to_string(d.path().join("tripwire.log")).unwrap();
        assert_eq!(log, "claude --print hi\n");
    }

    #[test]
    fn daemon_env_is_exactly_the_r3_table() {
        let root = Path::new("/tmp/x");
        let env = daemon_env(root, 9999, &EnvOptions::default());
        let keys: Vec<&str> = env.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            [
                "DELILAH_ADDR",
                "DELILAH_AGENT_BIN",
                "DELILAH_ALLOW_FILE_PROTOCOL",
                "DELILAH_CLAUDE_BIN",
                "DELILAH_GH_BIN",
                "DELILAH_HOME",
                "DELILAH_JIRA_ALLOW_HTTP_LOOPBACK",
                "DELILAH_NOTIFY",
                "DELILAH_PR_POLL_SECS",
                "DELILAH_SECRETS",
                "HOME",
                "PATH",
                "SHELL"
            ]
        );
        assert_eq!(env["HOME"], "/tmp/x/home");
        assert_eq!(env["SHELL"], "/bin/zsh");
        assert!(env["PATH"].starts_with("/tmp/x/tripwire:/usr/bin:/bin:/usr/sbin:/sbin"));
        assert_eq!(env["DELILAH_HOME"], "/tmp/x/delilah");
        assert_eq!(env["DELILAH_ADDR"], "127.0.0.1:9999");
        assert_eq!(env["DELILAH_AGENT_BIN"], "/tmp/x/bin/claude-fake");
        assert_eq!(env["DELILAH_CLAUDE_BIN"], "/tmp/x/bin/claude-oneshot");
        assert_eq!(env["DELILAH_GH_BIN"], "/tmp/x/bin/gh-fake");
        assert_eq!(env["DELILAH_SECRETS"], "memory");
        assert_eq!(env["DELILAH_JIRA_ALLOW_HTTP_LOOPBACK"], "1");
        assert_eq!(env["DELILAH_ALLOW_FILE_PROTOCOL"], "1");
        assert_eq!(env["DELILAH_NOTIFY"], "0");
        assert_eq!(env["DELILAH_PR_POLL_SECS"], "1");
        assert!(
            !env.keys().any(|k| k == "DELILAH_URL"
                || k.starts_with("CLAUDE")
                || k.starts_with("ANTHROPIC"))
        );
    }

    #[test]
    fn real_claude_needs_the_opt_in_and_keeps_gh_fake_and_tripwired() {
        let opts = EnvOptions {
            real_claude: true,
            ..Default::default()
        };
        let root = Path::new("/tmp/x");
        let without = |k: &str| (k == "PATH").then(|| "/usr/bin".to_string());
        let r = std::panic::catch_unwind(|| daemon_env_from(root, 9999, &opts, &without));
        assert!(r.is_err(), "no opt-in must panic");
        let with = |k: &str| match k {
            "DELILAH_E2E_REAL_CLAUDE" => Some("1".to_string()),
            "HOME" => Some("/Users/me".to_string()),
            "PATH" => Some("/usr/bin".to_string()),
            _ => None,
        };
        let env = daemon_env_from(root, 9999, &opts, &with);
        assert_eq!(env["HOME"], "/Users/me");
        assert_eq!(env["DELILAH_GH_BIN"], "/tmp/x/bin/gh-fake");
        assert!(
            env["PATH"].starts_with("/tmp/x/tripwire-gh:"),
            "{}",
            env["PATH"]
        );
        assert!(!env.contains_key("DELILAH_AGENT_BIN"));
    }

    #[test]
    fn sh_quote_survives_a_quote_in_the_path() {
        assert_eq!(sh_quote("/a b/c"), "'/a b/c'");
        assert_eq!(sh_quote("/it's"), "'/it'\\''s'");
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("it's here");
        std::fs::create_dir_all(&root).unwrap();
        layout(&root, &EnvOptions::default()).unwrap();
        let st = Command::new(root.join("tripwire/claude"))
            .arg("x")
            .status()
            .unwrap();
        assert!(!st.success());
        assert_eq!(
            std::fs::read_to_string(root.join("tripwire.log")).unwrap(),
            "claude x\n"
        );
    }

    #[test]
    fn scenario_names_allow_one_leading_at() {
        assert!(crate::fake::scenario::valid_name("@signal"));
        assert!(!crate::fake::scenario::valid_name("@@x"));
    }

    #[test]
    fn daemon_env_adds_the_ui_origin_only_when_asked() {
        let opts = EnvOptions {
            ui_origin: Some("http://localhost:1500".into()),
            ..Default::default()
        };
        let env = daemon_env(Path::new("/tmp/x"), 9999, &opts);
        assert_eq!(env["DELILAH_UI_ORIGIN"], "http://localhost:1500");
    }
}

/// Needs `cargo build -p delilah-daemon -p delilah-e2e` first.
#[cfg(all(test, feature = "e2e"))]
mod e2e_tests {
    use super::*;

    fn health(env: &Env) -> Value {
        reqwest::blocking::get(format!("{}/api/v1/health", env.api))
            .unwrap()
            .json()
            .unwrap()
    }

    #[test]
    fn starts_a_daemon_on_its_own_port_and_cleans_up() {
        let env = Env::start(EnvOptions::default());
        assert!(env.port != 7420 && !(7501..=7509).contains(&env.port));
        assert!(health(&env)["version"].is_string());
        let (repo, parent) = env.register_fixtures();
        assert_eq!((repo.as_str(), parent.as_str()), ("signal", "parent"));
        let projects: Value = reqwest::blocking::get(format!("{}/api/v1/projects", env.api))
            .unwrap()
            .json()
            .unwrap();
        assert_eq!(projects.as_array().unwrap().len(), 2);
        let (root, port) = (env.root.clone(), env.port);
        drop(env);
        assert!(!root.exists());
        assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_err());
    }

    #[test]
    fn two_envs_at_once_do_not_interfere() {
        let (a, b) = (
            Env::start(EnvOptions::default()),
            Env::start(EnvOptions::default()),
        );
        assert_ne!(a.port, b.port);
        assert_ne!(a.root, b.root);
        assert!(health(&a).is_object() && health(&b).is_object());
    }

    #[test]
    fn the_tripwire_fails_the_test_on_drop() {
        let env = Env::start(EnvOptions::default());
        let root = env.root.clone();
        let _ = Command::new(root.join("tripwire/claude")).status();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || drop(env)));
        let err = r.expect_err("drop should panic");
        let msg = err.downcast_ref::<String>().cloned().unwrap_or_default();
        assert!(msg.contains("the real claude/gh was run"), "{msg}");
    }
}
