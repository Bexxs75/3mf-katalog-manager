use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use super::{
    formats, is_blocked_path, origin, safe_relative_path, ArchiveError, ArchiveFormat, EntryKind,
    EntryMeta, MAX_ENTRIES,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtractStats {
    pub written_files: u32,
    /// When merging: the file already existed and was NOT touched.
    pub existing_skipped: u32,
    /// Unsafe or impossible entries (zip slip, links, protected target paths, ...).
    pub unsafe_skipped: u32,
    /// Blocked file types (executables, shortcuts, ...).
    pub blocked_skipped: u32,
}

/// Result of a successful extraction. Keeps the list of all paths created in
/// this run, so the caller can still clean up completely after a LATER error
/// (e.g. in the database).
#[derive(Debug)]
pub struct Extraction {
    pub stats: ExtractStats,
    created: Vec<PathBuf>,
}

impl Extraction {
    /// Removes everything this run created - in reverse order, so folders are removed
    /// after their content. Files/folders that existed before are never touched.
    pub fn rollback(self) {
        remove_created(&self.created);
    }
}

fn remove_created(created: &[PathBuf]) {
    for path in created.iter().rev() {
        match fs::symlink_metadata(path) {
            Ok(meta) if meta.is_dir() => {
                let _ = fs::remove_dir(path);
            }
            Ok(_) => {
                let _ = fs::remove_file(path);
            }
            Err(_) => {}
        }
    }
}

/// The only place that writes to disk while extracting.
pub(super) struct Extractor<'g> {
    root: PathBuf,
    byte_limit: u64,
    written_bytes: u64,
    entries_seen: usize,
    /// `false` = the path is in a protected area (decided by the caller, see
    /// `commands::reject_if_sensitive_path`).
    guard: &'g dyn Fn(&Path) -> bool,
    /// Origin mark of the archive, copied onto every file.
    origin_mark: Option<Vec<u8>>,
    created: Vec<PathBuf>,
    stats: ExtractStats,
}

impl<'g> Extractor<'g> {
    fn new(
        root: &Path,
        merge: bool,
        byte_limit: u64,
        guard: &'g dyn Fn(&Path) -> bool,
        origin_mark: Option<Vec<u8>>,
    ) -> Result<Self, ArchiveError> {
        if !guard(root) {
            return Err(ArchiveError::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("Zielordner liegt in einem geschuetzten Bereich: {}", root.display()),
            )));
        }
        let mut extractor = Extractor {
            root: root.to_path_buf(),
            byte_limit,
            written_bytes: 0,
            entries_seen: 0,
            guard,
            origin_mark,
            created: Vec::new(),
            stats: ExtractStats::default(),
        };
        match fs::symlink_metadata(root) {
            Ok(meta) if merge && meta.is_dir() && !meta.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(ArchiveError::Io(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    format!("Zielordner existiert bereits: {}", root.display()),
                )))
            }
            Err(_) => {
                fs::create_dir(root)?;
                extractor.created.push(root.to_path_buf());
            }
        }
        Ok(extractor)
    }

    /// Creates `rel` (relative to `root`) level by level. `false` if a level already
    /// exists as a file or symlink or is protected - then NOTHING may be written below
    /// it (otherwise a user-created symlink in the merge folder could redirect the
    /// target).
    fn ensure_dir(&mut self, rel: &Path) -> Result<bool, ArchiveError> {
        let mut current = self.root.clone();
        for component in rel.components() {
            current.push(component);
            match fs::symlink_metadata(&current) {
                Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => return Ok(false),
                Ok(_) => {}
                Err(_) => {
                    if !(self.guard)(&current) {
                        return Ok(false);
                    }
                    fs::create_dir(&current)?;
                    self.created.push(current.clone());
                }
            }
        }
        Ok(true)
    }

    /// Target path for a file entry, or `None` if it is skipped. Folder entries are
    /// created here directly. Counts EVERY entry against `MAX_ENTRIES` - even though
    /// `inspect` already checked, the archive may have been replaced since.
    pub(super) fn prepare(&mut self, meta: &EntryMeta) -> Result<Option<PathBuf>, ArchiveError> {
        self.entries_seen += 1;
        if self.entries_seen > MAX_ENTRIES {
            return Err(ArchiveError::TooManyEntries);
        }
        let Some(rel) = safe_relative_path(&meta.name) else {
            self.stats.unsafe_skipped += 1;
            return Ok(None);
        };
        if meta.kind == EntryKind::Other {
            self.stats.unsafe_skipped += 1;
            return Ok(None);
        }
        if is_blocked_path(&rel) {
            self.stats.blocked_skipped += 1;
            return Ok(None);
        }
        if meta.kind == EntryKind::Directory {
            if !self.ensure_dir(&rel)? {
                self.stats.unsafe_skipped += 1;
            }
            return Ok(None);
        }
        if meta.encrypted {
            return Err(ArchiveError::Encrypted);
        }
        if let Some(parent) = rel.parent() {
            if !self.ensure_dir(parent)? {
                self.stats.unsafe_skipped += 1;
                return Ok(None);
            }
        }
        let target = self.root.join(&rel);
        if fs::symlink_metadata(&target).is_ok() {
            self.stats.existing_skipped += 1;
            return Ok(None);
        }
        if !(self.guard)(&target) {
            self.stats.unsafe_skipped += 1;
            return Ok(None);
        }
        Ok(Some(target))
    }

    /// Writes streamed and counts the ACTUALLY written bytes against the budget.
    /// `create_new` guarantees that nothing is overwritten and no symlink at the
    /// target is followed.
    pub(super) fn write_from(&mut self, target: &Path, reader: &mut dyn Read) -> Result<(), ArchiveError> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        // Never executable, whatever permissions the archive brings.
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o644);
        let mut file = options.open(target)?;
        self.created.push(target.to_path_buf());
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            self.written_bytes += n as u64;
            if self.written_bytes > self.byte_limit {
                return Err(ArchiveError::LimitExceeded);
            }
            file.write_all(&buf[..n])?;
        }
        drop(file);
        self.finish_file(target);
        Ok(())
    }

    fn finish_file(&mut self, target: &Path) {
        if let Some(mark) = &self.origin_mark {
            origin::apply(target, mark);
        }
        self.stats.written_files += 1;
    }

    /// Counts an entry a format adapter may not write after `prepare` after all
    /// (e.g. a RAR reference entry without data).
    pub(super) fn skip_unsafe(&mut self) {
        self.stats.unsafe_skipped += 1;
    }

    /// Checks whether `size` bytes fit into the remaining budget. Called BEFORE writing to prevent an overrun.
    pub(super) fn ensure_budget_for(&self, size: u64) -> Result<(), ArchiveError> {
        if self.written_bytes.saturating_add(size) > self.byte_limit {
            return Err(ArchiveError::LimitExceeded);
        }
        Ok(())
    }
}

/// Extracts `archive` to `dest`. With `merge == false`, `dest` must not exist
/// yet. `guard` is asked for `dest` and every path to be created (`false` =
/// protected, don't create). On ANY error, everything this run created is removed
/// before the error is returned.
pub fn extract_archive(
    archive: &Path,
    format: ArchiveFormat,
    dest: &Path,
    merge: bool,
    byte_limit: u64,
    guard: &dyn Fn(&Path) -> bool,
) -> Result<Extraction, ArchiveError> {
    let mut extractor = Extractor::new(dest, merge, byte_limit, guard, origin::read(archive))?;
    match formats::extract(archive, format, &mut extractor) {
        Ok(()) => Ok(Extraction {
            stats: extractor.stats,
            created: extractor.created,
        }),
        Err(e) => {
            remove_created(&extractor.created);
            Err(e)
        }
    }
}
