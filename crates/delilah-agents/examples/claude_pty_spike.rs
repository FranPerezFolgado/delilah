//! Spike: runs Claude Code in a PTY with hooks passed through `--settings` (nothing is
//! written into the repo), sends one prompt, waits for the turn to end and prints what
//! the hooks and the status line reported. Then checks `--resume` with the same id.
//!
//! Usage: cargo run -p delilah-agents --example claude_pty_spike -- <workspace-dir> <out-dir>

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde_json::{Value, json};

const HOOK_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "Notification",
    "Stop",
    "StopFailure",
    "SessionEnd",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let [_, workspace, out] = args.as_slice() else {
        return Err("usage: claude_pty_spike <workspace-dir> <out-dir>".into());
    };
    let workspace = PathBuf::from(workspace);
    let out = PathBuf::from(out);
    std::fs::create_dir_all(&out)?;

    let hooks_log = out.join("hooks.jsonl");
    let status_log = out.join("statusline.jsonl");
    let settings = out.join("claude-settings.json");
    write_settings(&settings, &hooks_log, &status_log)?;

    let session_id = uuid::Uuid::new_v4().to_string();
    println!("session id: {session_id}");

    let pty = native_pty_system().openpty(PtySize {
        rows: 40,
        cols: 120,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let mut cmd = CommandBuilder::new("claude");
    cmd.args([
        "--session-id",
        &session_id,
        "--settings",
        &settings.to_string_lossy(),
    ]);
    cmd.cwd(&workspace);
    let mut child = pty.slave.spawn_command(cmd)?;
    drop(pty.slave);

    let screen = Arc::new(Mutex::new(Vec::<u8>::new()));
    let mut reader = pty.master.try_clone_reader()?;
    let sink = Arc::clone(&screen);
    let transcript = out.join("pty.raw");
    thread::spawn(move || {
        let mut file = std::fs::File::create(transcript).ok();
        let mut buf = [0u8; 4096];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            sink.lock().unwrap().extend_from_slice(&buf[..n]);
            if let Some(f) = file.as_mut() {
                let _ = f.write_all(&buf[..n]);
            }
        }
    });
    let mut writer = pty.master.take_writer()?;

    // A fresh folder shows the trust dialog before anything else; accept it once.
    let started = wait_for(Duration::from_secs(30), || {
        let text = String::from_utf8_lossy(&screen.lock().unwrap()).to_lowercase();
        if text.contains("trust") && !text.contains("delilah-trust-sent") {
            return Some(true);
        }
        hook_seen(&hooks_log, "SessionStart").then_some(false)
    });
    if started == Some(true) {
        println!("trust dialog shown: accepting");
        // Claude 2.1.292 opens it on "No, exit": let it settle, move down, then Enter.
        for _ in 0..3 {
            std::thread::sleep(Duration::from_millis(600));
            let yes = delilah_agents::claude::trust_dialog_selection(&screen.lock().unwrap());
            if yes == Some(true) {
                break;
            }
            writer.write_all(delilah_agents::claude::DOWN)?;
        }
        writer.write_all(delilah_agents::claude::ENTER)?;
        screen
            .lock()
            .unwrap()
            .extend_from_slice(b"delilah-trust-sent");
    }
    wait_for(Duration::from_secs(30), || {
        hook_seen(&hooks_log, "SessionStart").then_some(())
    })
    .ok_or("no SessionStart hook within 30 s")?;
    thread::sleep(Duration::from_millis(800));

    writer.write_all("Responde solo con la palabra: hola".as_bytes())?;
    thread::sleep(Duration::from_millis(300));
    writer.write_all(b"\r")?;
    let ended = wait_for(Duration::from_secs(120), || {
        ["Stop", "StopFailure"]
            .into_iter()
            .find(|e| hook_seen(&hooks_log, e))
    })
    .ok_or("the turn did not end within 120 s")?;
    println!("turn ended with: {ended}");

    // Give the status line a moment to report, then leave.
    thread::sleep(Duration::from_secs(2));
    writer.write_all(b"/exit\r")?;
    if wait_for(Duration::from_secs(10), || child.try_wait().ok().flatten()).is_none() {
        child.kill()?;
    }

    println!("\nhooks received:");
    for event in read_jsonl(&hooks_log) {
        println!(
            "  {:<17} session={} {}",
            event["hook_event_name"].as_str().unwrap_or("?"),
            event["session_id"].as_str().map(|s| &s[..8]).unwrap_or("-"),
            event.get("tool_name").and_then(Value::as_str).unwrap_or("")
        );
    }
    match read_jsonl(&status_log).last() {
        Some(last) => println!(
            "\nstatus line rate_limits: {}",
            last.get("rate_limits").unwrap_or(&Value::Null)
        ),
        None => println!("\nstatus line: no payload received"),
    }

    println!("\nresume check (claude --resume {session_id} -p ...):");
    let resumed = Command::new("claude")
        .args([
            "--resume",
            &session_id,
            "-p",
            "¿Qué palabra respondiste en tu mensaje anterior? Responde solo esa palabra.",
        ])
        .current_dir(&workspace)
        .output()?;
    println!("  {}", String::from_utf8_lossy(&resumed.stdout).trim());
    Ok(())
}

fn write_settings(path: &Path, hooks_log: &Path, status_log: &Path) -> std::io::Result<()> {
    let append = |log: &Path| format!("{{ cat; echo; }} >> '{}'", log.display());
    let hooks: serde_json::Map<String, Value> = HOOK_EVENTS
        .iter()
        .map(|event| {
            let entry = json!([{ "matcher": "*", "hooks": [{ "type": "command", "command": append(hooks_log) }] }]);
            (event.to_string(), entry)
        })
        .collect();
    let settings = json!({
        "hooks": hooks,
        "statusLine": { "type": "command", "command": format!("{}; echo delilah", append(status_log)) }
    });
    std::fs::write(path, serde_json::to_vec_pretty(&settings)?)
}

fn hook_seen(log: &Path, event: &str) -> bool {
    read_jsonl(log)
        .iter()
        .any(|e| e["hook_event_name"] == event)
}

fn read_jsonl(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn wait_for<T>(timeout: Duration, mut check: impl FnMut() -> Option<T>) -> Option<T> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Some(value) = check() {
            return Some(value);
        }
        thread::sleep(Duration::from_millis(200));
    }
    None
}
