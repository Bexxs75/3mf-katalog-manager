//! Desktop-compatible reads. Mutations remain with the caller.
use super::*;

#[derive(Debug, Clone, Copy, Default)]
pub struct StdDateRenameFs;

impl DateRenameFs for StdDateRenameFs {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }

    fn entry(&self, path: &Path) -> io::Result<()> {
        std::fs::symlink_metadata(path).map(|_| ())
    }

    fn modified(&self, path: &Path) -> io::Result<SystemTime> {
        std::fs::metadata(path)?.modified()
    }
}
