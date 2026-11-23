//! Desktop filesystem adapter, without server handle confinement or model moves.
use super::*;
use crate::import::sources::{SourceFs, StdSourceFs};

pub struct StdFolderTrashFs;

impl FolderTrashFs for StdFolderTrashFs {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        StdSourceFs.canonicalize(path)
    }
    fn kind(&self, path: &Path, follow_links: bool) -> io::Result<SourceKind> {
        StdSourceFs.stat(path, follow_links).map(|m| m.kind)
    }
    fn list(&self, path: &Path) -> io::Result<SourceEntries<'_>> {
        StdSourceFs.list(path)
    }
    fn remove_empty_dir(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_dir(path)
    }
}
