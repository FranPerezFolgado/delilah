//! `e2e-env start|fixture`: an isolated daemon for the UI layer, and fixtures by hand.
//! See specs/005-e2e-tests/contracts/e2e-env.md.

use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use delilah_e2e::fixtures::Fixtures;
use delilah_e2e::{Env, EnvOptions};
use serde_json::json;
use tokio::signal::unix::{SignalKind, signal};

const USAGE: &str = "usage: e2e-env start [--ui-origin <url>] [--jira] [--keep]\n       e2e-env fixture --into <dir>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("e2e-env: {e}");
            ExitCode::from(1)
        }
    }
}

fn run(args: &[String]) -> Result<u8, String> {
    match args.first().map(String::as_str) {
        Some("start") => start(&args[1..]),
        Some("fixture") => fixture(&args[1..]),
        _ => Err(USAGE.into()),
    }
}

fn start(args: &[String]) -> Result<u8, String> {
    let mut opts = EnvOptions::default();
    let mut keep = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--ui-origin" => {
                opts.ui_origin = Some(it.next().ok_or("--ui-origin needs a value")?.clone())
            }
            "--jira" => opts.jira = true,
            "--keep" => keep = true,
            other => return Err(format!("unknown argument {other:?}\n{USAGE}")),
        }
    }
    let mut env = std::panic::catch_unwind(|| Env::start(opts))
        .map_err(|_| "could not start the environment (see above)".to_string())?;
    if keep {
        env.keep_root();
    }
    let line = json!({
        "api": env.api,
        "root": env.root,
        "home": env.root.join("delilah"),
        "log": env.log,
        "fake_dir": env.fake_dir(),
        "gh_dir": env.gh.dir,
        "jira": env.jira.as_ref().map(|j| j.url.clone()),
        "fixtures": {"repo": env.fixtures.repo, "parent": env.fixtures.parent},
    });
    println!("{line}");

    wait_for_stop()?;
    Ok(if env.finish() { 3 } else { 0 })
}

/// Blocks until stdin hits EOF or SIGINT/SIGTERM arrives.
fn wait_for_stop() -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    rt.block_on(async {
        let mut term = signal(SignalKind::terminate()).map_err(|e| e.to_string())?;
        let mut int = signal(SignalKind::interrupt()).map_err(|e| e.to_string())?;
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        std::thread::spawn(move || {
            let mut sink = Vec::new();
            let _ = std::io::stdin().read_to_end(&mut sink);
            let _ = tx.send(());
        });
        tokio::select! {
            _ = rx => {}
            _ = term.recv() => {}
            _ = int.recv() => {}
        }
        Ok(())
    })
}

fn fixture(args: &[String]) -> Result<u8, String> {
    let into = match args {
        [flag, dir] if flag == "--into" => PathBuf::from(dir),
        _ => return Err(USAGE.into()),
    };
    std::fs::create_dir_all(&into).map_err(|e| format!("{}: {e}", into.display()))?;
    let into = into.canonicalize().map_err(|e| e.to_string())?;
    // The fixtures need a HOME with a git identity, like an Env's.
    let home = into.join("home");
    std::fs::create_dir_all(&home).map_err(|e| e.to_string())?;
    std::fs::write(
        home.join(".gitconfig"),
        "[user]\n\tname = Delilah E2E\n\temail = e2e@delilah.invalid\n\
         [protocol \"file\"]\n\tallow = always\n[commit]\n\tgpgsign = false\n",
    )
    .map_err(|e| e.to_string())?;
    let f = Fixtures::make(&into.join("fixtures"), &home)?;
    println!("{}", json!({"repo": f.repo, "parent": f.parent}));
    Ok(0)
}
