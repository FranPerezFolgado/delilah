//! Fake Claude Code: same arguments, same hooks, scripted behaviour
//! (contracts/fake-claude.md). Never touches the network or the user's config.

use std::io::{BufReader, Write};
use std::process::exit;

use delilah_e2e::fake::args::{FakeArgs, Mode};
use delilah_e2e::fake::hooks::{payload, Hooks};
use delilah_e2e::fake::record::Record;
use delilah_e2e::fake::{run, scenario};

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = match FakeArgs::parse(argv.clone()) {
        Ok(a) => a,
        Err(e) => die(&e),
    };
    let Some(fake_dir) = args.fake_dir.clone() else {
        die("--fake-dir is required");
    };
    let hooks = match Hooks::load(&args.settings) {
        Ok(h) => h,
        Err(e) => die(&e),
    };
    let cwd = std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default();
    let task = hooks.task();
    let (name, scenario) = match scenario::select(&fake_dir, args.prompt.as_deref(), task.as_deref()) {
        Ok(s) => s,
        Err(e) => {
            // Hooks are loaded: let the daemon see a failed session, not a hang.
            hooks.run("StopFailure", &payload::stop_failure(&args.session_id, &cwd, &e));
            die(&e)
        }
    };
    let mut record = Record {
        args: argv,
        mode: match args.mode {
            Mode::New => "new",
            Mode::Resume => "resume",
        }
        .to_string(),
        session_id: args.session_id.clone(),
        task,
        cwd,
        env: std::env::vars().collect(),
        settings: args.settings.display().to_string(),
        prompt: args.prompt.clone(),
        scenario: name,
        steps_done: 0,
        hooks: vec![],
        answers: vec![],
        inputs: vec![],
        exited: None,
    };
    let mut input = BufReader::new(std::io::stdin());
    let mut out = std::io::stdout();
    let code = run::run(&mut record, &scenario, &hooks, Some(&fake_dir), &mut input, &mut out);
    let _ = out.flush();
    exit(code);
}

fn die(message: &str) -> ! {
    eprintln!("fake-claude: {message}");
    exit(2);
}
