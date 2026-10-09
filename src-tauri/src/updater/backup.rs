//! Copy of the catalog database taken right before an update is installed, so a
//! schema change of the new version can be undone by installing the old one again.
use std::path::{Path, PathBuf};

use rusqlite::Connection;

pub const KEEP: usize = 3;
const PREFIX: &str = "Katalog-Sicherung_";
const KEEP_SCHEMA: usize = 2;

#[derive(Clone, Copy, PartialEq, Debug)]
enum BackupKind {
    Normal,
    StableOrigin,
    Schema,
}

pub fn file_name(from: &str, to: &str) -> Result<String, String> {
    if semver::Version::parse(from).is_err() || semver::Version::parse(to).is_err() {
        return Err("Ungültige Version für die Sicherung".into());
    }
    Ok(format!(
        "{PREFIX}{}_vor-Update_{from}_auf_{to}.db",
        chrono::Local::now().format("%Y-%m-%d_%H%M")
    ))
}

pub fn schema_file_name(from: i64, to: i64) -> String {
    format!(
        "{PREFIX}{}_vor-Schema-Migration_{from}_auf_{to}.db",
        chrono::Local::now().format("%Y-%m-%d_%H%M")
    )
}

pub fn create(conn: &Connection, dir: &Path, from: &str, to: &str) -> Result<PathBuf, String> {
    create_named(conn, dir, &file_name(from, to)?)
}

pub fn create_schema(conn: &Connection, dir: &Path, from: i64, to: i64) -> Result<PathBuf, String> {
    create_named(conn, dir, &schema_file_name(from, to))
}

fn create_named(conn: &Connection, dir: &Path, name: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    crate::harden_permissions(dir);
    // Reserve a unique name atomically: retries within a minute must not destroy
    // an earlier catalog copy, nor follow a pre-existing symlink.
    let mut path = dir.join(name);
    let mut suffix = 1;
    loop {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => break,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                suffix += 1;
                path = dir.join(format!("{}_{suffix}.db", name.trim_end_matches(".db")));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    crate::harden_permissions(&path);
    let copied = (|| -> Result<(), String> {
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
        Ok(())
    })();
    if let Err(error) = copied {
        // An incomplete copy must not count toward retention on a later attempt.
        let _ = std::fs::remove_file(&path);
        return Err(error);
    }
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

// Only names produced by the updater belong to its retention policy.
fn backup_kind(name: &str) -> Option<BackupKind> {
    if name.starts_with("catalog-vor-") && name.ends_with(".db") {
        return Some(BackupKind::Normal);
    }
    let re = regex::Regex::new(r"^Katalog-Sicherung_(\d{4}-\d{2}-\d{2}_\d{4})_vor-(?:Update_([0-9A-Za-z.+-]+)_auf_([0-9A-Za-z.+-]+)|Schema-Migration_(\d+)_auf_(\d+))(?:_(\d+))?\.db$").unwrap();
    let caps = re.captures(name)?;
    chrono::NaiveDateTime::parse_from_str(&caps[1], "%Y-%m-%d_%H%M").ok()?;
    if caps.get(4).is_some() {
        return Some(BackupKind::Schema);
    }
    let from = caps.get(2)?.as_str();
    let to = caps.get(3)?.as_str();
    semver::Version::parse(from).ok()?;
    semver::Version::parse(to).ok()?;
    Some(if from.contains('-') {
        BackupKind::Normal
    } else {
        BackupKind::StableOrigin
    })
}

fn prune_preserving(dir: &Path, keep: usize, protected: Option<&Path>) -> std::io::Result<()> {
    for kind in [
        BackupKind::Normal,
        BackupKind::StableOrigin,
        BackupKind::Schema,
    ] {
        let reserved = protected
            .and_then(|p| p.file_name()?.to_str())
            .and_then(backup_kind)
            == Some(kind);
        let limit = (if kind == BackupKind::Schema {
            KEEP_SCHEMA
        } else {
            keep
        })
        .saturating_sub(usize::from(reserved));
        let mut backups = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if (kind != BackupKind::StableOrigin && protected == Some(path.as_path()))
                || !entry.file_type()?.is_file()
            {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if backup_kind(&name) != Some(kind) {
                continue;
            }
            let metadata = entry.metadata()?;
            let time = metadata.created().or_else(|_| metadata.modified())?;
            let stamp = if name.starts_with(PREFIX) {
                name[PREFIX.len()..PREFIX.len() + 15].to_string()
            } else {
                chrono::DateTime::<chrono::Local>::from(time)
                    .format("%Y-%m-%d_%H%M")
                    .to_string()
            };
            backups.push((stamp, time, path));
        }
        backups.sort();
        if kind == BackupKind::StableOrigin {
            // The oldest stable-origin backup is the way back to the original
            // stable version and must never be displaced. Keep it and the newest;
            // schema backups and normal updates rotate in separate pools.
            let newest = backups.len().saturating_sub(1);
            for (index, (_, _, path)) in backups.into_iter().enumerate() {
                if index != 0 && index != newest && protected != Some(path.as_path()) {
                    std::fs::remove_file(path)?;
                }
            }
        } else {
            let excess = backups.len().saturating_sub(limit);
            for (_, _, path) in backups.into_iter().take(excess) {
                std::fs::remove_file(path)?;
            }
        }
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
    fn stable_origin_survives_schema_and_rc_updates_and_keeps_oldest_and_latest() {
        let dir = tmp("three-classes");
        std::fs::create_dir_all(&dir).unwrap();
        let names = [
            "Katalog-Sicherung_2026-10-01_0900_vor-Update_0.15.3_auf_0.16.0-2.db",
            "Katalog-Sicherung_2026-10-02_0900_vor-Schema-Migration_3_auf_4.db",
            "Katalog-Sicherung_2026-10-03_0900_vor-Update_0.16.0-2_auf_0.16.0-3.db",
            "Katalog-Sicherung_2026-10-04_0900_vor-Schema-Migration_4_auf_5.db",
            "Katalog-Sicherung_2026-10-05_0900_vor-Update_0.16.0-3_auf_0.16.0-4.db",
            "Katalog-Sicherung_2026-10-06_0900_vor-Update_0.16.0-4_auf_0.16.0.db",
            "Katalog-Sicherung_2026-10-07_0900_vor-Update_0.16.0_auf_0.17.0-1.db",
        ];
        for name in names {
            let path = dir.join(name);
            std::fs::write(&path, b"x").unwrap();
            prune_preserving(&dir, KEEP, Some(&path)).unwrap();
            assert!(
                dir.join(names[0]).exists(),
                "first stable origin must survive {name}"
            );
        }
        for name in names {
            assert!(dir.join(name).exists(), "missing {name}");
        }
        let newest =
            dir.join("Katalog-Sicherung_2026-10-08_0900_vor-Update_0.17.0_auf_0.18.0-1.db");
        std::fs::write(&newest, b"x").unwrap();
        prune_preserving(&dir, KEEP, Some(&newest)).unwrap();
        assert!(dir.join(names[0]).exists());
        assert!(newest.exists());
        assert!(!dir.join(names[6]).exists(), "middle stable origin rotates");
        for name in &names[1..6] {
            assert!(dir.join(name).exists());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn safe_names_and_collisions_preserve_the_original_copy() {
        assert!(file_name("../0.15.3", "0.16.0-2").is_err());
        assert!(file_name("0.15.3", "0.16.0-2/evil").is_err());
        let name = file_name("0.15.3", "0.16.0-2").unwrap();
        assert!(name.starts_with("Katalog-Sicherung_"));
        assert!(name.ends_with("_vor-Update_0.15.3_auf_0.16.0-2.db"));
        assert!(!name.contains([':', '/', ' ']));
        let conn = crate::db::connect_in_memory().unwrap();
        let dir = tmp("collision");
        let first = create_named(&conn, &dir, &name).unwrap();
        conn.execute("INSERT INTO app_settings VALUES ('collision', 'new')", [])
            .unwrap();
        let second = create_named(&conn, &dir, &name).unwrap();
        assert_ne!(first, second);
        assert_eq!(
            backup_kind(second.file_name().unwrap().to_str().unwrap()),
            Some(BackupKind::StableOrigin)
        );
        let saved = Connection::open(first).unwrap();
        let count: i64 = saved
            .query_row(
                "SELECT COUNT(*) FROM app_settings WHERE key='collision'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn keeps_two_schema_copies_and_recognizes_only_owned_names() {
        let dir = tmp("rollback-limit");
        std::fs::create_dir_all(&dir).unwrap();
        for day in 1..5 {
            std::fs::write(
                dir.join(format!(
                    "Katalog-Sicherung_2026-10-{day:02}_0930_vor-Schema-Migration_3_auf_4.db"
                )),
                b"x",
            )
            .unwrap();
        }
        prune(&dir, KEEP).unwrap();
        let mut names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        names.sort();
        assert_eq!(names.len(), 2);
        assert!(names[0].to_str().unwrap().contains("2026-10-03"));
        assert!(names[1].to_str().unwrap().contains("2026-10-04"));
        assert_eq!(
            backup_kind(
                "Katalog-Sicherung_2026-10-01_0930_vor-Update_0.15.3_auf_0.16.0-2_foreign.db"
            ),
            None
        );
        assert_eq!(
            backup_kind("catalog-vor-schema-3-auf-4.db"),
            Some(BackupKind::Normal)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rollback_copies_survive_normal_rotation() {
        let dir = tmp("rollback");
        std::fs::create_dir_all(&dir).unwrap();
        let rollback = "Katalog-Sicherung_2026-10-01_0930_vor-Update_0.15.3_auf_0.16.0-2.db";
        let schema = "Katalog-Sicherung_2026-10-01_0931_vor-Schema-Migration_3_auf_4.db";
        for name in [rollback, schema, "fremd.txt", "Katalog-Sicherung_fremd.db"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        for day in 2..7 {
            std::fs::write(
                dir.join(format!(
                    "Katalog-Sicherung_2026-10-{day:02}_0930_vor-Update_0.16.0-2_auf_0.16.0-3.db"
                )),
                b"x",
            )
            .unwrap();
        }
        prune(&dir, KEEP).unwrap();
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 7);
        assert!(dir.join(rollback).exists());
        assert!(dir.join(schema).exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn creates_a_readable_copy_named_after_the_new_version() {
        let conn = crate::db::connect_in_memory().unwrap();
        conn.execute("INSERT INTO app_settings (key, value) VALUES ('probe', 'x')", []).unwrap();
        let dir = tmp("create");
        let path = create(&conn, &dir, "0.15.0", "0.15.1").unwrap();
        let name = path.file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with("Katalog-Sicherung_"));
        assert!(name.ends_with("_vor-Update_0.15.0_auf_0.15.1.db"));
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
            let p = dir.join(format!("catalog-vor-{v}.db"));
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
            let path = dir.join(format!("catalog-vor-{version}.db"));
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
            let path = dir.join(format!("catalog-vor-{version}.db"));
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
    fn directories_named_like_backups_are_not_touched() {
        let conn = crate::db::connect_in_memory().unwrap();
        let dir = tmp("prune-stuck");
        std::fs::create_dir_all(&dir).unwrap();
        let base = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        // Retention owns regular backup files only, not unrelated directories.
        let stuck = dir.join("catalog-vor-0.1.db");
        std::fs::create_dir(&stuck).unwrap();
        std::fs::File::open(&stuck).unwrap().set_modified(base).unwrap();
        for (i, v) in ["0.2", "0.3"].iter().enumerate() {
            let p = dir.join(format!("catalog-vor-{v}.db"));
            std::fs::write(&p, b"x").unwrap();
            std::fs::File::options()
                .write(true)
                .open(&p)
                .unwrap()
                .set_modified(base + std::time::Duration::from_secs((i + 1) as u64 * 60))
                .unwrap();
        }
        let path = create(&conn, &dir, "0.16.0-3", "0.16.0-4").expect("a prune failure must not fail the backup itself");
        let name = path.file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with("Katalog-Sicherung_"));
        assert!(name.ends_with("_vor-Update_0.16.0-3_auf_0.16.0-4.db"));
        assert!(stuck.exists(), "the undeletable entry must still be there");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn fails_when_the_target_cannot_be_created() {
        let conn = crate::db::connect_in_memory().unwrap();
        let dir = tmp("blocked");
        std::fs::write(&dir, b"a file, not a directory").unwrap();
        assert!(create(&conn, &dir, "0.15.0", "0.15.1").is_err());
        std::fs::remove_file(&dir).unwrap();
    }
}
