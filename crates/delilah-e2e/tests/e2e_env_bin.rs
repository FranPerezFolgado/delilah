//! Spawns the built `e2e-env`, uses its daemon, closes stdin and checks it cleans up.

use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

fn spawn(args: &[&str]) -> (std::process::Child, Value) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_e2e-env"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn e2e-env");
    let mut line = String::new();
    BufReader::new(child.stdout.as_mut().unwrap()).read_line(&mut line).unwrap();
    let info: Value = serde_json::from_str(&line).unwrap_or_else(|e| panic!("{e}: {line:?}"));
    (child, info)
}

fn wait_exit(child: &mut std::process::Child) -> std::process::ExitStatus {
    let start = Instant::now();
    loop {
        if let Some(st) = child.try_wait().unwrap() {
            return st;
        }
        assert!(start.elapsed() < Duration::from_secs(20), "e2e-env did not exit");
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn port_of(api: &str) -> u16 {
    api.rsplit(':').next().unwrap().parse().unwrap()
}

#[test]
fn start_serves_a_daemon_and_stops_when_stdin_closes() {
    let (mut child, info) = spawn(&["start", "--jira"]);
    let api = info["api"].as_str().unwrap().to_string();
    let root = std::path::PathBuf::from(info["root"].as_str().unwrap());
    assert!(info["jira"].is_string() && info["fixtures"]["repo"].is_string());
    assert!(info["fixtures"]["parent"].is_string() && root.is_dir());
    let health = reqwest::blocking::get(format!("{api}/api/v1/health")).unwrap();
    assert!(health.status().is_success());

    drop(child.stdin.take());
    assert!(wait_exit(&mut child).success());
    assert!(TcpStream::connect(("127.0.0.1", port_of(&api))).is_err());
    assert!(!root.exists());
}

#[test]
fn exits_3_when_the_tripwire_fired() {
    let (mut child, info) = spawn(&["start"]);
    let root = std::path::PathBuf::from(info["root"].as_str().unwrap());
    let _ = Command::new(root.join("tripwire/claude")).status();
    drop(child.stdin.take());
    assert_eq!(wait_exit(&mut child).code(), Some(3));
}

#[test]
fn keep_leaves_the_root_and_fixture_makes_a_set() {
    let (mut child, info) = spawn(&["start", "--keep"]);
    let root = std::path::PathBuf::from(info["root"].as_str().unwrap());
    drop(child.stdin.take());
    assert!(wait_exit(&mut child).success());
    assert!(root.is_dir());
    std::fs::remove_dir_all(&root).unwrap();

    let d = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_e2e-env"))
        .args(["fixture", "--into"])
        .arg(d.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(std::path::Path::new(v["repo"].as_str().unwrap()).join(".git").exists());
    assert!(std::path::Path::new(v["parent"].as_str().unwrap()).is_dir());
}
