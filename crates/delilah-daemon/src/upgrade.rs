//! Safety net for updates: the first start on a new version keeps a copy of the
//! database as the previous version left it, before any migration runs.
//!
//! `home/version` holds the version of the last start that opened the database.
//! Copies go to `home/backups/delilah-<prev>.db`; the last 3 are kept.

use std::path::{Path, PathBuf};

use sqlx::{
    ConnectOptions, Connection,
    sqlite::{SqliteConnectOptions, SqliteConnection},
};

const KEEP: usize = 3;

fn marker(home: &Path) -> PathBuf {
    home.join("version")
}

/// The marker's content is untrusted as a path: only a plain version token passes.
fn sanitize(prev: &str) -> String {
    let ok = !prev.is_empty()
        && prev.len() <= 64
        && !prev.starts_with('.')
        && prev
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+' | '_'));
    if ok {
        prev.to_string()
    } else {
        "unknown".to_string()
    }
}

/// Copy the database before `Store::open` migrates it, if this is a new version.
/// Never fails: a missing backup must not stop the daemon from starting.
pub async fn before_open(home: &Path, db: &Path, current: &str) {
    let prev = std::fs::read_to_string(marker(home)).ok();
    let prev = prev.as_deref().map(str::trim);
    if prev == Some(current) {
        return;
    }
    let backups = home.join("backups");
    if db.exists() {
        let name = format!("delilah-{}.db", sanitize(prev.unwrap_or("")));
        let target = backups.join(&name);
        if !target.exists() {
            match copy(db, &backups, &target).await {
                Ok(()) => {
                    tracing::info!(copy = %target.display(), "kept a copy of the database before upgrading")
                }
                Err(e) => tracing::warn!("could not back up the database to {name}: {e}"),
            }
        }
    }
    if let Err(e) = prune(&backups) {
        tracing::warn!("could not prune the database copies: {e}");
    }
}

async fn copy(db: &Path, backups: &Path, target: &Path) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(backups)?;
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(backups, std::fs::Permissions::from_mode(0o700))?;
    }
    let mut conn: SqliteConnection = SqliteConnectOptions::new()
        .filename(db)
        .read_only(true)
        .disable_statement_logging()
        .connect()
        .await?;
    let res = sqlx::query("VACUUM INTO ?1")
        .bind(target.to_string_lossy().into_owned())
        .execute(&mut conn)
        .await;
    let _ = conn.close().await;
    if let Err(e) = res {
        let _ = std::fs::remove_file(target);
        return Err(e.into());
    }
    Ok(())
}

/// Keep the newest `KEEP` copies by modification time.
fn prune(backups: &Path) -> std::io::Result<()> {
    let Ok(dir) = std::fs::read_dir(backups) else {
        return Ok(());
    };
    let mut copies: Vec<(std::time::SystemTime, PathBuf)> = dir
        .filter_map(Result::ok)
        .filter(|e| {
            let n = e.file_name();
            let n = n.to_string_lossy();
            n.starts_with("delilah-") && n.ends_with(".db")
        })
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    copies.sort_by_key(|c| std::cmp::Reverse(c.0));
    for (_, path) in copies.into_iter().skip(KEEP) {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

/// Record that this version opened (and migrated) the database.
pub fn after_open(home: &Path, current: &str) -> std::io::Result<()> {
    std::fs::write(marker(home), format!("{current}\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    async fn make_db(path: &Path) {
        let mut conn = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .connect()
            .await
            .unwrap();
        sqlx::query("CREATE TABLE t (v TEXT); INSERT INTO t VALUES ('hello')")
            .execute(&mut conn)
            .await
            .unwrap();
        conn.close().await.unwrap();
    }

    async fn rows(path: &Path) -> Vec<String> {
        let mut conn = SqliteConnectOptions::new()
            .filename(path)
            .read_only(true)
            .connect()
            .await
            .unwrap();
        let r: Vec<(String,)> = sqlx::query_as("SELECT v FROM t")
            .fetch_all(&mut conn)
            .await
            .unwrap();
        r.into_iter().map(|x| x.0).collect()
    }

    async fn setup() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let db = dir.path().join("delilah.db");
        make_db(&db).await;
        (dir, db)
    }

    #[tokio::test]
    async fn no_marker_copies_as_unknown() {
        let (dir, db) = setup().await;
        before_open(dir.path(), &db, "0.5.0").await;
        let copy = dir.path().join("backups/delilah-unknown.db");
        assert_eq!(rows(&copy).await, vec!["hello"]);
    }

    #[tokio::test]
    async fn marker_names_the_copy() {
        let (dir, db) = setup().await;
        std::fs::write(dir.path().join("version"), "0.4.0\n").unwrap();
        before_open(dir.path(), &db, "0.5.0").await;
        assert!(dir.path().join("backups/delilah-0.4.0.db").exists());
    }

    #[tokio::test]
    async fn same_version_makes_no_copy() {
        let (dir, db) = setup().await;
        std::fs::write(dir.path().join("version"), "0.5.0\n").unwrap();
        before_open(dir.path(), &db, "0.5.0").await;
        assert!(!dir.path().join("backups").exists());
    }

    #[tokio::test]
    async fn never_overwrites_a_copy() {
        let (dir, db) = setup().await;
        std::fs::write(dir.path().join("version"), "0.4.0\n").unwrap();
        let backups = dir.path().join("backups");
        std::fs::create_dir_all(&backups).unwrap();
        std::fs::write(backups.join("delilah-0.4.0.db"), "old").unwrap();
        before_open(dir.path(), &db, "0.5.0").await;
        assert_eq!(
            std::fs::read_to_string(backups.join("delilah-0.4.0.db")).unwrap(),
            "old"
        );
    }

    #[tokio::test]
    async fn fresh_install_makes_no_copy() {
        let dir = tempfile::TempDir::new().unwrap();
        before_open(dir.path(), &dir.path().join("delilah.db"), "0.5.0").await;
        assert!(
            !dir.path()
                .join("backups")
                .join("delilah-unknown.db")
                .exists()
        );
    }

    #[tokio::test]
    async fn prunes_to_the_three_newest() {
        let (dir, db) = setup().await;
        std::fs::write(dir.path().join("version"), "0.4.0\n").unwrap();
        let backups = dir.path().join("backups");
        std::fs::create_dir_all(&backups).unwrap();
        let now = SystemTime::now();
        for (i, v) in ["0.1.0", "0.2.0", "0.3.0"].iter().enumerate() {
            let f = std::fs::File::create(backups.join(format!("delilah-{v}.db"))).unwrap();
            f.set_modified(now - Duration::from_secs(1000 - i as u64 * 100))
                .unwrap();
        }
        before_open(dir.path(), &db, "0.5.0").await;
        let mut names: Vec<_> = std::fs::read_dir(&backups)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            vec!["delilah-0.2.0.db", "delilah-0.3.0.db", "delilah-0.4.0.db"]
        );
    }

    #[tokio::test]
    async fn marker_cannot_escape_backups() {
        let (dir, db) = setup().await;
        std::fs::write(dir.path().join("version"), "../../evil\n").unwrap();
        before_open(dir.path(), &db, "0.5.0").await;
        assert!(dir.path().join("backups/delilah-unknown.db").exists());
    }

    #[test]
    fn after_open_writes_marker() {
        let dir = tempfile::TempDir::new().unwrap();
        after_open(dir.path(), "0.5.0").unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("version")).unwrap(),
            "0.5.0\n"
        );
    }

    #[tokio::test]
    async fn copy_failure_does_not_panic() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, db) = setup().await;
        let backups = dir.path().join("backups");
        std::fs::create_dir_all(&backups).unwrap();
        // `copy` sets 0700 itself, so make the dir un-creatable through a file in the way.
        std::fs::remove_dir(&backups).unwrap();
        std::fs::write(&backups, "not a dir").unwrap();
        before_open(dir.path(), &db, "0.5.0").await;
        std::fs::set_permissions(&backups, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}
