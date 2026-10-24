//! Tells the conversation views when a live session's transcript gained a complete
//! line: one daemon-wide loop (and a poke from the hooks) that only stats the file and
//! looks at the new bytes for a newline, never parses it.

use std::io::{Read, Seek, SeekFrom};
use std::time::Duration;

use tokio::time::MissedTickBehavior;

use delilah_agents::transcript;

use crate::conversation::candidate_transcript;
use crate::events::Topic;
use crate::sessions::Live;
use crate::state::AppState;
use crate::store_sessions::SessionKind;

pub const INTERVAL: Duration = Duration::from_secs(1);

pub fn spawn(app: AppState) {
    spawn_with(app, INTERVAL);
}

pub fn spawn_with(app: AppState, interval: Duration) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            tick(&app).await;
        }
    });
}

/// Checks every live Claude session and forgets the ones that are gone.
pub async fn tick(app: &AppState) {
    let live = app.sessions.all();
    app.transcript_seen
        .lock()
        .unwrap()
        .retain(|id, _| live.iter().any(|l| &l.id == id));
    for l in live {
        check(app, &l).await;
    }
}

/// Checks one session at once (a hook just spoke).
pub fn poke(app: &AppState, session: &str) {
    let Some(live) = app.sessions.get(session) else {
        return;
    };
    let app = app.clone();
    tokio::spawn(async move { check(&app, &live).await });
}

async fn check(app: &AppState, live: &Live) {
    if live.kind == SessionKind::Shell {
        return;
    }
    let Some(candidate) = candidate_transcript(app, &live.task, &live.id, Some(live)).await else {
        return;
    };
    let path = transcript::within(&app.config.claude_projects, &candidate);
    // Outside the projects folder: never read. Not there yet: counts as empty.
    if path.is_none() && candidate.exists() {
        return;
    }
    let size = path
        .as_ref()
        .and_then(|p| std::fs::metadata(p).ok())
        .map(|m| m.len());
    let before = {
        let mut seen = app.transcript_seen.lock().unwrap();
        match seen.get_mut(&live.id) {
            Some(last) => {
                let before = *last;
                if let Some(size) = size {
                    *last = size;
                }
                Some(before)
            }
            None => {
                // First sight: the lines already there are not news (a file still to
                // be created counts as empty, so its first line is).
                seen.insert(live.id.clone(), size.unwrap_or(0));
                None
            }
        }
    };
    if let (Some(before), Some(size), Some(path)) = (before, size, &path)
        && size > before
        && has_newline_from(path, before)
    {
        app.events.emit(Topic::Conversations);
    }
}
/// Whether the file has a `\n` from `offset` on.
fn has_newline_from(path: &std::path::Path, offset: u64) -> bool {
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    if file.seek(SeekFrom::Start(offset)).is_err() {
        return false;
    }
    let mut buf = [0u8; 8192];
    loop {
        match file.read(&mut buf) {
            Ok(0) | Err(_) => return false,
            Ok(n) if buf[..n].contains(&b'\n') => return true,
            Ok(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::PathBuf;

    use portable_pty::CommandBuilder;

    use super::*;
    use crate::config::Config;
    use crate::events::Events;
    use crate::store::Store;

    const FAST: Duration = Duration::from_millis(30);

    async fn app() -> AppState {
        let config = Config {
            home: std::env::temp_dir().join(format!("delilah-tw-{}", uuid::Uuid::new_v4())),
            addr: "127.0.0.1:7420".parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            claude_bin: "claude".into(),
            agent_program: None,
            claude_projects: crate::config::Config::scratch_projects(),
            ..Config::test_defaults()
        };
        AppState::new(Store::in_memory().await.unwrap(), config)
    }

    fn session(app: &AppState, kind: SessionKind, content: &str) -> (Live, PathBuf) {
        let mut cmd = CommandBuilder::new("sleep");
        cmd.arg("30");
        let pty = delilah_agents::pty::PtySession::spawn(cmd, 80, 24, None).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let live = app.sessions.insert(&id, "@web", kind, false, pty);
        let root = &app.config.claude_projects;
        std::fs::create_dir_all(root).unwrap();
        let path = root.join(format!("delilah-tw-{id}.jsonl"));
        std::fs::write(&path, content).unwrap();
        live.set_transcript(path.clone());
        (live, path)
    }

    fn append(path: &PathBuf, text: &str) {
        let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
        f.write_all(text.as_bytes()).unwrap();
    }

    async fn next_event(rx: &mut tokio::sync::broadcast::Receiver<Topic>) -> bool {
        matches!(
            tokio::time::timeout(Duration::from_millis(400), async {
                loop {
                    if let Ok(Topic::Conversations) = rx.recv().await {
                        return;
                    }
                }
            })
            .await,
            Ok(())
        )
    }

    fn events(app: &AppState) -> tokio::sync::broadcast::Receiver<Topic> {
        let events: &Events = &app.events;
        events.subscribe()
    }

    #[tokio::test]
    async fn a_complete_line_emits_within_one_interval() {
        let app = app().await;
        let (_live, path) = session(&app, SessionKind::Claude, "{\"a\":1}\n");
        spawn_with(app.clone(), FAST);
        tokio::time::sleep(FAST * 3).await;
        let mut rx = events(&app);
        append(&path, "{\"b\":2}\n");
        assert!(next_event(&mut rx).await);
    }

    #[tokio::test]
    async fn the_first_sight_of_a_session_emits_nothing() {
        let app = app().await;
        let (_live, _path) = session(&app, SessionKind::Claude, "{\"a\":1}\n");
        let mut rx = events(&app);
        spawn_with(app.clone(), FAST);
        assert!(!next_event(&mut rx).await);
    }

    #[tokio::test]
    async fn a_partial_line_emits_nothing_until_completed() {
        let app = app().await;
        let (_live, path) = session(&app, SessionKind::Claude, "");
        spawn_with(app.clone(), FAST);
        tokio::time::sleep(FAST * 3).await;
        let mut rx = events(&app);
        append(&path, "{\"b\":");
        assert!(!next_event(&mut rx).await);
        append(&path, "2}\n");
        assert!(next_event(&mut rx).await);
    }

    #[tokio::test]
    async fn no_live_session_means_no_events() {
        let app = app().await;
        let mut rx = events(&app);
        spawn_with(app.clone(), FAST);
        assert!(!next_event(&mut rx).await);
    }

    #[tokio::test]
    async fn a_shell_session_is_not_watched() {
        let app = app().await;
        let (_live, path) = session(&app, SessionKind::Shell, "");
        spawn_with(app.clone(), FAST);
        tokio::time::sleep(FAST * 3).await;
        let mut rx = events(&app);
        append(&path, "x\n");
        assert!(!next_event(&mut rx).await);
    }

    #[tokio::test]
    async fn poke_checks_at_once() {
        let app = app().await;
        let (live, path) = session(&app, SessionKind::Claude, "");
        // No loop running: the pokes alone do the checking.
        poke(&app, &live.id);
        tokio::time::sleep(Duration::from_millis(100)).await;
        let mut rx = events(&app);
        append(&path, "{\"b\":2}\n");
        poke(&app, &live.id);
        assert!(next_event(&mut rx).await);
    }
}
