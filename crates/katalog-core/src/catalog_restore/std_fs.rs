//! Desktop adapter; the caller supplies a private temporary directory.
use super::*;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};

pub struct StdRestoreFs {
    directory: PathBuf,
}

impl StdRestoreFs {
    /// The directory must exist and be controlled by the caller.
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }
}

struct TempCatalog {
    conn: Option<Connection>,
    path: PathBuf,
}

impl TemporaryCatalog for TempCatalog {
    fn connection(&mut self) -> &mut Connection {
        self.conn
            .as_mut()
            .expect("temporary connection remains open")
    }
}

impl Drop for TempCatalog {
    fn drop(&mut self) {
        drop(self.conn.take());
        for suffix in ["", "-journal", "-wal", "-shm"] {
            let mut name = self.path.as_os_str().to_os_string();
            name.push(suffix);
            let _ = std::fs::remove_file(PathBuf::from(name));
        }
    }
}

impl RestoreFs for StdRestoreFs {
    fn resolve(&self, path: &Path) -> CmdResult<PathBuf> {
        crate::import::placement::resolve_path_for_sensitivity_check(path)
    }

    fn temporary_catalog(&self, bytes: &[u8]) -> CmdResult<Box<dyn TemporaryCatalog>> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = self.directory.join(format!(
            "catalog-restore-{}-{}.db",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        // Exclusive creation must precede the cleanup guard: a collision is not ours.
        let mut file = crate::safe_file::create_new_private(&path).map_err(|e| e.to_string())?;
        let mut temporary = TempCatalog { conn: None, path };
        let written = file.write_all(bytes);
        drop(file);
        written.map_err(|e| e.to_string())?;
        temporary.conn = Some(Connection::open(&temporary.path).map_err(|e| e.to_string())?);
        Ok(Box::new(temporary))
    }
}
