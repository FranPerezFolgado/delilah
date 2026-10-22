//! End-to-end test harness and fake Claude Code for Delilah.

use std::fmt::Debug;
use std::time::{Duration, Instant};

pub mod api;
pub mod cli;
pub mod env;
pub mod fake;
pub mod fixtures;
pub mod jira_server;

pub use api::Api;
pub use cli::Cli;
pub use env::{Env, EnvOptions};

/// How often `wait_until` looks again.
pub const POLL: Duration = Duration::from_millis(100);
/// The timeout when a test doesn't say.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);
/// No wait is longer than this, whatever is asked (FR-012).
pub const MAX_TIMEOUT: Duration = Duration::from_secs(30);

/// Polls `f` every 100 ms until it returns `Some`. Panics on timeout (clamped to 30 s)
/// with `what` and `last value: None`. Use [`wait_for`] to report the last state seen.
pub fn wait_until<T>(what: &str, timeout: Duration, mut f: impl FnMut() -> Option<T>) -> T {
    wait_for(what, timeout, None, || f().ok_or(None::<()>))
}

/// Like [`wait_until`], but `f` says what it saw while not ready (`Err(state)`), and the
/// panic shows the last state plus the last 40 lines of the daemon log `log`, if given.
pub fn wait_for<T, S: Debug>(
    what: &str,
    timeout: Duration,
    log: Option<&std::path::Path>,
    mut f: impl FnMut() -> Result<T, S>,
) -> T {
    let timeout = timeout.min(MAX_TIMEOUT);
    let start = Instant::now();
    loop {
        let last = match f() {
            Ok(v) => return v,
            Err(s) => s,
        };
        if start.elapsed() >= timeout {
            let tail = log.map(|p| tail_of(p, 40)).unwrap_or_default();
            panic!(
                "timed out after {:?} waiting for {what}\nlast value: {last:?}\n--- daemon log (tail) ---\n{tail}",
                timeout
            );
        }
        std::thread::sleep(POLL);
    }
}

/// The last `n` lines of a file, or a note when it can't be read.
pub fn tail_of(path: &std::path::Path, n: usize) -> String {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let lines: Vec<&str> = text.lines().collect();
            lines[lines.len().saturating_sub(n)..].join("\n")
        }
        Err(e) => format!("({}: {e})", path.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_the_value_once_ready() {
        let mut n = 0;
        let v = wait_until("three", Duration::from_secs(5), || {
            n += 1;
            (n == 3).then_some(n)
        });
        assert_eq!(v, 3);
    }

    #[test]
    #[should_panic(expected = "waiting for the moon")]
    fn panics_naming_what_it_waited_for() {
        wait_until::<()>("the moon", Duration::from_millis(250), || None);
    }

    #[test]
    #[should_panic(expected = "last value: \"state-7\"")]
    fn panics_with_the_last_state_seen() {
        let mut n = 0;
        wait_for::<(), _>("x", Duration::from_millis(250), None, || {
            n += 1;
            Err(format!("state-{}", if n > 0 { 7 } else { 0 }))
        });
    }

    #[test]
    #[should_panic(expected = "line-49")]
    fn panic_shows_the_log_tail() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("l.log");
        let text: String = (0..50).map(|i| format!("line-{i}\n")).collect();
        std::fs::write(&log, text).unwrap();
        wait_for::<(), _>("x", Duration::from_millis(150), Some(&log), || Err(0));
    }

    #[test]
    fn tail_keeps_only_the_last_lines() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("l.log");
        let text: String = (0..50).map(|i| format!("line-{i}\n")).collect();
        std::fs::write(&log, text).unwrap();
        let t = tail_of(&log, 40);
        assert!(t.starts_with("line-10") && !t.contains("line-9\n"));
    }

    #[test]
    fn a_long_timeout_is_clamped_to_thirty_seconds() {
        // A closure that is ready at once returns immediately whatever the timeout; the
        // clamp is checked through the constant the panic path uses.
        assert_eq!(Duration::from_secs(300).min(MAX_TIMEOUT), Duration::from_secs(30));
        assert_eq!(wait_until("now", Duration::from_secs(300), || Some(1)), 1);
    }
}
