//! Private drag copies outlive the native session. A process-held file lock prevents
//! another process from collecting its files, even when a drag lasts more than a day.
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const RETENTION: Duration = Duration::from_secs(24 * 60 * 60);
static NEXT: AtomicU64 = AtomicU64::new(0);

pub(crate) struct DragStorage {
    parent: PathBuf,
    root: PathBuf,
    _lease: File,
}

fn private_dir(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

fn unique_dir(parent: &Path, prefix: &str) -> io::Result<PathBuf> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    loop {
        let path = parent.join(format!("{prefix}-{}-{stamp}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        match private_dir(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
}

impl DragStorage {
    // The app-specific cache directory separates normal and preview identifiers.
    pub(crate) fn new(app_cache: &Path) -> io::Result<Self> {
        fs::create_dir_all(app_cache)?;
        let parent = app_cache.join("native-drag");
        match private_dir(&parent) {
            Ok(()) => (),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e),
        }
        let meta = fs::symlink_metadata(&parent)?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err(io::Error::other("invalid drag directory"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&parent, fs::Permissions::from_mode(0o700))?;
        }
        Self::collect(&parent, SystemTime::now());
        let root = unique_dir(&parent, "instance")?;
        let lease = OpenOptions::new().read(true).write(true).create_new(true).open(root.join("lease"))?;
        lease.try_lock().map_err(io::Error::other)?;
        Ok(Self { parent, root, _lease: lease })
    }

    pub(crate) fn session(&self) -> io::Result<PathBuf> {
        unique_dir(&self.root, "drag")
    }

    fn collect(parent: &Path, now: SystemTime) {
        let Ok(entries) = fs::read_dir(parent) else { return };
        for entry in entries.flatten() {
            if !entry.file_name().to_string_lossy().starts_with("instance-") { continue; }
            let path = entry.path();
            let Ok(meta) = fs::symlink_metadata(&path) else { continue };
            if !meta.is_dir() || meta.file_type().is_symlink() { continue; }
            let old = meta.modified().ok().and_then(|time| now.duration_since(time).ok()).is_some_and(|age| age > RETENTION);
            if !old { continue; }
            let lease_path = path.join("lease");
            let Ok(meta) = fs::symlink_metadata(&lease_path) else { continue };
            if !meta.is_file() || meta.file_type().is_symlink() { continue; }
            let Ok(lease) = OpenOptions::new().read(true).write(true).open(&lease_path) else { continue };
            if lease.try_lock().is_err() { continue; }
            // No process reuses an instance directory. Close before removal for Windows.
            drop(lease);
            if let Err(e) = fs::remove_dir_all(&path) {
                log::warn!(target: "drag", "orphaned drag directory not removed: {e}");
            }
        }
    }

    pub(crate) fn cleanup(&self) {
        Self::collect(&self.parent, SystemTime::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orphaned_sessions_between_six_and_twenty_four_hours_survive() {
        let base = unique_dir(&std::env::temp_dir(), "drag-test").unwrap();
        let storage = DragStorage::new(&base).unwrap();
        let session = storage.session().unwrap();
        let parent = storage.parent.clone();
        let now = SystemTime::now();
        drop(storage);
        DragStorage::collect(&parent, now + Duration::from_secs(12 * 60 * 60));
        assert!(session.exists());
        DragStorage::collect(&parent, now + RETENTION + Duration::from_secs(10));
        assert!(!session.exists());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn sequential_drags_and_parallel_instances_keep_every_source() {
        let base = unique_dir(&std::env::temp_dir(), "drag-test").unwrap();
        let normal = base.join("normal");
        let first = DragStorage::new(&normal).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&first.root).unwrap().permissions().mode() & 0o777, 0o700);
            assert_eq!(fs::metadata(&first.parent).unwrap().permissions().mode() & 0o777, 0o700);
        }
        let a = first.session().unwrap().join("a.stl");
        fs::write(&a, b"first").unwrap();
        let b = first.session().unwrap().join("a.stl");
        fs::write(&b, b"second").unwrap();
        let second = DragStorage::new(&normal).unwrap();
        let preview = DragStorage::new(&base.join("preview")).unwrap();
        let later = SystemTime::now() + RETENTION + Duration::from_secs(10);
        DragStorage::collect(&first.parent, later);
        assert_eq!(fs::read(&a).unwrap(), b"first");
        assert_eq!(fs::read(&b).unwrap(), b"second");
        assert!(second.root.exists());
        assert!(preview.root.exists());
        let old_root = first.root.clone();
        drop(first);
        DragStorage::collect(&second.parent, SystemTime::now());
        assert!(old_root.exists(), "recent inactive copies survive exit");
        DragStorage::collect(&second.parent, later);
        assert!(!old_root.exists(), "only old, unlocked own instances are collected");
        assert!(second.root.exists());
        assert!(preview.root.exists());
        drop(second);
        drop(preview);
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn unknown_directories_and_symlinks_are_never_collected() {
        let base = unique_dir(&std::env::temp_dir(), "drag-test").unwrap();
        let storage = DragStorage::new(&base).unwrap();
        let unknown = storage.parent.join("instance-unknown");
        fs::create_dir(&unknown).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&unknown, storage.parent.join("instance-link")).unwrap();
        DragStorage::collect(&storage.parent, SystemTime::now() + RETENTION + Duration::from_secs(10));
        assert!(unknown.exists());
        #[cfg(unix)]
        assert!(storage.parent.join("instance-link").is_symlink());
        drop(storage);
        fs::remove_dir_all(base).unwrap();
    }
}
