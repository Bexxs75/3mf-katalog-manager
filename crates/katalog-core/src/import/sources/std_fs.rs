//! Desktop filesystem adapter. This is not a handle-confined server backend.
use super::*;

pub struct StdSourceFs;

fn kind(value: std::fs::FileType) -> SourceKind {
    if value.is_symlink() {
        SourceKind::Symlink
    } else if value.is_dir() {
        SourceKind::Directory
    } else if value.is_file() {
        SourceKind::File
    } else {
        SourceKind::Other
    }
}

impl SourceFs for StdSourceFs {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        path.canonicalize()
    }

    fn stat(&self, path: &Path, follow_links: bool) -> io::Result<SourceMetadata> {
        let meta = if follow_links {
            std::fs::metadata(path)
        } else {
            std::fs::symlink_metadata(path)
        }?;
        Ok(SourceMetadata {
            kind: kind(meta.file_type()),
            len: meta.len(),
        })
    }

    fn list(&self, path: &Path) -> io::Result<SourceEntries<'_>> {
        Ok(Box::new(std::fs::read_dir(path)?.map(|entry| {
            let entry = entry?;
            Ok(SourceEntry {
                path: entry.path(),
                kind: entry.file_type().map(kind),
            })
        })))
    }

    fn content_hash(&self, path: &Path) -> CmdResult<String> {
        files::compute_content_hash(path)
    }
}
