//! Unpacks an uploaded zip/7z archive into ordinary, separate model uploads.
//!
//! Why separate uploads and not a sub-folder of the upload folder: everything that
//! protects the upload folder (journal entries addressed by a single file name,
//! `Directory::checked`, the file-operations protection that only allows direct
//! children of it) works on flat names. Models in a nested folder could not be
//! removed or renamed from the catalog afterwards and would be invisible to the
//! restart recovery. After unpacking, each model is therefore an upload like any
//! other and goes through the unchanged import path (parser limits, duplicate
//! detection, journal).
//!
//! Memory: zip is read entry by entry; 7z streams through the decoder, but the
//! library allocates the LZMA/LZMA2 dictionary and the PPMd model exactly as
//! declared in the archive header, so `SEVENZ_MEMORY_LIMIT` caps those before
//! anything is decoded. Nothing is ever held in memory as a whole archive.
use crate::{
    config::UploadConfig,
    upload_store::{
        clean_name, extension, is_model_extension, signature_ok, Entry, StagedModel, StoreError,
        UploadStore, MAX_PENDING_UPLOADS, STAGING_PREFIX,
    },
};
use katalog_core::{
    archive::{self, ArchiveError, ArchiveFormat, ExtractOptions, InspectStatus},
    db,
};
use serde_json::Value;
use std::{
    collections::HashSet,
    time::{Duration, Instant},
    io::Read,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
};

/// Largest dictionary / model memory ONE 7z decoder may reserve. Typical 7-Zip
/// settings use 16 to 64 MiB (ultra: 64 MiB); a crafted header can ask for 4 GiB.
pub(crate) const SEVENZ_MEMORY_LIMIT: u64 = 256 * 1024 * 1024;
/// Wall-clock limit for unpacking ONE archive: cheap to store, slow to decode.
pub(crate) const ARCHIVE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

pub(crate) struct Expanded {
    pub children: Vec<Entry>,
    pub models: u32,
    pub skipped: u32,
}

#[derive(Debug)]
pub(crate) enum Failure {
    /// The archive cannot be imported; the kind is shown to the user.
    Invalid(&'static str),
    Cancelled,
    Store(StoreError),
}
impl From<StoreError> for Failure {
    fn from(e: StoreError) -> Self {
        Failure::Store(e)
    }
}

/// Removes the staging folder on every exit path, including panics and early returns.
struct Staging(PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn is_model(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| is_model_extension(&e.to_ascii_lowercase()))
}

fn extraction_failure(error: ArchiveError) -> Failure {
    match error {
        ArchiveError::Encrypted => Failure::Invalid("archive_encrypted"),
        ArchiveError::Unsupported(_) => Failure::Invalid("archive_format_unsupported"),
        ArchiveError::Unreadable(_) => Failure::Invalid("archive_unreadable"),
        ArchiveError::LimitExceeded | ArchiveError::TooManyEntries => Failure::Invalid("archive_too_large"),
        ArchiveError::Cancelled => Failure::Cancelled,
        ArchiveError::TimedOut => Failure::Invalid("archive_timeout"),
        ArchiveError::MemoryLimit => Failure::Invalid("archive_memory_limit"),
        // A path deeper than the file system allows is a property of the archive.
        ArchiveError::Io(e) if e.raw_os_error() == Some(36) => Failure::Invalid("archive_unreadable"),
        // An error without an OS error number did not come from our disk but from
        // decoding (checksum mismatch, truncated stream): the archive is damaged.
        ArchiveError::Io(e) if e.raw_os_error().is_none() => Failure::Invalid("archive_unreadable"),
        ArchiveError::Io(e) => Failure::Store(e.into()),
    }
}

/// Test-only switches, looked up by the name of the archive so that tests running
/// in parallel do not influence each other.
#[cfg(test)]
#[derive(Clone, Default)]
pub(crate) struct Hook {
    /// Pretend the hand-over to the journal failed (e.g. no space left).
    pub fail_before_adopt: Option<StoreError>,
    /// Pause per file entry while unpacking, to make cancellation observable.
    pub delay_per_entry: Option<Duration>,
    /// Replacement for `ARCHIVE_TIMEOUT`.
    pub timeout: Option<Duration>,
}
#[cfg(test)]
pub(crate) static HOOKS: std::sync::Mutex<Vec<(String, Hook)>> = std::sync::Mutex::new(Vec::new());
#[cfg(test)]
fn hook_for(name: &str) -> Hook {
    HOOKS.lock().unwrap().iter().find(|(n, _)| n == name).map(|(_, h)| h.clone()).unwrap_or_default()
}

// Lets a test act between the end of unpacking and the hand-over to the journal.
#[cfg(test)]
thread_local! {
    pub(crate) static AFTER_EXTRACT: std::cell::RefCell<Option<Box<dyn Fn()>>> =
        const { std::cell::RefCell::new(None) };
}

fn occupied_paths(db_path: &Path) -> Result<HashSet<PathBuf>, StoreError> {
    let fail = || StoreError::new(500, "upload_database");
    let conn = db::open_read_only(db_path).map_err(|_| fail())?;
    let mut stmt = conn.prepare("SELECT path FROM files").map_err(|_| fail())?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|_| fail())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| fail())?;
    Ok(rows.into_iter().map(PathBuf::from).collect())
}

/// Regular files below `root`, sorted by path so name conflicts resolve the same way every time.
fn walk_files(root: &Path) -> std::io::Result<Vec<(PathBuf, String)>> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for item in std::fs::read_dir(&dir)? {
            let item = item?;
            let meta = std::fs::symlink_metadata(item.path())?;
            if meta.is_dir() {
                stack.push(item.path());
            } else if meta.is_file() {
                if let Some(name) = item.file_name().to_str() {
                    found.push((dir.clone(), name.to_owned()));
                }
            }
        }
    }
    found.sort();
    Ok(found)
}

fn head_of(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut head = Vec::new();
    std::fs::File::open(path)?.take(65536).read_to_end(&mut head)?;
    Ok(head)
}

pub(crate) struct Context<'a> {
    pub store: &'a Arc<UploadStore>,
    pub upload: &'a UploadConfig,
    pub db_path: &'a Path,
    pub cancel: &'a AtomicBool,
    pub job_id: &'a str,
    pub timeout: Duration,
}

/// Unpacks `entry` (state `Importing`, owned by `job_id`) and turns its models into
/// uploads. On success the journal already shows the archive as handled and the
/// returned children as `Importing`. On any failure, including cancellation and
/// timeout BEFORE the hand-over, nothing is left behind except the untouched archive.
/// Once the hand-over happened there is nothing to roll back: a later cancel leaves
/// the models as ordinary open uploads and the archive as handled. `summary(models, skipped)` is stored as the archive's result.
pub(crate) fn expand(
    ctx: &Context<'_>,
    entry: &Entry,
    summary: &dyn Fn(u32, u32) -> Value,
) -> Result<Expanded, Failure> {
    let cancelled = || ctx.cancel.load(std::sync::atomic::Ordering::Acquire);
    #[cfg(test)]
    let hook = hook_for(&entry.name);
    #[cfg(test)]
    let timeout = hook.timeout.unwrap_or(ctx.timeout);
    #[cfg(not(test))]
    let timeout = ctx.timeout;
    if cancelled() {
        return Err(Failure::Cancelled);
    }
    // Same identity check a normal import does: the file must still be the uploaded one.
    drop(ctx.store.checked_file(entry)?);
    let path = ctx.store.path(entry);
    let format = archive::detect_format(&path)
        .filter(|f| matches!(f, ArchiveFormat::Zip | ArchiveFormat::SevenZ))
        .ok_or(Failure::Invalid("archive_format_unsupported"))?;

    let inspected = archive::inspect_with(&path, is_model, Some(SEVENZ_MEMORY_LIMIT));
    match inspected.status {
        InspectStatus::Ok => {}
        InspectStatus::Encrypted => return Err(Failure::Invalid("archive_encrypted")),
        InspectStatus::Unsupported => return Err(Failure::Invalid("archive_format_unsupported")),
        InspectStatus::Unreadable => return Err(Failure::Invalid("archive_unreadable")),
        InspectStatus::MemoryLimit => return Err(Failure::Invalid("archive_memory_limit")),
        InspectStatus::TooLarge => return Err(Failure::Invalid("archive_too_large")),
        InspectStatus::NoModels => return Err(Failure::Invalid("archive_no_models")),
    }
    // The configured bound is tighter than the core's own 2 GiB.
    if inspected.unpacked_size > ctx.upload.archive_max_bytes {
        return Err(Failure::Invalid("archive_too_large"));
    }
    // More models than the open-upload limit can ever hold: no amount of waiting helps.
    if u64::from(inspected.model_count) > MAX_PENDING_UPLOADS {
        return Err(Failure::Invalid("archive_too_many_models"));
    }
    // Reserved before the first byte is written; released when this function ends,
    // by which time the models are journalled uploads and count on their own.
    let _reservation = ctx.store.begin_extraction(
        &entry.id,
        inspected.model_bytes,
        u64::from(inspected.model_count),
    )?;

    let staging_name = format!("{STAGING_PREFIX}{}", crate::upload_store::random_id()?);
    let staging_path = ctx.upload.dir.join(&staging_name);
    // Private root; the folders below it are unreachable for others as well.
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&staging_path)
        .map_err(StoreError::from)?;
    let staging = Staging(staging_path);

    let keep = |rel: &Path| {
        #[cfg(test)]
        if let Some(pause) = hook.delay_per_entry {
            std::thread::sleep(pause);
        }
        is_model(rel)
    };
    let mut options = ExtractOptions::new(ctx.upload.archive_max_bytes);
    options.deadline = Some(Instant::now() + timeout);
    options.sevenz_memory_limit = Some(SEVENZ_MEMORY_LIMIT);
    options.cancel = Some(ctx.cancel);
    options.keep = Some(&keep);
    let inside = |target: &Path| target.starts_with(&staging.0);
    // `merge`: the private staging root exists already; the extractor still never
    // writes through an existing link or over an existing file.
    archive::extract_archive_with(&path, format, &staging.0, true, &options, &inside)
        .map_err(extraction_failure)?;
    #[cfg(test)]
    AFTER_EXTRACT.with(|hook| {
        if let Some(hook) = hook.borrow().as_ref() {
            hook();
        }
    });
    if cancelled() {
        return Err(Failure::Cancelled);
    }

    let mesh_limit = ctx.upload.limits.max_mesh_file_bytes.min(512 * 1024 * 1024);
    let mut staged = Vec::new();
    for (dir, file) in walk_files(&staging.0).map_err(StoreError::from)? {
        let source = dir.join(&file);
        let ext = extension(&file);
        let Ok(name) = clean_name(&file) else { continue };
        if !is_model_extension(&ext) {
            continue;
        }
        let size = std::fs::symlink_metadata(&source).map_err(StoreError::from)?.len();
        let limit = if matches!(ext.as_str(), "stl" | "obj") {
            ctx.upload.max_bytes.min(mesh_limit)
        } else {
            ctx.upload.max_bytes
        };
        if size == 0 || size > limit {
            continue;
        }
        if !head_of(&source).is_ok_and(|head| signature_ok(&ext, &head)) {
            continue;
        }
        staged.push(StagedModel { dir, file, name });
    }
    if staged.is_empty() {
        return Err(Failure::Invalid("archive_no_models"));
    }
    if cancelled() {
        return Err(Failure::Cancelled);
    }

    #[cfg(test)]
    if let Some(error) = hook.fail_before_adopt {
        return Err(Failure::Store(error));
    }
    let occupied = occupied_paths(ctx.db_path)?;
    let models = staged.len() as u32;
    let skipped = inspected.file_count.saturating_sub(models);
    let children =
        ctx.store
            .adopt_extracted(&entry.id, ctx.job_id, &staged, &occupied, summary(models, skipped))?;
    Ok(Expanded {
        children,
        models,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(failure: Failure) -> String {
        match failure {
            Failure::Invalid(kind) => kind.to_owned(),
            Failure::Store(e) => format!("store:{}", e.kind),
            Failure::Cancelled => "cancelled".to_owned(),
        }
    }

    #[test]
    fn archive_errors_map_to_their_own_codes() {
        let damaged = std::io::Error::new(std::io::ErrorKind::InvalidData, "checksum");
        let disk_full = std::io::Error::from_raw_os_error(28);
        let too_long = std::io::Error::from_raw_os_error(36);
        for (error, expected) in [
            (ArchiveError::MemoryLimit, "archive_memory_limit"),
            (ArchiveError::TimedOut, "archive_timeout"),
            (ArchiveError::Io(damaged), "archive_unreadable"),
            (ArchiveError::Io(too_long), "archive_unreadable"),
            (ArchiveError::Io(disk_full), "store:no_space"),
            (ArchiveError::Encrypted, "archive_encrypted"),
            (ArchiveError::LimitExceeded, "archive_too_large"),
            (ArchiveError::Cancelled, "cancelled"),
        ] {
            assert_eq!(kind(extraction_failure(error)), expected);
        }
    }
}
