//! Tells the conversation views when a live session's transcript gained a complete
//! line: one daemon-wide loop (and a poke from the hooks) that stats the file and
//! looks at the new bytes for a newline. It also keeps each session's token meter
//! (`tokens.rs`): from the meter's offset it reads at most 1 MiB per check (outside the
//! lock, on a blocking thread), sums the usage of the new complete lines and tells the
//! activity topic when the total changed. The activity dirty flag is announced once per
//! tick too.

use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tokio::time::MissedTickBehavior;

use delilah_agents::transcript::{self, TokenUsage, UsageDelta};

use crate::conversation::candidate_transcript;
use crate::events::Topic;
use crate::sessions::Live;
use crate::state::AppState;
use crate::store_sessions::SessionKind;
use crate::tokens::TokenMeter;

/// The most one check reads of a transcript; a bigger file catches up over ticks.
const MAX_READ: usize = 1024 * 1024;

/// What the watcher knows of one live session's transcript.
#[derive(Debug, Default)]
pub struct TranscriptProgress {
    /// Last size seen.
    seen: u64,
    /// The file the meter is reading; another one resets it.
    path: Option<PathBuf>,
    meter: TokenMeter,
    /// How many reads of the file were made (to test that idle ticks make none).
    #[cfg(test)]
    reads: u32,
}

impl TranscriptProgress {
    /// Tokens counted so far; `Some` once any byte of the transcript was read, even if
    /// it had no usage lines yet.
    pub fn tokens(&self) -> Option<TokenUsage> {
        self.meter.usage()
    }
}

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
    app.transcript_progress
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|id, _| live.iter().any(|l| &l.id == id));
    for l in live {
        check(app, &l).await;
    }
    if app.activity_dirty.swap(false, Ordering::Relaxed) {
        app.events.emit(Topic::Activity);
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
    let (before, offset, last_id) = {
        let mut map = app
            .transcript_progress
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        match map.get_mut(&live.id) {
            Some(p) => {
                // Another file, or one that shrank: count it again from the start.
                if p.path != path || size.is_some_and(|s| s < p.meter.offset()) {
                    p.meter.reset();
                    p.seen = 0;
                    p.path = path.clone();
                }
                let before = p.seen;
                if let Some(size) = size {
                    p.seen = size;
                }
                (
                    Some(before),
                    p.meter.offset(),
                    p.meter.last_id().map(String::from),
                )
            }
            None => {
                // First sight: the lines already there are not news for the
                // conversation (a file still to be created counts as empty, so its
                // first line is), but they do count as tokens.
                map.insert(
                    live.id.clone(),
                    TranscriptProgress {
                        seen: size.unwrap_or(0),
                        path: path.clone(),
                        meter: TokenMeter::default(),
                        #[cfg(test)]
                        reads: 0,
                    },
                );
                (None, 0, None)
            }
        }
    };
    if let (Some(before), Some(size), Some(path)) = (before, size, &path)
        && size > before
        && has_newline_from(path, before)
    {
        app.events.emit(Topic::Conversations);
    }
    let Some(path) = path else {
        return;
    };
    // Nothing new past the meter: no file access.
    if size.is_none_or(|s| s <= offset) {
        return;
    }
    #[cfg(test)]
    if let Some(p) = app
        .transcript_progress
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_mut(&live.id)
    {
        p.reads += 1;
    }
    let read = tokio::task::spawn_blocking(move || read_usage(&path, offset, last_id.as_deref()))
        .await
        .ok()
        .flatten();
    let Some(delta) = read else {
        return;
    };
    let changed = {
        let mut map = app
            .transcript_progress
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        map.get_mut(&live.id)
            .is_some_and(|p| p.meter.apply(offset, delta))
    };
    if changed {
        app.events.emit(Topic::Activity);
    }
}

/// The usage of the complete lines in the next `MAX_READ` bytes from `offset`. A line
/// bigger than that is skipped whole (its bytes are consumed) so the meter never sticks.
fn read_usage(path: &std::path::Path, offset: u64, last_id: Option<&str>) -> Option<UsageDelta> {
    let mut file = std::fs::File::open(path).ok()?;
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut buf = Vec::new();
    file.take(MAX_READ as u64).read_to_end(&mut buf).ok()?;
    let mut delta = transcript::usage_delta(&buf, last_id);
    if delta.consumed == 0 && buf.len() == MAX_READ && !buf.contains(&b'\n') {
        delta.consumed = buf.len();
    }
    Some(delta)
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

    fn usage_line(id: &str, out: u64) -> String {
        format!(
            "{}\n",
            serde_json::json!({"type":"assistant","message":{"id":id,"usage":{"output_tokens":out}}})
        )
    }

    fn total(app: &AppState, id: &str) -> Option<u64> {
        let map = app
            .transcript_progress
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        map.get(id).map(|p| p.tokens().map_or(0, |t| t.total))
    }

    async fn activity_event(rx: &mut tokio::sync::broadcast::Receiver<Topic>) -> bool {
        tokio::time::timeout(Duration::from_millis(400), async {
            loop {
                if let Ok(Topic::Activity) = rx.recv().await {
                    return;
                }
            }
        })
        .await
        .is_ok()
    }

    #[tokio::test]
    async fn first_sight_counts_the_existing_lines() {
        let app = app().await;
        let (live, _) = session(&app, SessionKind::Claude, &usage_line("m1", 7));
        tick(&app).await;
        assert_eq!(total(&app, &live.id), Some(7));
    }

    #[tokio::test]
    async fn an_appended_line_is_counted_in_one_tick_and_emits() {
        let app = app().await;
        let (live, path) = session(&app, SessionKind::Claude, &usage_line("m1", 7));
        tick(&app).await;
        let mut rx = events(&app);
        append(&path, &usage_line("m2", 5));
        tick(&app).await;
        assert_eq!(total(&app, &live.id), Some(12));
        assert!(activity_event(&mut rx).await);
    }

    #[tokio::test]
    async fn a_partial_line_is_not_counted_until_completed() {
        let app = app().await;
        let (live, path) = session(&app, SessionKind::Claude, "");
        tick(&app).await;
        let full = usage_line("m1", 9);
        let (a, b) = full.split_at(20);
        append(&path, a);
        tick(&app).await;
        assert_eq!(total(&app, &live.id), Some(0));
        append(&path, b);
        tick(&app).await;
        assert_eq!(total(&app, &live.id), Some(9));
    }

    #[tokio::test]
    async fn a_big_file_catches_up_over_ticks() {
        let app = app().await;
        let line = usage_line("m1", 1);
        // Distinct ids so every line counts; enough lines to pass 1 MiB.
        let n = MAX_READ / line.len() * 2;
        let text: String = (0..n).map(|i| usage_line(&format!("m{i}"), 1)).collect();
        let (live, _) = session(&app, SessionKind::Claude, &text);
        tick(&app).await;
        let first = total(&app, &live.id).unwrap();
        assert!(first > 0 && first < n as u64);
        for _ in 0..5 {
            tick(&app).await;
        }
        assert_eq!(total(&app, &live.id), Some(n as u64));
    }

    #[tokio::test]
    async fn an_oversized_line_is_skipped_not_stuck() {
        let app = app().await;
        let giant = "x".repeat(MAX_READ + 10);
        let text = format!("{giant}\n{}", usage_line("m1", 3));
        let (live, _) = session(&app, SessionKind::Claude, &text);
        for _ in 0..4 {
            tick(&app).await;
        }
        assert_eq!(total(&app, &live.id), Some(3));
    }

    #[tokio::test]
    async fn two_concurrent_checks_count_once() {
        let app = app().await;
        let (live, _) = session(&app, SessionKind::Claude, &usage_line("m1", 7));
        tokio::join!(check(&app, &live), check(&app, &live), check(&app, &live));
        assert_eq!(total(&app, &live.id), Some(7));
    }

    #[tokio::test]
    async fn shell_and_outside_transcripts_are_never_read() {
        let app = app().await;
        let (shell, _) = session(&app, SessionKind::Shell, &usage_line("m1", 7));
        let (claude, _) = session(&app, SessionKind::Claude, "");
        let outside = std::env::temp_dir().join(format!("delilah-out-{}.jsonl", claude.id));
        std::fs::write(&outside, usage_line("m1", 7)).unwrap();
        claude.set_transcript(outside);
        tick(&app).await;
        assert_eq!(total(&app, &shell.id), None);
        assert_eq!(total(&app, &claude.id), None);
    }

    #[tokio::test]
    async fn the_dirty_flag_gives_one_emit_per_tick() {
        let app = app().await;
        let mut rx = events(&app);
        app.activity_dirty.store(true, Ordering::Relaxed);
        tick(&app).await;
        assert!(activity_event(&mut rx).await);
        tick(&app).await;
        assert!(!activity_event(&mut rx).await);
    }

    fn reads(app: &AppState, id: &str) -> u32 {
        app.transcript_progress
            .lock()
            .unwrap()
            .get(id)
            .map_or(0, |p| p.reads)
    }

    #[tokio::test]
    async fn a_tick_without_growth_reads_nothing() {
        let app = app().await;
        let (live, path) = session(&app, SessionKind::Claude, &usage_line("m1", 7));
        tick(&app).await;
        let first = reads(&app, &live.id);
        tick(&app).await;
        tick(&app).await;
        assert_eq!(reads(&app, &live.id), first);
        append(&path, &usage_line("m2", 1));
        tick(&app).await;
        assert_eq!(reads(&app, &live.id), first + 1);
        assert_eq!(total(&app, &live.id), Some(8));
    }

    #[tokio::test]
    async fn a_shrunk_or_replaced_transcript_is_counted_again() {
        let app = app().await;
        let (live, path) = session(
            &app,
            SessionKind::Claude,
            &(usage_line("m1", 7) + &usage_line("m2", 5)),
        );
        tick(&app).await;
        assert_eq!(total(&app, &live.id), Some(12));
        // Shrinks (rewritten shorter).
        std::fs::write(&path, usage_line("m3", 2)).unwrap();
        tick(&app).await;
        assert_eq!(total(&app, &live.id), Some(2));
        // Replaced by another, longer file.
        let other = app
            .config
            .claude_projects
            .join(format!("delilah-tw-other-{}.jsonl", live.id));
        std::fs::write(
            &other,
            usage_line("a", 1) + &usage_line("b", 1) + &usage_line("c", 1),
        )
        .unwrap();
        live.set_transcript(other);
        tick(&app).await;
        assert_eq!(total(&app, &live.id), Some(3));
    }

    #[tokio::test]
    async fn tokens_is_some_once_bytes_were_read() {
        let app = app().await;
        let (live, _) = session(&app, SessionKind::Claude, "{\"type\":\"user\"}\n");
        tick(&app).await;
        let map = app.transcript_progress.lock().unwrap();
        assert_eq!(
            map.get(&live.id).unwrap().tokens().map(|t| t.total),
            Some(0)
        );
    }
}
