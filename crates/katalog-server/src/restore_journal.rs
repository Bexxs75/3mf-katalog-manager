//! Durable replacement decision. Before Installed recovery rolls back; afterwards
//! it finishes the new catalog's empty ledgers before opening any runtime.
use crate::config::Config;
use katalog_core::upload_fs::Directory;
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Write},
    path::Path,
};

const NAME: &str = "web-restore.json";
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum Phase {
    Prepared,
    OldMoved,
    Installed,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    pub id: String,
    pub phase: Phase,
}
pub(crate) struct Files {
    pub db: Directory,
    pub auth: Directory,
    pub backups: Directory,
    pub db_name: String,
}
pub(crate) fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}
impl Files {
    pub fn open(config: &Config) -> io::Result<Self> {
        let journal = crate::config::restore_journal_path(&config.db_path, &config.auth_file)
            .map_err(io::Error::other)?;
        let db = Directory::open(&parent(&config.db_path).canonicalize()?)?;
        match db.mkdir("backups", 0o700) {
            Ok(()) => (),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e),
        }
        let backups = db.open_dir("backups")?;
        Ok(Self {
            db,
            backups,
            auth: Directory::open(&parent(&journal).canonicalize()?)?,
            db_name: config
                .db_path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or(io::ErrorKind::InvalidInput)?
                .to_owned(),
        })
    }
    pub fn save(&self, record: &Record) -> io::Result<()> {
        atomic(&self.auth, NAME, &serde_json::to_vec(record)?)
    }
    pub fn reset_ledgers(&self) -> io::Result<()> {
        atomic(
            &self.auth,
            "web-fsops.json",
            br#"{"version":1,"intents":[],"folders":[]}"#,
        )?;
        atomic(
            &self.auth,
            "web-uploads.json",
            br#"{"entries":{},"jobs":{},"job_order":[]}"#,
        )
    }
    pub fn finish(&self) -> io::Result<()> {
        self.auth.unlink(NAME)?;
        self.auth.sync()
    }
}
pub(crate) fn atomic(dir: &Directory, name: &str, bytes: &[u8]) -> io::Result<()> {
    let tmp = format!(
        ".restore-{}.part",
        crate::upload_store::random_id().map_err(|_| io::ErrorKind::Other)?
    );
    let result = (|| {
        let mut file = dir.create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        dir.rename(&tmp, name)?;
        dir.sync()
    })();
    if result.is_err() {
        let _ = dir.unlink(&tmp);
    }
    result
}
pub(crate) fn remove_if_exists(dir: &Directory, name: &str) -> io::Result<()> {
    match dir.unlink(name) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}
fn exists(dir: &Directory, name: &str) -> io::Result<bool> {
    match dir.read(name) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}
pub(crate) fn pending(config: &Config) -> io::Result<bool> {
    let parent = parent(&config.auth_file);
    match parent.canonicalize() {
        Ok(path) => exists(&Directory::open(&path)?, NAME),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}
pub(crate) fn recover(config: &Config) -> io::Result<()> {
    if !pending(config)? {
        return Ok(());
    }
    let files = Files::open(config)?;
    let record: Record = serde_json::from_reader(files.auth.read_private(NAME, 4096)?)?;
    if record.id.len() != 32 || !record.id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(io::ErrorKind::InvalidData.into());
    }
    recover_record(&files, &record)
}
fn recover_record(files: &Files, record: &Record) -> io::Result<()> {
    let bak = format!(".bak-{}", record.id);
    let new = format!(".restore-new-{}", record.id);
    match record.phase {
        Phase::Prepared | Phase::OldMoved => {
            if exists(&files.db, &bak)? {
                // Keep the fallback until the journal removal is durable: recovery
                // itself may crash after restoring the active name.
                let mut input = files.db.read(&bak)?;
                remove_if_exists(&files.db, &new)?;
                let mut output = files.db.create(&new)?;
                io::copy(&mut input, &mut output)?;
                output.sync_all()?;
                files.db.rename(&new, &files.db_name)?;
            } else if record.phase == Phase::OldMoved || !exists(&files.db, &files.db_name)? {
                let mut input = files.backups.read(&format!("restore-{}.db", record.id))?;
                remove_if_exists(&files.db, &new)?;
                let mut output = files.db.create(&new)?;
                io::copy(&mut input, &mut output)?;
                output.sync_all()?;
                files.db.rename(&new, &files.db_name)?;
            }
        }
        Phase::Installed => {
            // A missing committed catalog is never replaced by an empty SQLite DB.
            use std::os::fd::AsRawFd;
            let file = files.db.read(&files.db_name)?;
            let conn = katalog_core::db::open_read_only(Path::new(&format!(
                "/proc/self/fd/{}",
                file.as_raw_fd()
            )))
            .map_err(|_| io::ErrorKind::InvalidData)?;
            let check: String = conn
                .query_row("PRAGMA quick_check", [], |r| r.get(0))
                .map_err(|_| io::ErrorKind::InvalidData)?;
            if check != "ok" {
                return Err(io::ErrorKind::InvalidData.into());
            }
            drop(conn);
            files.reset_ledgers()?;
        }
    }
    remove_if_exists(&files.db, &new)?;
    files.db.sync()?;
    files.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restore_recovery_every_phase_and_rename_gap() {
        for (phase, moved, installed) in [
            (Phase::Prepared, false, false),
            (Phase::Prepared, true, false),
            (Phase::OldMoved, true, false),
            (Phase::OldMoved, true, true),
            (Phase::Installed, true, true),
        ] {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../.target")
                .join(format!(
                    "restore-recovery-{}",
                    crate::upload_store::random_id().unwrap()
                ));
            std::fs::create_dir_all(path.join("backups")).unwrap();
            let path = path.canonicalize().unwrap();
            let files = Files {
                db: Directory::open(&path).unwrap(),
                auth: Directory::open(&path).unwrap(),
                backups: Directory::open(&path.join("backups")).unwrap(),
                db_name: "catalog.db".into(),
            };
            let record = Record {
                id: "0123456789abcdef0123456789abcdef".into(),
                phase,
            };
            let old = katalog_core::db::connect(&path.join("catalog.db")).unwrap();
            let new_path = path.join(".restore-new-0123456789abcdef0123456789abcdef");
            let new = katalog_core::db::connect(&new_path).unwrap();
            katalog_core::db::test_insert_minimal_file(&new, "/models/new.stl", None).unwrap();
            drop((old, new));
            atomic(&files.auth, "web-fsops.json", b"old pending intents").unwrap();
            files.save(&record).unwrap();
            if moved {
                files
                    .db
                    .rename("catalog.db", ".bak-0123456789abcdef0123456789abcdef")
                    .unwrap();
            }
            if installed {
                files
                    .db
                    .rename(
                        ".restore-new-0123456789abcdef0123456789abcdef",
                        "catalog.db",
                    )
                    .unwrap();
            }
            recover_record(&files, &record).unwrap();
            let restored = katalog_core::db::open_read_only(&path.join("catalog.db")).unwrap();
            assert_eq!(
                restored
                    .query_row("SELECT count(*) FROM files", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                if phase == Phase::Installed { 1 } else { 0 }
            );
            drop(restored);
            let ledger = std::fs::read(path.join("web-fsops.json")).unwrap();
            if phase == Phase::Installed {
                assert!(katalog_core::fsops::Journal::open(&path)
                    .unwrap()
                    .is_empty());
            } else {
                assert_eq!(ledger, b"old pending intents");
            }
            assert!(!path.join(NAME).exists());
            std::fs::remove_dir_all(path).unwrap();
        }
    }
    #[test]
    fn restore_recovery_falls_back_to_snapshot_without_bak() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.target")
            .join(format!(
                "restore-fallback-{}",
                crate::upload_store::random_id().unwrap()
            ));
        std::fs::create_dir_all(path.join("backups")).unwrap();
        let path = path.canonicalize().unwrap();
        let files = Files {
            db: Directory::open(&path).unwrap(),
            auth: Directory::open(&path).unwrap(),
            backups: Directory::open(&path.join("backups")).unwrap(),
            db_name: "catalog.db".into(),
        };
        let record = Record {
            id: "0123456789abcdef0123456789abcdef".into(),
            phase: Phase::OldMoved,
        };
        let backup = path.join("backups/restore-0123456789abcdef0123456789abcdef.db");
        drop(katalog_core::db::connect(&backup).unwrap());
        files.save(&record).unwrap();
        recover_record(&files, &record).unwrap();
        drop(katalog_core::db::open_read_only(&path.join("catalog.db")).unwrap());
        assert!(!path.join(NAME).exists());
        std::fs::remove_dir_all(path).unwrap();
    }
}
