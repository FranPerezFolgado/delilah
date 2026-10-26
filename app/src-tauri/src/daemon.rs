//! Keeps `delilahd` running, the way Orca keeps its daemon: as a per-user LaunchAgent.
//! launchd starts it at login and restarts it if it crashes, and it outlives the
//! window, so agents keep working with the app closed. The app only installs the
//! agent, points it at the `delilahd` inside the bundle, and restarts it after an
//! update. Debug builds leave it alone: there the daemon comes from
//! `scripts/dev-daemon.sh`.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, UNIX_EPOCH};

pub const LABEL: &str = "dev.delilah.daemon";
const ADDR: &str = "127.0.0.1:7420";

/// Installs or refreshes the LaunchAgent and waits (briefly) for the daemon to answer.
/// Runs off the main thread: the window opens at once and shows "sin conexión" until
/// the daemon is up.
pub fn ensure_running() -> Result<(), String> {
    let bundled = bundled_daemon().ok_or("no delilahd next to the app binary")?;
    let plist_path = plist_path()?;
    let plist = plist(&bundled)?;
    let installed = std::fs::read_to_string(&plist_path).ok();
    if installed.as_deref() != Some(plist.as_str()) {
        // New install, or the app moved: (re)load the agent with the new definition.
        std::fs::create_dir_all(plist_path.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&plist_path, &plist).map_err(|e| e.to_string())?;
        let _ = launchctl(&["bootout", &service()]);
        launchctl(&["bootstrap", &domain(), &plist_path.display().to_string()])?;
    } else if launchctl(&["print", &service()]).is_err() {
        // Installed but not loaded, e.g. after "Detener el daemon".
        launchctl(&["bootstrap", &domain(), &plist_path.display().to_string()])?;
    } else if let Some(running) = health() {
        // Loaded and answering: restart it if the app brought a different binary.
        if running.exe_mtime.is_some() && running.exe_mtime != mtime(&bundled) {
            restart()?;
        }
    }
    wait_for_health(Duration::from_secs(15))
}

pub fn restart() -> Result<(), String> {
    launchctl(&["kickstart", "-k", &service()])
}

/// Stops the daemon until the app is opened again. Agents stop with it.
pub fn stop() -> Result<(), String> {
    launchctl(&["bootout", &service()])
}

fn bundled_daemon() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let daemon = exe.parent()?.join("delilahd");
    daemon.is_file().then_some(daemon)
}

fn plist_path() -> Result<PathBuf, String> {
    Ok(home()?.join(format!("Library/LaunchAgents/{LABEL}.plist")))
}

fn home() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not set".into())
}

fn plist(daemon: &Path) -> Result<String, String> {
    let logs = home()?.join("Library/Logs/Delilah");
    std::fs::create_dir_all(&logs).map_err(|e| e.to_string())?;
    let log = logs.join("delilahd.log");
    // launchd gives its agents almost no environment. The daemon reads PATH from the
    // login shell itself; the shell and the locale have to be given here.
    let env = [
        ("LANG", locale()),
        ("SHELL", login_shell()),
        ("NO_COLOR", "1".into()),
    ];
    let env: String = env
        .iter()
        .map(|(k, v)| format!("\t\t<key>{k}</key>\n\t\t<string>{}</string>\n", xml(v)))
        .collect();
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{LABEL}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{daemon}</string>
	</array>
	<key>EnvironmentVariables</key>
	<dict>
{env}	</dict>
	<key>RunAtLoad</key>
	<true/>
	<key>KeepAlive</key>
	<dict>
		<key>SuccessfulExit</key>
		<false/>
	</dict>
	<key>ProcessType</key>
	<string>Interactive</string>
	<key>StandardOutPath</key>
	<string>{log}</string>
	<key>StandardErrorPath</key>
	<string>{log}</string>
</dict>
</plist>
"#,
        daemon = xml(&daemon.display().to_string()),
        log = xml(&log.display().to_string()),
    ))
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The user's language as a UTF-8 locale, e.g. `es_ES.UTF-8`; `en_US.UTF-8` if macOS
/// has no such locale.
fn locale() -> String {
    let fallback = "en_US.UTF-8".to_string();
    let Some(apple) = output("defaults", &["read", "-g", "AppleLocale"]) else {
        return fallback;
    };
    let base = apple.split('@').next().unwrap_or_default().trim();
    let candidate = format!("{base}.UTF-8");
    if Path::new("/usr/share/locale").join(&candidate).is_dir() {
        candidate
    } else {
        fallback
    }
}

fn login_shell() -> String {
    if let Ok(shell) = std::env::var("SHELL")
        && !shell.is_empty()
    {
        return shell;
    }
    let user = std::env::var("USER").unwrap_or_default();
    output(
        "dscl",
        &[".", "-read", &format!("/Users/{user}"), "UserShell"],
    )
    .and_then(|out| out.split_whitespace().last().map(str::to_string))
    .unwrap_or_else(|| "/bin/zsh".into())
}

fn domain() -> String {
    let uid = output("id", &["-u"]).unwrap_or_else(|| "501".into());
    format!("gui/{}", uid.trim())
}

fn service() -> String {
    format!("{}/{LABEL}", domain())
}

fn launchctl(args: &[&str]) -> Result<(), String> {
    let out = Command::new("/bin/launchctl")
        .args(args)
        .output()
        .map_err(|e| format!("launchctl: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "launchctl {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

fn output(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn mtime(path: &Path) -> Option<u64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(modified.duration_since(UNIX_EPOCH).ok()?.as_secs())
}

#[derive(serde::Deserialize)]
struct Health {
    exe_mtime: Option<u64>,
}

/// A GET to the daemon with a bare socket: tiny requests are not worth an HTTP client.
fn get<T: serde::de::DeserializeOwned>(path: &str) -> Option<T> {
    let addr: SocketAddr = ADDR.parse().ok()?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(500)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    let request = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    if !response.starts_with("HTTP/1.1 200") {
        return None;
    }
    let body = response.split("\r\n\r\n").nth(1)?;
    serde_json::from_str(body).ok()
}

fn health() -> Option<Health> {
    get("/api/v1/health")
}

/// Whether the app speaks English: the language picked in Settings (kept by the
/// daemon), or else the Mac's.
pub fn english() -> bool {
    let value = get::<serde_json::Value>("/api/v1/preferences/language");
    let pref = value.as_ref().and_then(|v| v.as_str());
    let langs = output("defaults", &["read", "-g", "AppleLanguages"]).unwrap_or_default();
    english_for(pref, &langs)
}

/// The language rule (contracts/language.md): "es"/"en" are explicit; anything else
/// follows the first entry of `defaults read -g AppleLanguages`, where `es*` is
/// Spanish and anything else (empty, garbage) is English.
pub fn english_for(pref: Option<&str>, apple_languages: &str) -> bool {
    match pref {
        Some("en") => true,
        Some("es") => false,
        _ => {
            // First array element, quoted or not: `(\n    es,\n    "en-US"\n)`.
            let trim = |c: char| c == '(' || c == ')' || c == '"' || c == ',' || c.is_whitespace();
            let inner = apple_languages.trim_start_matches(trim);
            let end = inner.find(|c: char| c == '"' || c == ',' || c == ')' || c.is_whitespace());
            let first = inner[..end.unwrap_or(inner.len())].to_lowercase();
            !first.starts_with("es")
        }
    }
}

fn wait_for_health(timeout: Duration) -> Result<(), String> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if health().is_some() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err(format!(
        "delilahd did not answer on {ADDR}; see ~/Library/Logs/Delilah/delilahd.log"
    ))
}

#[cfg(test)]
mod tests {
    use super::english_for;

    // The table of contracts/language.md, mirrored in app/src/i18n.test.ts and crates/delilah-daemon/src/lang.rs.
    fn apple(first: &str) -> String {
        format!("(\n    \"{first}\",\n    \"en-US\"\n)")
    }

    #[test]
    fn english_for_explicit_preference_wins() {
        for sys in ["es-ES", "en-US", "", "garbage"] {
            let langs = if sys.is_empty() || sys == "garbage" { sys.to_string() } else { apple(sys) };
            assert!(english_for(Some("en"), &langs));
            assert!(!english_for(Some("es"), &langs));
        }
    }

    #[test]
    fn english_for_other_preferences_follow_the_system() {
        for pref in [None, Some("auto"), Some("fr")] {
            for (sys, en) in [
                ("es-ES", false),
                ("es-419", false),
                ("ES", false),
                ("en-US", true),
                ("fr-FR", true),
                ("ja-JP", true),
            ] {
                assert_eq!(english_for(pref, &apple(sys)), en, "{pref:?} {sys}");
            }
            assert!(!english_for(pref, "(\n    es,\n    \"en-US\"\n)"));
            assert!(english_for(pref, "(\n    en,\n    \"es-ES\"\n)"));
            assert!(!english_for(pref, "es-ES"));
            assert!(english_for(pref, ""));
            assert!(english_for(pref, "not a plist"));
        }
    }
}
