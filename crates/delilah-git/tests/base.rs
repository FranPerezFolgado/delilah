mod common;
use common::*;

use std::path::PathBuf;
use std::time::Duration;

use delilah_git::{Deadline, FetchOutcome, fetch, prepare_base};

fn dl() -> Deadline {
    Deadline::after(Duration::from_secs(20))
}

fn remote(fx: &Fixture) -> PathBuf {
    fx.root.join("env/remotes/svc-orders.git")
}

#[test]
fn fresh_remote_head_is_the_base_and_main_stays() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let before = git(&repo, &["rev-parse", "main"]);
    let new = advance_remote(&remote(&fx), "a.txt", "advance");
    let b = prepare_base(&repo, "svc", &opts(), dl());
    assert_eq!(b.refname, "origin/main");
    assert_eq!(b.sha, new);
    assert_eq!(b.fetch, FetchOutcome::Fetched);
    assert_eq!(git(&repo, &["rev-parse", "main"]), before);
}

#[test]
fn no_origin_is_no_remote() {
    let dir = tempfile::TempDir::new().unwrap();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["commit", "-q", "--allow-empty", "-m", "one"]);
    let b = prepare_base(dir.path(), "x", &opts(), dl());
    assert_eq!(b.fetch, FetchOutcome::NoRemote);
    assert_eq!(b.refname, "main");
    assert_eq!(b.sha, git(dir.path(), &["rev-parse", "HEAD"]));
}

#[test]
fn failed_fetch_falls_back_to_last_fetched() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let old = git(&repo, &["rev-parse", "origin/main"]);
    advance_remote(&remote(&fx), "a.txt", "advance");
    git(&repo, &["remote", "set-url", "origin", "/nonexistent"]);
    let b = prepare_base(&repo, "svc", &opts(), dl());
    match &b.fetch {
        FetchOutcome::Failed(r) => assert!(!r.is_empty()),
        o => panic!("{o:?}"),
    }
    assert_eq!(b.refname, "origin/main");
    assert_eq!(b.sha, old);
}

#[test]
fn missing_origin_head_is_restored() {
    let fx = Fixture::new();
    let repo = fx.repo();
    git(&repo, &["remote", "set-head", "origin", "-d"]);
    let b = prepare_base(&repo, "svc", &opts(), dl());
    assert_eq!(b.refname, "origin/main");
    assert_eq!(b.fetch, FetchOutcome::Fetched);
    assert_eq!(
        git(
            &repo,
            &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"]
        ),
        "origin/main"
    );
}

#[test]
fn master_default_is_resolved() {
    let fx = Fixture::new();
    let bare = fx.root.join("m.git");
    git(&fx.root, &["init", "-q", "--bare", "-b", "master", "m.git"]);
    let work = fx.root.join("mwork");
    git(&fx.root, &["init", "-q", "-b", "master", "mwork"]);
    git(&work, &["commit", "-q", "--allow-empty", "-m", "one"]);
    git(&work, &["remote", "add", "origin", &bare.to_string_lossy()]);
    git(&work, &["push", "-q", "origin", "master"]);
    git(
        &fx.root,
        &["clone", "-q", &bare.to_string_lossy(), "mclone"],
    );
    let clone = fx.root.join("mclone");
    git(&clone, &["remote", "set-head", "origin", "-d"]);
    let b = prepare_base(&clone, "m", &opts(), dl());
    assert_eq!(b.refname, "origin/master");
    assert_eq!(b.fetch, FetchOutcome::Fetched);
}

#[test]
fn origin_head_is_reported_by_target_name() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let b = prepare_base(&repo, "svc", &opts(), dl());
    assert_eq!(b.refname, "origin/main");
}

#[test]
fn concurrent_prepares_both_succeed() {
    let fx = Fixture::new();
    let repo = fx.repo();
    advance_remote(&remote(&fx), "a.txt", "advance");
    let hs: Vec<_> = (0..2)
        .map(|_| {
            let r = repo.clone();
            std::thread::spawn(move || prepare_base(&r, "svc", &opts(), dl()))
        })
        .collect();
    for h in hs {
        assert_eq!(h.join().unwrap().fetch, FetchOutcome::Fetched);
    }
}

#[test]
fn fetch_and_prepare_do_not_clash() {
    let fx = Fixture::new();
    let repo = fx.repo();
    let r = repo.clone();
    let t = std::thread::spawn(move || fetch(&r, &opts()));
    let b = prepare_base(&repo, "svc", &opts(), dl());
    assert_eq!(b.fetch, FetchOutcome::Fetched);
    t.join().unwrap().unwrap();
}
