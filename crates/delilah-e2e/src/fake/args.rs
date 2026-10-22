//! Argument parsing for the fake Claude Code: the same flags the daemon passes
//! to `claude` (see `delilah-agents::claude`), preceded by `--fake-dir <dir>`.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    New,
    Resume,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeArgs {
    pub fake_dir: Option<PathBuf>,
    pub mode: Mode,
    pub session_id: String,
    pub settings: PathBuf,
    pub add_dirs: Vec<PathBuf>,
    pub remote_control: Option<String>,
    pub permission_mode: Option<String>,
    pub setting_sources: Option<String>,
    pub strict_mcp: bool,
    pub plugin_dir: Option<PathBuf>,
    pub append_system_prompt: Option<String>,
    pub prompt: Option<String>,
    /// Options the fake does not know: recorded, never an error.
    pub unknown: Vec<String>,
}

impl FakeArgs {
    /// Parses argv without the program name.
    pub fn parse<I, S>(args: I) -> Result<FakeArgs, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut it = args.into_iter().map(Into::into);
        let mut fake_dir = None;
        let mut new_id = None;
        let mut resume_id = None;
        let mut settings = None;
        let mut add_dirs = Vec::new();
        let mut remote_control = None;
        let mut permission_mode = None;
        let mut setting_sources = None;
        let mut strict_mcp = false;
        let mut plugin_dir = None;
        let mut append_system_prompt = None;
        let mut prompt = None;
        let mut unknown = Vec::new();

        while let Some(arg) = it.next() {
            let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} needs a value"));
            match arg.as_str() {
                "--" => {
                    prompt = it.next();
                    unknown.extend(it.by_ref());
                    break;
                }
                "--fake-dir" => fake_dir = Some(PathBuf::from(value("--fake-dir")?)),
                "--session-id" => new_id = Some(value("--session-id")?),
                "--resume" => resume_id = Some(value("--resume")?),
                "--settings" => settings = Some(PathBuf::from(value("--settings")?)),
                "--add-dir" => add_dirs.push(PathBuf::from(value("--add-dir")?)),
                "--remote-control" => remote_control = Some(value("--remote-control")?),
                "--permission-mode" => permission_mode = Some(value("--permission-mode")?),
                "--setting-sources" => setting_sources = Some(value("--setting-sources")?),
                "--strict-mcp-config" => strict_mcp = true,
                "--plugin-dir" => plugin_dir = Some(PathBuf::from(value("--plugin-dir")?)),
                "--append-system-prompt" => {
                    append_system_prompt = Some(value("--append-system-prompt")?)
                }
                _ => unknown.push(arg),
            }
        }

        let (mode, session_id) = match (new_id, resume_id) {
            (Some(id), None) => (Mode::New, id),
            (None, Some(id)) => (Mode::Resume, id),
            (Some(_), Some(_)) => {
                return Err("--session-id and --resume are mutually exclusive".into());
            }
            (None, None) => return Err("one of --session-id or --resume is required".into()),
        };
        let settings = settings.ok_or("--settings is required")?;
        Ok(FakeArgs {
            fake_dir,
            mode,
            session_id,
            settings,
            add_dirs,
            remote_control,
            permission_mode,
            setting_sources,
            strict_mcp,
            plugin_dir,
            append_system_prompt,
            prompt,
            unknown,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<FakeArgs, String> {
        FakeArgs::parse(args.iter().copied())
    }

    #[test]
    fn new_session() {
        let a = parse(&["--session-id", "abc", "--settings", "/tmp/s.json", "--", "/feature PROJ-1"])
            .unwrap();
        assert_eq!(a.mode, Mode::New);
        assert_eq!(a.session_id, "abc");
        assert_eq!(a.settings, PathBuf::from("/tmp/s.json"));
        assert_eq!(a.prompt.as_deref(), Some("/feature PROJ-1"));
        assert!(a.unknown.is_empty() && a.add_dirs.is_empty() && !a.strict_mcp);
    }

    #[test]
    fn resume_session() {
        let a = parse(&["--resume", "abc", "--settings", "/tmp/s.json", "--", "continúa"]).unwrap();
        assert_eq!(a.mode, Mode::Resume);
        assert_eq!(a.session_id, "abc");
        assert_eq!(a.prompt.as_deref(), Some("continúa"));
    }

    #[test]
    fn fake_dir_first_and_remote_control() {
        let a = parse(&[
            "--fake-dir", "/r/fake", "--session-id", "abc", "--remote-control", "PROJ-1 · Principal",
            "--settings", "/tmp/s.json", "--permission-mode", "plan", "--", "/feature PROJ-1",
        ])
        .unwrap();
        assert_eq!(a.fake_dir, Some(PathBuf::from("/r/fake")));
        assert_eq!(a.remote_control.as_deref(), Some("PROJ-1 · Principal"));
        assert_eq!(a.permission_mode.as_deref(), Some("plan"));
    }

    #[test]
    fn repeated_add_dir() {
        let a = parse(&[
            "--session-id", "abc", "--add-dir", "/tmp/tasks/PROJ-1", "--add-dir", "/b",
            "--settings", "/tmp/s.json", "--", "/feature PROJ-1",
        ])
        .unwrap();
        assert_eq!(a.add_dirs, [PathBuf::from("/tmp/tasks/PROJ-1"), PathBuf::from("/b")]);
    }

    #[test]
    fn dont_ask_flags() {
        let a = parse(&[
            "--session-id", "abc", "--settings", "/s", "--permission-mode", "dontAsk",
            "--setting-sources", "user", "--strict-mcp-config", "--", "x",
        ])
        .unwrap();
        assert_eq!(a.permission_mode.as_deref(), Some("dontAsk"));
        assert_eq!(a.setting_sources.as_deref(), Some("user"));
        assert!(a.strict_mcp);
    }

    #[test]
    fn plugin_dir_and_system_prompt() {
        let a = parse(&[
            "--session-id", "abc", "--settings", "/s", "--plugin-dir", "/tmp/plugin",
            "--append-system-prompt", "be brief", "--", "idea",
        ])
        .unwrap();
        assert_eq!(a.plugin_dir, Some(PathBuf::from("/tmp/plugin")));
        assert_eq!(a.append_system_prompt.as_deref(), Some("be brief"));
    }

    #[test]
    fn prompt_starting_with_dash() {
        for p in ["--dangerously-skip-permissions", "-p", "idea"] {
            let a = parse(&["--session-id", "abc", "--settings", "/s", "--", p]).unwrap();
            assert_eq!(a.prompt.as_deref(), Some(p));
            assert!(a.unknown.is_empty());
        }
    }

    #[test]
    fn no_prompt_is_fine() {
        let a = parse(&["--session-id", "abc", "--settings", "/s"]).unwrap();
        assert_eq!(a.prompt, None);
    }

    #[test]
    fn unknown_option_is_recorded() {
        let a = parse(&["--session-id", "abc", "--new-flag", "--settings", "/s", "--", "x"]).unwrap();
        assert_eq!(a.unknown, ["--new-flag"]);
    }

    #[test]
    fn errors() {
        assert!(parse(&["--session-id", "abc", "--", "x"]).unwrap_err().contains("--settings"));
        let both = parse(&["--session-id", "a", "--resume", "a", "--settings", "/s"]);
        assert!(both.unwrap_err().contains("mutually exclusive"));
        assert!(parse(&["--settings", "/s"]).unwrap_err().contains("--session-id"));
        assert!(parse(&["--session-id"]).unwrap_err().contains("needs a value"));
    }
}
