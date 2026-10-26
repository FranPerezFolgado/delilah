//! The skills and slash commands Claude Code can run for a project: the user's
//! (`~/.claude`), the project's (`<repo>/.claude`) and those of enabled plugins.
//! Read-only: files are read, never executed, and symlinks are not followed.

use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Deepest folder level scanned under a `commands` folder.
const MAX_DEPTH: usize = 6;
/// Only the start of a file is read: the frontmatter and first line live there.
const MAX_READ: u64 = 16 * 1024;
/// Entries read per folder, so a huge folder cannot stall the call.
const MAX_ENTRIES: usize = 2000;

/// Where a skill comes from; the order is the listing order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillSource {
    Project,
    User,
    Plugin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Skill {
    /// What follows the `/` to invoke it (`plugin:skill` for plugins).
    pub name: String,
    pub description: String,
    pub source: SkillSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
}

/// Every skill and command available for the project at `project_path`, with `home`
/// the user's home folder (`None`: only the project's). The same name in two sources is
/// listed once per source. Anything unreadable or malformed is skipped.
pub fn list(home: Option<&Path>, project_path: &Path) -> Vec<Skill> {
    let mut out = Vec::new();
    // A cloned repo is not trusted to point its folders elsewhere; the user's own config
    // (often a dotfiles symlink) is.
    let project_root = project_path.join(".claude");
    if !is_link(&project_root) {
        scan_root(&project_root, SkillSource::Project, None, false, &mut out);
    }
    if let Some(home) = home {
        let user_root = home.join(".claude");
        scan_root(&user_root, SkillSource::User, None, true, &mut out);
        for (plugin, install) in enabled_plugins(&user_root) {
            scan_root(&install, SkillSource::Plugin, Some(&plugin), true, &mut out);
        }
    }
    out.sort_by(|a, b| (a.source, &a.name).cmp(&(b.source, &b.name)));
    out
}

fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

/// Scans `root`'s `skills` and `commands` folders; with `links` false, a folder that is
/// a symlink is skipped.
fn scan_root(
    root: &Path,
    source: SkillSource,
    plugin: Option<&str>,
    links: bool,
    out: &mut Vec<Skill>,
) {
    let folder = |name: &str| Some(root.join(name)).filter(|p| links || !is_link(p));
    let prefix = |name: String| match plugin {
        Some(p) => format!("{p}:{name}"),
        None => name,
    };
    let mut push = |name: String, description: String| {
        out.push(Skill {
            name: prefix(name),
            description,
            source,
            plugin: plugin.map(str::to_string),
        });
    };
    // skills/<dir>/SKILL.md
    for (dir, is_dir) in folder("skills").map(|p| entries(&p)).unwrap_or_default() {
        if !is_dir {
            continue;
        }
        let Some(text) = read_head(&dir.join("SKILL.md")) else {
            continue;
        };
        let fm = frontmatter(&text);
        let name = fm
            .get("name")
            .filter(|n| !n.is_empty())
            .cloned()
            .or_else(|| dir.file_name().map(|n| n.to_string_lossy().into_owned()));
        if let Some(name) = name {
            push(name, description(&fm, &text));
        }
    }
    // commands/**/*.md, subfolders joined with `:`
    let mut found = Vec::new();
    if let Some(commands) = folder("commands") {
        collect_commands(&commands, &mut Vec::new(), &mut found);
    }
    for (parts, file) in found {
        let Some(text) = read_head(&file) else {
            continue;
        };
        let fm = frontmatter(&text);
        push(parts.join(":"), description(&fm, &text));
    }
}

fn collect_commands(dir: &Path, parts: &mut Vec<String>, out: &mut Vec<(Vec<String>, PathBuf)>) {
    if parts.len() >= MAX_DEPTH {
        return;
    }
    for (path, is_dir) in entries(dir) {
        let Some(file_name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        if is_dir {
            parts.push(file_name);
            collect_commands(&path, parts, out);
            parts.pop();
        } else if let Some(stem) = file_name.strip_suffix(".md").filter(|s| !s.is_empty()) {
            let mut p = parts.clone();
            p.push(stem.to_string());
            out.push((p, path));
        }
    }
}

/// The entries of `dir` that are real files or folders (symlinks are skipped), as
/// `(path, is_dir)`, in name order.
fn entries(dir: &Path) -> Vec<(PathBuf, bool)> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut v: Vec<(PathBuf, bool)> = rd
        .filter_map(Result::ok)
        .take(MAX_ENTRIES)
        .filter_map(|e| {
            let t = e.file_type().ok()?;
            if t.is_symlink() {
                None
            } else {
                Some((e.path(), t.is_dir()))
            }
        })
        .collect();
    v.sort();
    v
}

fn read_head(path: &Path) -> Option<String> {
    let meta = fs::symlink_metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let mut buf = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(MAX_READ)
        .read_to_end(&mut buf)
        .ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// The `key: value` lines between a leading `---` and the next `---`.
fn frontmatter(text: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return map;
    }
    // The key of an open YAML block scalar (`key: >` or `key: |`), whose indented
    // lines are its value, joined with spaces.
    let mut block: Option<String> = None;
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if line.starts_with(char::is_whitespace) {
            if let Some(value) = block.as_ref().and_then(|k| map.get_mut(k)) {
                let part = line.trim();
                if !part.is_empty() {
                    if !value.is_empty() {
                        value.push(' ');
                    }
                    value.push_str(part);
                }
            }
            continue;
        }
        block = None;
        if let Some((k, v)) = line.split_once(':') {
            let v = v.trim().trim_matches(|c| c == '"' || c == '\'').trim();
            let k = k.trim().to_string();
            if matches!(v, ">" | "|" | ">-" | "|-" | ">+" | "|+") {
                map.insert(k.clone(), String::new());
                block = Some(k);
            } else {
                map.insert(k, v.to_string());
            }
        }
    }
    map
}

/// The frontmatter `description`, else the first non-empty line of the body.
fn description(fm: &std::collections::HashMap<String, String>, text: &str) -> String {
    if let Some(d) = fm.get("description").filter(|d| !d.is_empty()) {
        return d.clone();
    }
    let mut lines = text.lines();
    let body: Vec<&str> = if text.lines().next().map(str::trim) == Some("---") {
        lines.next();
        let mut rest = lines.skip_while(|l| l.trim() != "---");
        rest.next();
        rest.collect()
    } else {
        lines.collect()
    };
    body.iter()
        .map(|l| l.trim().trim_start_matches('#').trim())
        .find(|l| !l.is_empty())
        .unwrap_or_default()
        .to_string()
}

/// `(plugin name, install path)` of the plugins installed and enabled in `settings.json`.
fn enabled_plugins(claude: &Path) -> Vec<(String, PathBuf)> {
    let read = |p: PathBuf| -> Option<serde_json::Value> {
        serde_json::from_str(&read_head_all(&p)?).ok()
    };
    let (Some(installed), Some(settings)) = (
        read(claude.join("plugins/installed_plugins.json")),
        read(claude.join("settings.json")),
    ) else {
        return Vec::new();
    };
    let Some(enabled) = settings.get("enabledPlugins").and_then(|e| e.as_object()) else {
        return Vec::new();
    };
    let Some(plugins) = installed.get("plugins").and_then(|p| p.as_object()) else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for (id, entry) in plugins {
        if enabled.get(id).and_then(|v| v.as_bool()) != Some(true) {
            continue;
        }
        // Format 2 keeps a list of installs per plugin, format 1 a single object.
        let install = match entry {
            serde_json::Value::Array(a) => a.first(),
            other => Some(other),
        }
        .and_then(|e| e.get("installPath"))
        .and_then(|p| p.as_str());
        let name = id.split('@').next().unwrap_or(id);
        if let Some(install) = install.filter(|_| !name.is_empty() && seen.insert(id.clone())) {
            out.push((name.to_string(), PathBuf::from(install)));
        }
    }
    out.sort();
    out
}

/// A small JSON config file, whole (bounded at 1 MiB).
fn read_head_all(path: &Path) -> Option<String> {
    let mut buf = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(1 << 20)
        .read_to_end(&mut buf)
        .ok()?;
    String::from_utf8(buf).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn names(v: &[Skill]) -> Vec<(String, SkillSource)> {
        v.iter().map(|s| (s.name.clone(), s.source)).collect()
    }

    #[test]
    fn lists_user_project_and_enabled_plugin_entries_sorted() {
        let t = tempfile::tempdir().unwrap();
        let (home, proj) = (t.path().join("home"), t.path().join("proj"));
        let c = home.join(".claude");
        write(
            &c.join("skills/zeta/SKILL.md"),
            "---\nname: zeta\ndescription: The last one\n---\nbody",
        );
        write(&c.join("commands/a/b.md"), "# Do the thing\nmore");
        write(
            &c.join("commands/plain.md"),
            "---\ndescription: \"Quoted\"\n---\nx",
        );
        write(&proj.join(".claude/commands/fix.md"), "Fix a bug\n");
        write(
            &proj.join(".claude/skills/fix/SKILL.md"),
            "no frontmatter\n",
        );
        let plug = t.path().join("cache/pl");
        write(
            &plug.join("skills/design/SKILL.md"),
            "---\nname: design\n---\nPretty",
        );
        write(&plug.join("commands/go.md"), "Go\n");
        let off = t.path().join("cache/off");
        write(&off.join("commands/no.md"), "No\n");
        write(
            &c.join("plugins/installed_plugins.json"),
            &format!(
                r#"{{"version":2,"plugins":{{"pl@m":[{{"installPath":"{}"}}],"off@m":[{{"installPath":"{}"}}]}}}}"#,
                plug.display(),
                off.display()
            ),
        );
        write(
            &c.join("settings.json"),
            r#"{"enabledPlugins":{"pl@m":true,"off@m":false}}"#,
        );

        let got = list(Some(&home), &proj);
        assert_eq!(
            names(&got),
            vec![
                ("fix".into(), SkillSource::Project),
                ("fix".into(), SkillSource::Project),
                ("a:b".into(), SkillSource::User),
                ("plain".into(), SkillSource::User),
                ("zeta".into(), SkillSource::User),
                ("pl:design".into(), SkillSource::Plugin),
                ("pl:go".into(), SkillSource::Plugin),
            ]
        );
        let by = |n: &str| got.iter().find(|s| s.name == n).unwrap();
        assert_eq!(by("zeta").description, "The last one");
        assert_eq!(by("a:b").description, "Do the thing");
        assert_eq!(by("plain").description, "Quoted");
        assert_eq!(by("pl:design").description, "Pretty");
        assert_eq!(by("pl:go").plugin.as_deref(), Some("pl"));
        assert_eq!(by("plain").plugin, None);
    }

    #[test]
    fn skill_name_falls_back_to_folder_and_bad_files_are_skipped() {
        let t = tempfile::tempdir().unwrap();
        let (home, proj) = (t.path().join("home"), t.path().join("proj"));
        let c = home.join(".claude");
        write(
            &c.join("skills/stemmed/SKILL.md"),
            "---\ndescription: d\n---\n",
        );
        // Not UTF-8, a folder where a file should be, and broken JSON: none may fail the call.
        fs::create_dir_all(c.join("skills/broken")).unwrap();
        fs::create_dir_all(c.join("skills/broken/SKILL.md")).unwrap();
        fs::create_dir_all(c.join("commands/dir.md")).unwrap();
        write(&c.join("plugins/installed_plugins.json"), "{not json");
        let got = list(Some(&home), &proj);
        assert_eq!(names(&got), vec![("stemmed".into(), SkillSource::User)]);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_followed() {
        let t = tempfile::tempdir().unwrap();
        let (home, proj) = (t.path().join("home"), t.path().join("proj"));
        let outside = t.path().join("outside");
        write(&outside.join("x.md"), "secret\n");
        let c = home.join(".claude");
        fs::create_dir_all(c.join("commands")).unwrap();
        std::os::unix::fs::symlink(&outside, c.join("commands/link")).unwrap();
        std::os::unix::fs::symlink(outside.join("x.md"), c.join("commands/x.md")).unwrap();
        assert!(list(Some(&home), &proj).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_project_cannot_link_its_folders_elsewhere() {
        let t = tempfile::tempdir().unwrap();
        let (home, proj) = (t.path().join("home"), t.path().join("proj"));
        let outside = t.path().join("outside");
        write(&outside.join("commands/x.md"), "secret\n");
        write(&outside.join("y.md"), "secret\n");
        // The project's `.claude/commands`, and a second project's whole `.claude`.
        fs::create_dir_all(proj.join(".claude")).unwrap();
        std::os::unix::fs::symlink(&outside, proj.join(".claude/commands")).unwrap();
        assert!(list(Some(&home), &proj).is_empty());
        let other = t.path().join("other");
        fs::create_dir_all(&other).unwrap();
        std::os::unix::fs::symlink(&outside, other.join(".claude")).unwrap();
        assert!(list(Some(&home), &other).is_empty());
        // The user's own folders may be links (dotfiles).
        fs::create_dir_all(home.join(".claude")).unwrap();
        std::os::unix::fs::symlink(outside.join("commands"), home.join(".claude/commands"))
            .unwrap();
        assert_eq!(
            names(&list(Some(&home), &proj)),
            vec![("x".into(), SkillSource::User)]
        );
    }

    #[test]
    fn block_scalar_descriptions_are_joined() {
        let t = tempfile::tempdir().unwrap();
        let (home, proj) = (t.path().join("home"), t.path().join("proj"));
        write(
            &proj.join(".claude/skills/a/SKILL.md"),
            "---\nname: a\ndescription: >\n  Folded over\n  two lines.\nmodel: x\n---\nBody\n",
        );
        write(
            &proj.join(".claude/commands/b.md"),
            "---\ndescription: |-\n  Literal\n---\nBody\n",
        );
        let got = list(Some(&home), &proj);
        let by = |n: &str| got.iter().find(|s| s.name == n).unwrap();
        assert_eq!(by("a").description, "Folded over two lines.");
        assert_eq!(by("b").description, "Literal");
    }

    #[test]
    fn without_a_home_only_the_project_is_listed() {
        let t = tempfile::tempdir().unwrap();
        let proj = t.path().join("proj");
        write(&proj.join(".claude/commands/fix.md"), "Fix\n");
        assert_eq!(
            names(&list(None, &proj)),
            vec![("fix".into(), SkillSource::Project)]
        );
    }

    #[test]
    fn missing_folders_give_an_empty_list() {
        let t = tempfile::tempdir().unwrap();
        assert!(list(Some(&t.path().join("nohome")), &t.path().join("noproj")).is_empty());
    }
}
