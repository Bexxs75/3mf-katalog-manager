//! Process-wide ownership of a catalog, independent of the replaceable SQLite file.
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub enum LockError {
    InUse,
    Io(io::Error),
}
impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InUse => write!(
                f,
                "{}: Catalog is already in use / Katalog wird bereits verwendet",
                crate::error_code::ErrorCode::CatalogInUse.as_str()
            ),
            Self::Io(e) => write!(f, "Catalog lock failed: {e}"),
        }
    }
}
impl std::error::Error for LockError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::InUse => None,
        }
    }
}
impl From<io::Error> for LockError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Never unlink the sidecar: a waiter could otherwise lock a different inode.
#[derive(Debug)]
pub struct InstanceLock {
    _file: File,
}

impl InstanceLock {
    /// Production entry point; only the exact opt-out value disables locking.
    pub fn from_env(db: &Path, owner: &str) -> Result<Option<Self>, LockError> {
        if std::env::var_os("KATALOG_LOCK").as_deref() == Some(std::ffi::OsStr::new("off")) {
            log::warn!("KATALOG_LOCK=off: catalog instance locking is disabled");
            eprintln!("WARNING: KATALOG_LOCK=off: catalog instance locking is disabled");
            return Ok(None);
        }
        Self::acquire(db, owner).map(Some)
    }

    pub fn acquire(db: &Path, owner: &str) -> Result<Self, LockError> {
        let path = Self::path_for(db)?;
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => return Err(LockError::InUse),
            Err(std::fs::TryLockError::Error(e)) => return Err(e.into()),
        }
        // Only the owner may change display metadata; stale text never grants ownership.
        file.set_len(0)?;
        let hostname = std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("COMPUTERNAME"))
            .ok()
            .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
            .or_else(|| {
                let output = std::process::Command::new("hostname").output().ok()?;
                output
                    .status
                    .success()
                    .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
            })
            .unwrap_or_else(|| "unknown".into());
        writeln!(
            file,
            "{owner}, pid={}, hostname={}, start={}",
            std::process::id(),
            hostname.trim(),
            chrono::Utc::now().to_rfc3339()
        )?;
        file.flush()?;
        Ok(Self { _file: file })
    }

    pub fn path_for(db: &Path) -> io::Result<PathBuf> {
        // Resolve aliases before deriving the sidecar, including a new desktop catalog.
        let canonical = match db.canonicalize() {
            Ok(path) => path,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let parent = db
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new("."));
                parent.canonicalize()?.join(db.file_name().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "catalog file name missing")
                })?)
            }
            Err(e) => return Err(e),
        };
        let mut path = canonical.into_os_string();
        path.push(".lock");
        Ok(PathBuf::from(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    fn fixture() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "catalog-lock-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&dir).unwrap();
        dir
    }

    #[test]
    fn exclusive_until_drop_and_metadata_survives_contention() {
        let dir = fixture();
        let db = dir.join("catalog.db");
        let lock = InstanceLock::acquire(&db, "desktop").unwrap();
        let sidecar = InstanceLock::path_for(&db).unwrap();
        assert_eq!(sidecar, dir.canonicalize().unwrap().join("catalog.db.lock"));
        let text = std::fs::read_to_string(&sidecar).unwrap();
        assert!(text.starts_with("desktop, pid="));
        assert!(matches!(
            InstanceLock::acquire(&db, "server"),
            Err(LockError::InUse)
        ));
        assert_eq!(std::fs::read_to_string(&sidecar).unwrap(), text);
        drop(lock);
        assert!(sidecar.exists());
        drop(InstanceLock::acquire(&db, "server").unwrap());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn child_probe() {
        let Some(db) = std::env::var_os("KATALOG_LOCK_TEST_DB") else {
            return;
        };
        if std::env::var_os("KATALOG_LOCK_TEST_OPTOUT").is_some() {
            assert!(InstanceLock::from_env(Path::new(&db), "server")
                .unwrap()
                .is_none());
            return;
        }
        let result = InstanceLock::acquire(Path::new(&db), "server");
        if std::env::var_os("KATALOG_LOCK_TEST_BUSY").is_some() {
            assert!(matches!(result, Err(LockError::InUse)));
        } else {
            assert!(result.is_ok());
        }
    }

    #[test]
    fn opt_out_works_in_a_child_without_changing_parent_environment() {
        let dir = fixture();
        let db = dir.join("catalog.db");
        let lock = InstanceLock::acquire(&db, "desktop").unwrap();
        let sidecar = InstanceLock::path_for(&db).unwrap();
        let before = std::fs::read(&sidecar).unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "db::instance_lock::tests::child_probe"])
            .env("KATALOG_LOCK_TEST_DB", &db)
            .env("KATALOG_LOCK_TEST_OPTOUT", "1")
            .env("KATALOG_LOCK", "off")
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(std::fs::read(sidecar).unwrap(), before);
        drop(lock);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn database_symlink_uses_the_same_sidecar() {
        let dir = fixture();
        let db = dir.join("catalog.db");
        std::fs::write(&db, b"catalog").unwrap();
        let alias = dir.join("alias.db");
        std::os::unix::fs::symlink(&db, &alias).unwrap();
        let lock = InstanceLock::acquire(&db, "desktop").unwrap();
        assert!(matches!(
            InstanceLock::acquire(&alias, "server"),
            Err(LockError::InUse)
        ));
        drop(lock);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn child_process_observes_lock_and_release() {
        let dir = fixture();
        let db = dir.join("catalog.db");
        let probe = |busy: bool| {
            let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
            cmd.args(["--exact", "db::instance_lock::tests::child_probe"])
                .env("KATALOG_LOCK_TEST_DB", &db)
                .env_remove("KATALOG_LOCK_TEST_BUSY");
            if busy {
                cmd.env("KATALOG_LOCK_TEST_BUSY", "1");
            }
            assert!(cmd.status().unwrap().success());
        };
        let lock = InstanceLock::acquire(&db, "desktop").unwrap();
        probe(true);
        drop(lock);
        probe(false);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
