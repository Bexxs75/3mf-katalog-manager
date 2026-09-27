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
    rusqlite::backup::Backup::new(conn, &mut dst)
        .map_err(|e| e.to_string())?
        .run_to_completion(5, std::time::Duration::from_millis(250), None)
        .map_err(|e| e.to_string())?;
    drop(dst);
    crate::harden_permissions(&path);
    prune(dir, KEEP).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn prune(dir: &Path, keep: usize) -> std::io::Result<()> {
    let mut backups: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(PREFIX) && n.ends_with(".db"))
        })
        .filter_map(|p| Some((std::fs::metadata(&p).ok()?.modified().ok()?, p)))
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
    fn fails_when_the_target_cannot_be_created() {
        let conn = crate::db::connect_in_memory().unwrap();
        let dir = tmp("blocked");
        std::fs::write(&dir, b"a file, not a directory").unwrap();
        assert!(create(&conn, &dir, "0.15.1").is_err());
        std::fs::remove_file(&dir).unwrap();
    }
}
