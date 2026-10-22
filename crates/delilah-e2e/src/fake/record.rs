//! What a fake session saw, written to `<fake_dir>/records/<session>.json` after each
//! step so the test can read it (data-model.md "Record").

use std::collections::BTreeMap;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HookResult {
    pub event: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub args: Vec<String>,
    /// `new` or `resume`.
    pub mode: String,
    pub session_id: String,
    pub task: Option<String>,
    pub cwd: String,
    pub env: BTreeMap<String, String>,
    /// Path of the `--settings` file.
    pub settings: String,
    pub prompt: Option<String>,
    pub scenario: String,
    pub steps_done: usize,
    pub hooks: Vec<HookResult>,
    pub answers: Vec<Value>,
    pub inputs: Vec<String>,
    /// `None` while running, then the exit code.
    pub exited: Option<i32>,
}

fn records_dir(fake_dir: &Path) -> PathBuf {
    fake_dir.join("records")
}

impl Record {
    /// Writes the record atomically (temp file + rename) and returns its path.
    pub fn save(&self, fake_dir: &Path) -> Result<PathBuf, String> {
        let dir = records_dir(fake_dir);
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = dir.join(format!("{}.json", self.session_id));
        let tmp = dir.join(format!(".{}.json.tmp", self.session_id));
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let write = || -> std::io::Result<()> {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&tmp)?;
            f.write_all(text.as_bytes())
        };
        write().map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    }

    pub fn load(fake_dir: &Path, session: &str) -> Result<Record, String> {
        read(&records_dir(fake_dir).join(format!("{session}.json")))
    }

    /// The most recently written record whose task is `task`.
    pub fn for_task(fake_dir: &Path, task: &str) -> Result<Option<Record>, String> {
        let Ok(rd) = std::fs::read_dir(records_dir(fake_dir)) else {
            return Ok(None);
        };
        let mut best: Option<(std::time::SystemTime, Record)> = None;
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let Ok(record) = read(&path) else { continue };
            if record.task.as_deref() != Some(task) {
                continue;
            }
            let mtime = entry.metadata().and_then(|m| m.modified()).map_err(|e| e.to_string())?;
            if best.as_ref().is_none_or(|(t, _)| mtime > *t) {
                best = Some((mtime, record));
            }
        }
        Ok(best.map(|(_, r)| r))
    }
}

fn read(path: &Path) -> Result<Record, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample(session: &str, task: &str) -> Record {
        Record {
            args: vec!["--session-id".into(), session.into()],
            mode: "new".into(),
            session_id: session.into(),
            task: Some(task.into()),
            cwd: "/w".into(),
            env: BTreeMap::from([("A".into(), "1".into())]),
            settings: "/s.json".into(),
            prompt: Some("hi".into()),
            scenario: "default".into(),
            steps_done: 2,
            hooks: vec![HookResult { event: "Stop".into(), exit_code: 0 }],
            answers: vec![json!({"q": "a"})],
            inputs: vec!["line".into()],
            exited: None,
        }
    }

    #[test]
    fn serialises_every_field() {
        let v = serde_json::to_value(sample("s1", "K-1")).unwrap();
        for f in [
            "args", "mode", "session_id", "task", "cwd", "env", "settings", "prompt",
            "scenario", "steps_done", "hooks", "answers", "inputs", "exited",
        ] {
            assert!(v.get(f).is_some(), "missing {f}");
        }
        assert!(v["exited"].is_null());
        assert_eq!(v["hooks"][0], json!({"event": "Stop", "exit_code": 0}));
    }

    #[test]
    fn save_writes_atomically_and_loads_back() {
        let dir = tempfile::tempdir().unwrap();
        let r = sample("s1", "K-1");
        let path = r.save(dir.path()).unwrap();
        assert_eq!(path, dir.path().join("records/s1.json"));
        let names: Vec<_> = std::fs::read_dir(dir.path().join("records")).unwrap().flatten().collect();
        assert_eq!(names.len(), 1, "no temp file left behind");
        assert_eq!(Record::load(dir.path(), "s1").unwrap(), r);
    }

    #[test]
    fn records_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = sample("s1", "K-1").save(dir.path()).unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(&dir.path().join("records")), 0o700);
    }

    #[test]
    fn for_task_returns_latest() {
        let dir = tempfile::tempdir().unwrap();
        sample("old", "K-1").save(dir.path()).unwrap();
        sample("other", "K-2").save(dir.path()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let mut newer = sample("new", "K-1");
        newer.steps_done = 9;
        newer.save(dir.path()).unwrap();
        assert_eq!(Record::for_task(dir.path(), "K-1").unwrap().unwrap().session_id, "new");
        assert!(Record::for_task(dir.path(), "K-3").unwrap().is_none());
    }
}
