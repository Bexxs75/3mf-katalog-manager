//! Copy of the catalog database taken right before an update is installed, so a
//! schema change of the new version can be undone by installing the old one again.
use std::path::{Path, PathBuf};

use rusqlite::Connection;

pub const KEEP: usize = 3;
const PREFIX: &str = "catalog-vor-";

pub fn file_name(version: &str) -> String {
    format!("{PREFIX}{version}.db")
}

pub fn create(conn: &Connection, dir: &Path, version: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    crate::harden_permissions(dir);
    let path = dir.join(file_name(version));
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    let mut dst = Connection::open(&path).map_err(|e| e.to_string())?;
    // A page budget covering the whole database, so the copy finishes in a single
    // step: `run_to_completion` sleeps `pause_between_pages` after every step that
    // isn't the last one, and the caller already holds the catalog lock for the
    // duration, so there is no concurrent writer for that pause to make room for.
    rusqlite::backup::Backup::new(conn, &mut dst)
        .map_err(|e| e.to_string())?
        .run_to_completion(i32::MAX, std::time::Duration::from_millis(250), None)
        .map_err(|e| e.to_string())?;
    drop(dst);
    crate::harden_permissions(&path);
    // A stray old backup that can't be deleted (e.g. permissions, a locked file on
    // Windows) must not block this install forever: the new backup above already
    // succeeded, so just leave the surplus for the next prune to try again.
    if let Err(e) = prune_preserving(dir, KEEP, Some(&path)) {
        log::warn!(target: "update", "Alte Sicherungen konnten nicht aufgeräumt werden: {e}");
    }
    Ok(path)
}

// Keep the unprotected entry point available for callers pruning outside create().
#[allow(dead_code)]
pub fn prune(dir: &Path, keep: usize) -> std::io::Result<()> {
    prune_preserving(dir, keep, None)
}

fn prune_preserving(dir: &Path, keep: usize, protected: Option<&Path>) -> std::io::Result<()> {
    // Reserve a slot for the new backup even if its timestamps look older.
    let keep = keep.saturating_sub(usize::from(protected.is_some()));
    let mut backups: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| protected != Some(p.as_path()))
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(PREFIX) && n.ends_with(".db"))
        })
        .filter_map(|p| {
            let metadata = std::fs::metadata(&p).ok()?;
            // Opening an old backup elsewhere can change its modification time.
            let time = metadata.created().or_else(|_| metadata.modified()).ok()?;
            Some((time, p))
        })
        .collect();
    backups.sort();
    let excess = backups.len().saturating_sub(keep);
    for (_, p) in backups.into_iter().take(excess) {
        std::fs::remove_file(p)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mfk-upd-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn creates_a_readable_copy_named_after_the_new_version() {
        let conn = crate::db::connect_in_memory().unwrap();
        conn.execute("INSERT INTO app_settings (key, value) VALUES ('probe', 'x')", []).unwrap();
        let dir = tmp("create");
        let path = create(&conn, &dir, "0.15.1").unwrap();
        assert_eq!(path.file_name().unwrap(), "catalog-vor-0.15.1.db");
        let copy = rusqlite::Connection::open(&path).unwrap();
        let v: String = copy.query_row("SELECT value FROM app_settings WHERE key='probe'", [], |r| r.get(0)).unwrap();
        assert_eq!(v, "x");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn keeps_only_the_newest_three() {
        let dir = tmp("prune");
        std::fs::create_dir_all(&dir).unwrap();
        let base = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        for (i, v) in ["0.15.1", "0.15.2", "0.15.3", "0.15.4"].iter().enumerate() {
            let p = dir.join(file_name(v));
            std::fs::write(&p, b"x").unwrap();
            std::fs::File::options()
                .write(true)
                .open(&p)
                .unwrap()
                .set_modified(base + std::time::Duration::from_secs(i as u64 * 60))
                .unwrap();
        }
        std::fs::write(dir.join("fremd.txt"), b"x").unwrap();
        prune(&dir, KEEP).unwrap();
        let mut left: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        left.sort();
        assert_eq!(left, ["catalog-vor-0.15.2.db", "catalog-vor-0.15.3.db", "catalog-vor-0.15.4.db", "fremd.txt"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn keeps_newest_created_backups_after_old_backups_are_modified() {
        let dir = tmp("prune-created");
        std::fs::create_dir_all(&dir).unwrap();
        let mut paths = Vec::new();
        for version in ["1", "2", "3", "4"] {
            let path = dir.join(file_name(version));
            std::fs::write(&path, b"x").unwrap();
            if std::fs::metadata(&path).unwrap().created().is_err() {
                std::fs::remove_dir_all(&dir).unwrap();
                return;
            }
            paths.push(path);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
        for path in &paths[..2] {
            std::fs::File::options().write(true).open(path).unwrap().set_modified(later).unwrap();
        }

        prune(&dir, KEEP).unwrap();

        assert!(!paths[0].exists());
        for path in &paths[1..] {
            assert!(path.exists());
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn preserves_just_created_backup_even_with_oldest_timestamps() {
        let dir = tmp("prune-protected");
        std::fs::create_dir_all(&dir).unwrap();
        let mut paths = Vec::new();
        let base = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        for (i, version) in ["1", "2", "3", "4"].iter().enumerate() {
            let path = dir.join(file_name(version));
            std::fs::write(&path, b"x").unwrap();
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(base + std::time::Duration::from_secs(i as u64 * 60))
                .unwrap();
            paths.push(path);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        prune_preserving(&dir, KEEP, Some(&paths[0])).unwrap();

        assert!(paths[0].exists());
        assert!(!paths[1].exists());
        assert!(paths[2].exists());
        assert!(paths[3].exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_backup_that_cannot_be_pruned_does_not_fail_the_install() {
        let conn = crate::db::connect_in_memory().unwrap();
        let dir = tmp("prune-stuck");
        std::fs::create_dir_all(&dir).unwrap();
        let base = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        // A directory where a backup file is expected can't be removed with
        // `remove_file` (EISDIR on Linux) - stands in for any old backup that
        // can't be deleted, e.g. because of permissions or a lock held elsewhere.
        let stuck = dir.join(file_name("0.1"));
        std::fs::create_dir(&stuck).unwrap();
        std::fs::File::open(&stuck).unwrap().set_modified(base).unwrap();
        for (i, v) in ["0.2", "0.3"].iter().enumerate() {
            let p = dir.join(file_name(v));
            std::fs::write(&p, b"x").unwrap();
            std::fs::File::options()
                .write(true)
                .open(&p)
                .unwrap()
                .set_modified(base + std::time::Duration::from_secs((i + 1) as u64 * 60))
                .unwrap();
        }
        // Adding the 4th backup pushes the count past KEEP=3, so prune() must try
        // to remove the oldest one - the stuck directory - and fail to do so.
        let path = create(&conn, &dir, "0.4").expect("a prune failure must not fail the backup itself");
        assert_eq!(path.file_name().unwrap(), "catalog-vor-0.4.db");
        assert!(stuck.exists(), "the undeletable entry must still be there");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn fails_when_the_target_cannot_be_created() {
        let conn = crate::db::connect_in_memory().unwrap();
        let dir = tmp("blocked");
        std::fs::write(&dir, b"a file, not a directory").unwrap();
        assert!(create(&conn, &dir, "0.15.1").is_err());
        std::fs::remove_file(&dir).unwrap();
    }
}
