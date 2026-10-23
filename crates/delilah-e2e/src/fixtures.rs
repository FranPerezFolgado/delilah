//! Each Env's own copies of the fixture repos, and the fake `gh` data
//! (`scripts/fake-gh.sh` answers from the JSON files written here).

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

/// The repos a test works with: a plain repo and a parent with submodules.
#[derive(Debug, Clone)]
pub struct Fixtures {
    /// A plain repo (a clone of `svc-orders`), named `signal`.
    pub repo: PathBuf,
    /// The parent repo with four submodules.
    pub parent: PathBuf,
    /// The bare repos acting as GitHub (`svc-orders.git`, ...).
    pub remotes: PathBuf,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

impl Fixtures {
    /// Builds `<into>/parent-env` (as `scripts/make-fixture-parent.sh` does) and
    /// `<into>/signal`. `home` is the HOME git sees, so the user's git config is not read.
    pub fn make(into: &Path, home: &Path) -> Result<Fixtures, String> {
        std::fs::create_dir_all(into).map_err(|e| format!("{}: {e}", into.display()))?;
        let parent_env = into.join("parent-env");
        let script = repo_root().join("scripts/make-fixture-parent.sh");
        run(Command::new("bash").arg(&script).arg(&parent_env), home)?;
        let remotes = parent_env.join("remotes");
        let repo = into.join("signal");
        run(
            Command::new("git")
                .args(["-c", "protocol.file.allow=always", "clone", "-q"])
                .arg(remotes.join("svc-orders.git"))
                .arg(&repo),
            home,
        )?;
        Ok(Fixtures {
            repo,
            parent: parent_env.join("parent"),
            remotes,
        })
    }
}

fn run(cmd: &mut Command, home: &Path) -> Result<(), String> {
    let out = cmd
        .env("HOME", home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .map_err(|e| format!("{cmd:?}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{cmd:?} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// Whether a PR's checks pass or fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Checks {
    Passing,
    Failing,
}

/// The fake `gh` data folder (`FAKE_GH_DIR`): `repo.json`, `user.json`,
/// `pr-list.json` and `pr-<n>.json` in the shapes the daemon's `gh` module parses.
#[derive(Debug, Clone)]
pub struct FakeGh {
    pub dir: PathBuf,
}

/// The login `gh` is signed in as; reviews by anyone else count.
pub const GH_USER: &str = "e2e-user";
const REPO_URL: &str = "https://github.com/o/r";

impl FakeGh {
    /// Creates `dir` with `repo.json` and `user.json` and no PRs.
    pub fn init(dir: &Path) -> Result<FakeGh, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let gh = FakeGh {
            dir: dir.to_path_buf(),
        };
        gh.write(
            "repo.json",
            &json!({"nameWithOwner": "o/r", "url": REPO_URL,
                    "defaultBranchRef": {"name": "main"}}),
        );
        gh.write("user.json", &json!({"login": GH_USER}));
        Ok(gh)
    }

    fn write(&self, name: &str, value: &Value) {
        let path = self.dir.join(name);
        let tmp = self.dir.join(format!(".{name}.tmp"));
        std::fs::write(&tmp, serde_json::to_vec_pretty(value).unwrap())
            .and_then(|()| std::fs::rename(&tmp, &path))
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    }

    fn read(&self, name: &str) -> Value {
        let path = self.dir.join(name);
        match std::fs::read_to_string(&path) {
            Ok(t) => serde_json::from_str(&t).unwrap_or_else(|e| panic!("{}: {e}", path.display())),
            Err(_) => Value::Null,
        }
    }

    /// The URL `open_pr(n, ..)` gives the PR.
    pub fn pr_url(n: i64) -> String {
        format!("{REPO_URL}/pull/{n}")
    }

    /// Opens PR `n` from `branch`, with no checks, reviews or comments.
    pub fn open_pr(&self, n: i64, branch: &str) {
        let mut list = self.read("pr-list.json");
        if !list.is_array() {
            list = json!([]);
        }
        let list_arr = list.as_array_mut().unwrap();
        list_arr.retain(|p| p["number"] != n);
        list_arr.push(json!({
            "number": n, "url": Self::pr_url(n), "state": "OPEN",
            "updatedAt": "2026-10-20T10:00:00Z", "headRefName": branch,
            "baseRefName": "main", "title": format!("PR {n}"), "isDraft": false,
        }));
        self.write("pr-list.json", &list);
        self.write(
            &format!("pr-{n}.json"),
            &json!({"data": {"repository": {"pullRequest": {
                "state": "OPEN", "isDraft": false, "merged": false,
                "headRefOid": format!("sha-{n}-1"), "baseRefName": "main",
                "title": format!("PR {n}"), "url": Self::pr_url(n), "reviewDecision": null,
                "commits": {"nodes": [{"commit": {"statusCheckRollup": null}}]},
                "reviews": {"nodes": []}, "comments": {"nodes": []},
                "reviewThreads": {"nodes": []},
            }}}}),
        );
    }

    fn edit(&self, n: i64, f: impl FnOnce(&mut Value)) {
        let name = format!("pr-{n}.json");
        let mut v = self.read(&name);
        assert!(!v.is_null(), "PR {n} was not opened: no {name}");
        f(&mut v["data"]["repository"]["pullRequest"]);
        self.write(&name, &v);
    }

    /// Sets the PR's one check (`build`) to passing or failing.
    pub fn set_checks(&self, n: i64, checks: Checks) {
        let conclusion = if checks == Checks::Passing {
            "SUCCESS"
        } else {
            "FAILURE"
        };
        self.edit(n, |pr| {
            pr["commits"]["nodes"][0]["commit"]["statusCheckRollup"] = json!({"contexts": {"nodes": [
                {"__typename": "CheckRun", "name": "build", "status": "COMPLETED",
                 "conclusion": conclusion, "detailsUrl": "https://ci.invalid/1"}
            ]}});
        });
    }

    /// A new head commit (a failing check on it is a new `pr-failed` item).
    pub fn set_head(&self, n: i64, sha: &str) {
        self.edit(n, |pr| pr["headRefOid"] = json!(sha));
    }

    /// A review asking for changes, by someone other than the signed-in user.
    pub fn add_review(&self, n: i64, body: &str) {
        self.add_review_by(n, "reviewer", body);
    }

    pub fn add_review_by(&self, n: i64, author: &str, body: &str) {
        self.edit(n, |pr| {
            let nodes = pr["reviews"]["nodes"].as_array_mut().unwrap();
            let id = 100 + nodes.len() as i64;
            nodes.push(json!({"databaseId": id, "state": "CHANGES_REQUESTED",
                "author": {"login": author}, "body": body,
                "submittedAt": "2026-10-20T10:00:00Z"}));
            pr["reviewDecision"] = json!("CHANGES_REQUESTED");
        });
    }

    /// Marks the PR merged (in the list and in its snapshot).
    pub fn merge(&self, n: i64) {
        self.edit(n, |pr| {
            pr["state"] = json!("MERGED");
            pr["merged"] = json!(true);
        });
        let mut list = self.read("pr-list.json");
        for p in list.as_array_mut().into_iter().flatten() {
            if p["number"] == n {
                p["state"] = json!("MERGED");
            }
        }
        self.write("pr-list.json", &list);
    }

    /// Signs `gh` out: `gh api user` fails as "not logged in".
    pub fn sign_out(&self) {
        let _ = std::fs::remove_file(self.dir.join("user.json"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) -> bool {
        Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap()
            .status
            .success()
    }

    #[test]
    fn two_makes_give_independent_repos() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join(".gitconfig"),
            "[user]\n name = t\n email = t@t.invalid\n[protocol \"file\"]\n allow = always\n",
        )
        .unwrap();
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let fa = Fixtures::make(a.path(), home.path()).unwrap();
        let fb = Fixtures::make(b.path(), home.path()).unwrap();
        assert!(fa.repo.join(".git").exists() && fa.parent.join(".gitmodules").exists());
        assert!(fa.remotes.join("svc-orders.git").exists());
        assert_ne!(fa.repo, fb.repo);
        assert!(git(&fa.repo, &["checkout", "-q", "-b", "only-in-a"]));
        assert!(git(&fa.repo, &["rev-parse", "--verify", "only-in-a"]));
        assert!(!git(&fb.repo, &["rev-parse", "--verify", "only-in-a"]));
    }

    #[test]
    fn gh_helpers_write_the_shapes_the_daemon_reads() {
        let d = tempfile::tempdir().unwrap();
        let gh = FakeGh::init(d.path()).unwrap();
        assert!(d.path().join("repo.json").exists() && d.path().join("user.json").exists());
        assert!(!d.path().join("pr-list.json").exists());

        gh.open_pr(7, "feature/K-1");
        let list = gh.read("pr-list.json");
        assert_eq!(list[0]["number"], 7);
        assert_eq!(list[0]["state"], "OPEN");
        assert_eq!(list[0]["headRefName"], "feature/K-1");

        gh.set_checks(7, Checks::Failing);
        let pr = &gh.read("pr-7.json")["data"]["repository"]["pullRequest"];
        let node =
            &pr["commits"]["nodes"][0]["commit"]["statusCheckRollup"]["contexts"]["nodes"][0];
        assert_eq!(node["conclusion"], "FAILURE");
        gh.set_checks(7, Checks::Passing);
        let pr = &gh.read("pr-7.json")["data"]["repository"]["pullRequest"];
        assert_eq!(
            pr["commits"]["nodes"][0]["commit"]["statusCheckRollup"]["contexts"]["nodes"][0]["conclusion"],
            "SUCCESS"
        );

        gh.add_review(7, "Please fix");
        let pr = &gh.read("pr-7.json")["data"]["repository"]["pullRequest"];
        assert_eq!(pr["reviews"]["nodes"][0]["body"], "Please fix");
        assert_ne!(pr["reviews"]["nodes"][0]["author"]["login"], GH_USER);

        gh.merge(7);
        let pr = &gh.read("pr-7.json")["data"]["repository"]["pullRequest"];
        assert_eq!(
            (pr["state"].as_str(), pr["merged"].as_bool()),
            (Some("MERGED"), Some(true))
        );
        assert_eq!(gh.read("pr-list.json")[0]["state"], "MERGED");
    }
}
