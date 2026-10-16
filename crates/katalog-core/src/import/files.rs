//! Parsing and batch persistence shared by transport adapters.
use super::*;
use crate::{obj, stl, threemf, tagging::{self, TaggingContext}};
use crate::db::models::{FileType, MaterialRecord};
use crate::read::to_dto;
use std::collections::BTreeMap;
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResultDto {
    #[serde(skip)]
    pub duplicate_entries: Vec<DuplicateEntry>,
    pub imported: Vec<ModelFileDto>,
    pub duplicate_count: i64,
    /// Individually picked/dropped archives - NOT imported here, but handled by the
    /// frontend through the extract dialog.
    pub pending_archives: Vec<String>,
    /// Files that could not be imported, so the UI can name them.
    pub skipped: Vec<SkippedFileDto>,
}
/// Why a file was left out of an import. The UI shows a translated text per
/// reason; the detailed error only goes to the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SkipReason {
    /// The file has 0 bytes.
    Empty,
    /// Not a usable model: damaged, a different format, no geometry, or not readable.
    Invalid,
    /// The catalog could not store it - an app-side error worth a problem report.
    Failed,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedFileDto {
    pub path: String,
    pub reason: SkipReason,
}
pub fn is_ignored_import_path(path: &Path) -> bool {
    path.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
        n.starts_with('.') || n.starts_with("~$") || n.to_ascii_lowercase().ends_with(".part")
    })
}
pub fn is_supported_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            matches!(
                e.to_lowercase().as_str(),
                "3mf" | "stl" | "stp" | "step" | "obj"
            )
        })
        .unwrap_or(false)
}
pub fn compute_content_hash(path: &Path) -> CmdResult<String> {
    // Paths can come from an imported backup: `/dev/zero` would be read forever,
    // a FIFO would block the startup backfill.
    let file = crate::safe_file::open_regular(path).map_err(|e| e.to_string())?;
    compute_reader_hash(file)
}
/// Hash an already checked file descriptor without reopening its path.
pub fn compute_reader_hash(mut reader: impl std::io::Read) -> CmdResult<String> {
    use sha2::{Digest, Sha256};
    const CHUNK_SIZE: usize = 1024 * 1024;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; CHUNK_SIZE];
    loop {
        let bytes_read = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
pub fn collect_supported_files(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_symlink() {
        return;
    }
    if path.is_dir() {
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            collect_supported_files(&entry.path(), out);
        }
    } else if is_supported_extension(path) {
        out.push(path.to_path_buf());
    }
}
/// The only place where the STEP preview shows up in the metadata path. On any
/// error the file stays catalogable and just gets no automatic metadata.
#[cfg(feature = "step-preview")]
pub fn step_metadata(path: &Path) -> (Option<[f64; 3]>, Option<f64>, Option<i64>) {
    match crate::step::parse_step_file(path) {
        Ok(doc) => (
            doc.dimensions_mm,
            doc.volume_cm3,
            Some(doc.object_count as i64),
        ),
        Err(err) => {
            log::warn!(target: "step", "metadata not readable ({}): {err}", path.display());
            (None, None, None)
        }
    }
}

#[cfg(not(feature = "step-preview"))]
pub fn step_metadata(_path: &Path) -> (Option<[f64; 3]>, Option<f64>, Option<i64>) {
    (None, None, None)
}

/// A missing or unsupported filesystem timestamp never prevents import.
pub fn disk_modified_at(path: &Path) -> Option<String> {
    std::fs::metadata(path).ok()?.modified().ok()
        .map(|time| chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339())
}

pub fn read_model_file(path: &Path, display_name: Option<&str>) -> Result<NewFile, String> {
    let file_name = display_name.map(|n| n.to_string()).unwrap_or_else(|| {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unbenannt")
            .to_string()
    });
    let file_size_bytes = std::fs::metadata(path).map_err(|e| e.to_string())?.len() as i64;
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    let mut tagging_extent_mm = None;
    let (
        file_type,
        dimensions_mm,
        volume_cm3,
        object_count,
        materials,
        metadata,
        thumbnail_png,
        plate_count,
        slice_info_json,
    ) = match extension.as_deref() {
        Some("3mf") => {
            let doc = threemf::parse_3mf_file(path).map_err(|e| e.to_string())?;
            tagging_extent_mm = doc.tagging_extent_mm;
            (
                FileType::ThreeMf,
                doc.dimensions_mm,
                doc.volume_cm3,
                Some(doc.object_count as i64),
                doc.materials
                    .into_iter()
                    .map(|m| MaterialRecord {
                        name: m.name,
                        display_color: m.display_color,
                    })
                    .collect::<Vec<_>>(),
                doc.metadata,
                doc.thumbnail_png,
                doc.plate_count.map(|c| c as i64),
                doc.slice_info.and_then(|s| serde_json::to_string(&s).ok()),
            )
        }
        Some("stl") => {
            let doc = stl::parse_stl_file(path).map_err(|e| e.to_string())?;
            (
                FileType::Stl,
                doc.dimensions_mm,
                doc.volume_cm3,
                None,
                Vec::new(),
                BTreeMap::new(),
                None,
                None,
                None,
            )
        }
        Some("stp") | Some("step") => {
            let (dimensions_mm, volume_cm3, object_count) = step_metadata(path);
            (
                FileType::Stp,
                dimensions_mm,
                volume_cm3,
                object_count,
                Vec::new(),
                BTreeMap::new(),
                None,
                None,
                None,
            )
        }
        Some("obj") => {
            let doc = obj::parse_obj_file(path).map_err(|e| e.to_string())?;
            (
                FileType::Obj,
                doc.dimensions_mm,
                doc.volume_cm3,
                None,
                Vec::new(),
                BTreeMap::new(),
                None,
                None,
                None,
            )
        }
        _ => return Err("nicht unterstütztes Dateiformat".into()),
    };

    let tags = tagging::suggest_tags(&TaggingContext {
        file_name: &file_name,
        dimensions_mm: tagging_extent_mm.map(|extent| [extent; 3]).or(dimensions_mm),
        object_count,
        materials: &materials,
    });

    Ok(NewFile {
        name: file_name,
        path: path.to_string_lossy().to_string(),
        file_type,
        folder_id: None,
        origin: "local".to_string(),
        cloud_id: None,
        sync_status: "local-only".to_string(),
        file_size_bytes,
        dimensions_mm,
        volume_cm3,
        object_count,
        thumbnail_png,
        imported_at: chrono::Utc::now().to_rfc3339(),
        file_modified_at: disk_modified_at(path),
        materials,
        metadata: metadata.clone(),
        tags,
        print_status: "not_printed".to_string(),
        last_viewed_at: None,
        creator: metadata.get("Designer").cloned(),
        content_hash: None,
        render_snapshot_png: None,
        custom_image_png: None,
        source_url: None,
        license: None,
        queue_position: None,
        favorite: false,
        plate_count,
        slice_info_json,
    })
}

/// Explicit web budgets; desktop parser defaults are unaffected.
#[derive(Debug, Clone, Copy)]
pub struct WebImportLimits {
    pub max_file_bytes: u64,
    /// Hard-clamped to 512 MiB by the reader; uploads should enforce the same cap.
    pub max_mesh_file_bytes: u64,
    /// Hard-clamped to 1 GiB; includes every ZIP entry, even unused resources.
    pub max_unpacked_bytes: u64,
    /// Aggregate retained mesh budget across all referenced 3MF model entries.
    pub max_vertices: usize,
    pub max_triangles: usize,
}

impl Default for WebImportLimits {
    fn default() -> Self {
        Self { max_file_bytes: 1024 * 1024 * 1024, max_mesh_file_bytes: 512 * 1024 * 1024,
            max_unpacked_bytes: 1024 * 1024 * 1024, max_vertices: 2_000_000, max_triangles: 3_000_000 }
    }
}

/// Parse only the supplied descriptor. `path` is a logical DB path, never opened.
/// The caller verifies O_NOFOLLOW, journal identity and link count before calling.
/// Errors are stable kinds and contain no filesystem paths or parser input.
pub fn read_web_model(mut file: std::fs::File, path: &Path, name: &str, limits: WebImportLimits) -> Result<NewFile, String> {
    use std::io::{Read, Seek, SeekFrom};
    use sha2::{Digest, Sha256};
    let stat = file.metadata().map_err(|_| "read_failed")?;
    if !stat.is_file() || stat.len() == 0 { return Err("invalid".into()); }
    let is_mesh = path.extension().and_then(|s| s.to_str()).is_some_and(|s| s.eq_ignore_ascii_case("obj") || s.eq_ignore_ascii_case("stl"));
    let max_bytes = if is_mesh { limits.max_file_bytes.min(limits.max_mesh_file_bytes).min(512 * 1024 * 1024) } else { limits.max_file_bytes };
    if stat.len() > max_bytes { return Err("resource_limit".into()); }
    file.seek(SeekFrom::Start(0)).map_err(|_| "read_failed")?;
    let mut hasher = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(|_| "read_failed")?;
        if n == 0 { break; }
        count += n as u64;
        if count > max_bytes { return Err("resource_limit".into()); }
        hasher.update(&buffer[..n]);
    }
    if count != stat.len() { return Err("changed".into()); }
    let hash = format!("{:x}", hasher.finalize());
    file.seek(SeekFrom::Start(0)).map_err(|_| "read_failed")?;
    let mut bytes = Vec::new();
    if is_mesh {
        (&mut file).take(max_bytes.saturating_add(1)).read_to_end(&mut bytes).map_err(|_| "read_failed")?;
        if bytes.len() as u64 != count || format!("{:x}", Sha256::digest(&bytes)) != hash { return Err("changed".into()); }
    }
    let display_name = Some(name);
    let file_name = display_name.map(|n| n.to_string()).unwrap_or_else(|| {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unbenannt")
            .to_string()
    });
    let file_size_bytes = stat.len() as i64;
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    let mut tagging_extent_mm = None;
    let (
        file_type,
        dimensions_mm,
        volume_cm3,
        object_count,
        materials,
        metadata,
        thumbnail_png,
        plate_count,
        slice_info_json,
    ) = match extension.as_deref() {
        Some("3mf") => {
            let doc = threemf::parse_web_reader(&mut file, WebImportLimits { max_unpacked_bytes: limits.max_unpacked_bytes.min(1024 * 1024 * 1024), ..limits }).map_err(|_| "invalid")?;
            tagging_extent_mm = doc.tagging_extent_mm;
            (
                FileType::ThreeMf,
                doc.dimensions_mm,
                doc.volume_cm3,
                Some(doc.object_count as i64),
                doc.materials
                    .into_iter()
                    .map(|m| MaterialRecord {
                        name: m.name,
                        display_color: m.display_color,
                    })
                    .collect::<Vec<_>>(),
                doc.metadata,
                doc.thumbnail_png,
                doc.plate_count.map(|c| c as i64),
                doc.slice_info.and_then(|s| serde_json::to_string(&s).ok()),
            )
        }
        Some("stl") => {
            let doc = stl::parse_stl_bytes_limited(&bytes, limits.max_vertices, limits.max_triangles).map_err(|_| "invalid")?;
            (
                FileType::Stl,
                doc.dimensions_mm,
                doc.volume_cm3,
                None,
                Vec::new(),
                BTreeMap::new(),
                None,
                None,
                None,
            )
        }
        Some("stp") | Some("step") => {
            let (dimensions_mm, volume_cm3, object_count) = (None, None, None);
            (
                FileType::Stp,
                dimensions_mm,
                volume_cm3,
                object_count,
                Vec::new(),
                BTreeMap::new(),
                None,
                None,
                None,
            )
        }
        Some("obj") => {
            let doc = obj::parse_obj_bytes_limited(&bytes, limits.max_vertices, Some(limits.max_triangles as u64)).map_err(|_| "invalid")?;
            (
                FileType::Obj,
                doc.dimensions_mm,
                doc.volume_cm3,
                None,
                Vec::new(),
                BTreeMap::new(),
                None,
                None,
                None,
            )
        }
        _ => return Err("unsupported".into()),
    };

    // Detect in-place replacement while seeking through a ZIP or STEP descriptor.
    if !is_mesh {
        file.seek(SeekFrom::Start(0)).map_err(|_| "read_failed")?;
        let mut final_hasher = Sha256::new();
        let mut final_count = 0u64;
        loop {
            let n = file.read(&mut buffer).map_err(|_| "read_failed")?;
            if n == 0 { break; }
            final_count += n as u64;
            if final_count > max_bytes { return Err("resource_limit".into()); }
            final_hasher.update(&buffer[..n]);
        }
        if final_count != count || format!("{:x}", final_hasher.finalize()) != hash { return Err("changed".into()); }
    }

    let tags = tagging::suggest_tags(&TaggingContext {
        file_name: &file_name,
        dimensions_mm: tagging_extent_mm.map(|extent| [extent; 3]).or(dimensions_mm),
        object_count,
        materials: &materials,
    });

    Ok(NewFile {
        name: file_name,
        path: path.to_string_lossy().to_string(),
        file_type,
        folder_id: None,
        origin: "local".to_string(),
        cloud_id: None,
        sync_status: "local-only".to_string(),
        file_size_bytes,
        dimensions_mm,
        volume_cm3,
        object_count,
        thumbnail_png,
        imported_at: chrono::Utc::now().to_rfc3339(),
        file_modified_at: stat.modified().ok().map(|time| chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339()),
        materials,
        metadata: metadata.clone(),
        tags,
        print_status: "not_printed".to_string(),
        last_viewed_at: None,
        creator: metadata.get("Designer").cloned(),
        content_hash: Some(hash),
        render_snapshot_png: None,
        custom_image_png: None,
        source_url: None,
        license: None,
        queue_position: None,
        favorite: false,
        plate_count,
        slice_info_json,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ImportMode {
    Batched,
    Atomic,
}

/// Files stored per transaction. SQLite fsyncs on every commit, so one commit
/// per file would be slow; one commit for the whole import would hold the lock
/// for minutes.
pub const IMPORT_BATCH_SIZE: usize = 25;

/// How a batch import reaches the database. With the app's shared connection
/// every call locks it only for its own duration: the UI thread keeps
/// answering while files are read (a synchronous command waiting for the lock
/// freezes the window).
pub trait ImportDb {
    /// Hash outside the connection lock; adapters may report I/O diagnostics.
    fn hash(&mut self, path: &Path) -> CmdResult<String> {
        compute_content_hash(path)
    }
    fn batch_options(&self) -> BatchOptions { BatchOptions::default() }
    fn read_file(&mut self, path: &Path) -> CmdResult<NewFile> {
        read_model_file(path, None).map_err(Into::into)
    }
    fn report_in_flight(&self, _count: usize) {}
    fn savepoint_control(&self) -> fn(&Connection, &str) -> rusqlite::Result<()> {
        |conn, sql| conn.execute_batch(sql)
    }
    fn with_conn<R>(&mut self, f: impl FnOnce(&mut Connection) -> CmdResult<R>) -> CmdResult<R>;
}
impl ImportDb for &Mutex<Connection> {
    fn with_conn<R>(&mut self, f: impl FnOnce(&mut Connection) -> CmdResult<R>) -> CmdResult<R> {
        let mut conn = self
            .lock()
            .map_err(|_| "database lock poisoned".to_string())?;
        f(&mut conn)
    }
}
impl ImportDb for &mut Connection {
    fn with_conn<R>(&mut self, f: impl FnOnce(&mut Connection) -> CmdResult<R>) -> CmdResult<R> {
        f(self)
    }
}

/// A parsed file waiting to be stored with the next batch.
pub struct PendingImport {
    pub path: PathBuf,
    /// Set for folder imports: the root the file's subfolders are mirrored from.
    pub folder_root: Option<PathBuf>,
    pub new_file: NewFile,
}

#[cfg(any(test, feature = "test-support"))]
pub fn import_many_in_batches(
    db: &mut impl ImportDb,
    roots: Vec<PathBuf>,
    mode: ImportMode,
) -> CmdResult<ImportResultDto> {
    import_many_controlled(db, roots, mode, None)
}
#[cfg(any(test, feature = "test-support"))]
pub fn import_many_controlled(
    db: &mut impl ImportDb,
    roots: Vec<PathBuf>,
    mode: ImportMode,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> CmdResult<ImportResultDto> {
    import_many_selected(db, roots, mode, cancel, None)
}
pub fn import_many_selected(
    db: &mut impl ImportDb,
    roots: Vec<PathBuf>,
    mode: ImportMode,
    cancel: Option<&std::sync::atomic::AtomicBool>,
    selected: Option<&[PathBuf]>,
) -> CmdResult<ImportResultDto> {
    let mut seen_paths = HashSet::new();
    let mut result = ImportResultDto {
        duplicate_entries: Vec::new(),
        imported: Vec::new(),
        duplicate_count: 0,
        // Archives aren't known here: they're split off and attached by the
        // desktop compatibility callers once
        // `import_many`/`import_many_with_conn` has returned.
        pending_archives: Vec::new(),
        skipped: Vec::new(),
    };
    let mut batch = Vec::with_capacity(IMPORT_BATCH_SIZE);

    for root in roots {
        let folder_root = root.is_dir().then(|| root.clone());
        let mut candidates = Vec::new();
        collect_supported_files(&root, &mut candidates);

        for path in candidates {
            if selected.is_some_and(|paths| !paths.contains(&path)) {
                continue;
            }
            if cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Acquire)) {
                return Err(CmdError::expected("cancelled"));
            }
            let path_str = path.to_string_lossy().to_string();
            if !seen_paths.insert(path_str.clone()) {
                continue;
            }
            if mode != ImportMode::Atomic {
                match db.with_conn(|conn| {
                    db::file_exists_by_path(conn, &path_str).map_err(|e| e.to_string().into())
                }) {
                    Ok(true) => {
                        result.duplicate_count += 1;
                        result
                            .duplicate_entries
                            .push(DuplicateEntry {
                                path: path_str.clone(),
                                kind: "path".into(),
                                existing_file_id: None,
                            });
                        continue;
                    }
                    Ok(false) => {}
                    Err(e) => {
                        log::error!(target: "import", "duplicate check failed: {e}");
                        result.skip(path_str, SkipReason::Failed);
                        continue;
                    }
                }
            }
            if std::fs::metadata(&path).is_ok_and(|m| m.len() == 0) {
                log::debug!(target: "import", "übersprungen (leere Datei): {path_str}");
                result.skip(path_str, SkipReason::Empty);
                continue;
            }

            let content_hash = match db.hash(&path) {
                Ok(h) => h,
                // The host hash adapter owns fault diagnostics; retain the same
                // per-file outcome regardless of its logging policy.
                Err(e) => {
                    log::debug!(target: "import", "übersprungen (Hashing fehlgeschlagen): {path_str}: {e}");
                    result.skip(path_str, SkipReason::Invalid);
                    continue;
                }
            };
            if mode != ImportMode::Atomic {
                match db.with_conn(|conn| {
                    db::file_exists_by_hash(conn, &content_hash).map_err(|e| e.to_string().into())
                }) {
                    Ok(true) => {
                        result.duplicate_count += 1;
                        result
                            .duplicate_entries
                            .push(DuplicateEntry {
                                path: path_str.clone(),
                                kind: "hash".into(),
                                existing_file_id: None,
                            });
                        continue;
                    }
                    Ok(false) => {}
                    Err(e) => {
                        log::error!(target: "import", "duplicate check (hash) failed: {e}");
                        result.skip(path_str, SkipReason::Failed);
                        continue;
                    }
                }
            }
            let mut new_file = match db.read_file(&path) {
                Ok(f) => f,
                Err(e) => {
                    log::debug!(target: "import", "übersprungen: {path_str}: {e}");
                    result.skip(path_str, SkipReason::Invalid);
                    continue;
                }
            };
            new_file.content_hash = Some(content_hash);
            batch.push(PendingImport {
                path,
                folder_root: folder_root.clone(),
                new_file,
            });
            db.report_in_flight(batch.len());
            if mode == ImportMode::Batched && batch.len() >= IMPORT_BATCH_SIZE {
                store_batch(db, &mut batch, &mut result, mode)?;
            }
        }
    }
    if cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Acquire)) {
        return Err(CmdError::expected("cancelled"));
    }
    if mode == ImportMode::Atomic {
        let (local, _) = store_transaction_controlled(db, &mut batch, mode, cancel);
        merge_import_result(&mut result, local?.0);
    } else {
        store_batch(db, &mut batch, &mut result, mode)?;
    }
    Ok(result)
}

impl ImportResultDto {
    fn skip(&mut self, path: String, reason: SkipReason) {
        self.skipped.push(SkippedFileDto { path, reason });
    }
}

/// Stores the pending files in one transaction and empties `batch`.
pub fn store_batch(
    db: &mut impl ImportDb,
    batch: &mut Vec<PendingImport>,
    result: &mut ImportResultDto,
    mode: ImportMode,
) -> CmdResult<()> {
    store_batch_detailed(db, batch, result, mode, &mut Vec::new())
}
pub fn store_batch_detailed(
    db: &mut impl ImportDb,
    batch: &mut Vec<PendingImport>,
    result: &mut ImportResultDto,
    mode: ImportMode,
    duplicates: &mut Vec<DuplicateEntry>,
) -> CmdResult<()> {
    if batch.is_empty() {
        return Ok(());
    }
    let (attempt, commit_failed) = store_transaction(db, batch, mode);
    match attempt {
        Ok((local, committed_duplicates)) => {
            merge_import_result(result, local);
            duplicates.extend(committed_duplicates);
        }
        Err(_) if commit_failed && mode == ImportMode::Batched => {
            for pending in batch.iter_mut() {
                let (single, single_commit_failed) =
                    store_transaction(db, std::slice::from_mut(pending), mode);
                match single {
                    Ok((local, committed_duplicates)) => {
                        merge_import_result(result, local);
                        duplicates.extend(committed_duplicates);
                    }
                    Err(error) if single_commit_failed && error.expected => result.skip(
                        pending.path.to_string_lossy().into_owned(),
                        SkipReason::Failed,
                    ),
                    Err(error) => return Err(error),
                }
            }
        }
        Err(error) => return Err(error),
    }
    batch.clear();
    Ok(())
}

pub fn empty_import_result() -> ImportResultDto {
    ImportResultDto {
        duplicate_entries: Vec::new(),
        imported: Vec::new(),
        duplicate_count: 0,
        pending_archives: Vec::new(),
        skipped: Vec::new(),
    }
}
fn merge_import_result(result: &mut ImportResultDto, local: ImportResultDto) {
    result.imported.extend(local.imported);
    result.skipped.extend(local.skipped);
    result.duplicate_count += local.duplicate_count;
    result.duplicate_entries.extend(local.duplicate_entries);
}
fn database_error(error: impl std::fmt::Display) -> CmdError {
    format!("database: {error}").into()
}
fn file_statement_error(error: &DbError) -> bool {
    matches!(error, DbError::Invalid(_))
        || matches!(error,
        DbError::Sqlite(rusqlite::Error::SqliteFailure(code, _))
        if code.code == rusqlite::ErrorCode::ConstraintViolation)
}
/// Only committed results escape this function. Keeping the inputs borrowed
/// lets the caller recover a failed commit without re-reading the files.
fn store_transaction(
    db: &mut impl ImportDb,
    pending: &mut [PendingImport],
    mode: ImportMode,
) -> (
    CmdResult<(ImportResultDto, Vec<DuplicateEntry>)>,
    bool,
) {
    store_transaction_controlled(db, pending, mode, None)
}
fn store_transaction_controlled(
    db: &mut impl ImportDb,
    pending: &mut [PendingImport],
    mode: ImportMode,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> (
    CmdResult<(ImportResultDto, Vec<DuplicateEntry>)>,
    bool,
) {
    let control = db.savepoint_control();
    let options = db.batch_options();
    let mut commit_failed = false;
    let result = db.with_conn(|conn| {
        let mut local = empty_import_result();
        let mut duplicates = Vec::new();
        let tx = conn.transaction_with_behavior(options.transaction).map_err(database_error)?;
        if let Some(folder) = &options.fixed_folder {
            let valid = db::list_folders(&tx).map_err(database_error)?.iter().any(|row|
                row.id == folder.id && Path::new(&row.path) == folder.root);
            if !valid { return Err(CmdError::expected("targetMissing")); }
        }
        for item in pending {
            if tx.query_row("SELECT EXISTS(SELECT 1 FROM files WHERE path = ?1)", [&item.new_file.path], |row| row.get::<_, bool>(0)).map_err(database_error)? {
                local.duplicate_count += 1;
                duplicates.push(DuplicateEntry { path: item.new_file.path.clone(), kind: "path".into(), existing_file_id: None });
                continue;
            }
            if let Some(hash) = &item.new_file.content_hash {
                if db::file_exists_by_hash(&tx, hash).map_err(database_error)? {
                    local.duplicate_count += 1;
                    duplicates.push(DuplicateEntry { path: item.new_file.path.clone(), kind: "hash".into(), existing_file_id: None });
                    continue;
                }
            }
            control(&tx, "SAVEPOINT import_file").map_err(database_error)?;
            let stored: Result<ModelFileDto, DbError> = (|| {
                item.new_file.folder_id = if let Some(folder) = &options.fixed_folder { Some(folder.id) } else { match item.folder_root.as_deref().zip(item.path.parent()) {
                    Some((root, dir)) => Some(db::ensure_folder_path(&tx, root, dir)?),
                    None => None,
                }};
                let id = db::insert_file_within_tx(&tx, &item.new_file)?;
                let file = db::get_file(&tx, id)?.ok_or_else(|| DbError::Other("imported file missing".into()))?;
                let spools = db::list_filament_spools_with_images(&tx, options.fixed_folder.is_none())?;
                Ok(to_dto(file, &spools))
            })();
            match stored {
                Ok(dto) => {
                    control(&tx, "RELEASE import_file").map_err(database_error)?;
                    local.imported.push(dto);
                }
                Err(error) => {
                    control(&tx, "ROLLBACK TO import_file; RELEASE import_file").map_err(database_error)?;
                    if mode == ImportMode::Atomic || !file_statement_error(&error) { return Err(database_error(error)); }
                    local.skip(item.path.to_string_lossy().into_owned(), SkipReason::Failed);
                }
            }
        }
        if cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Acquire)) { return Err(CmdError::expected("cancelled")); }
        if let Err(error) = tx.commit() {
            // rusqlite rolls back on Drop after a rejected COMMIT. Never retry
            // unless the connection confirms that transaction has ended.
            if !conn.is_autocommit() {
                conn.execute_batch("ROLLBACK").map_err(database_error)?;
            }
            commit_failed = true;
            let file_related = matches!(&error, rusqlite::Error::SqliteFailure(code, _) if code.code == rusqlite::ErrorCode::ConstraintViolation);
            return Err(if file_related { CmdError::expected(format!("database: {error}")) } else { database_error(error) });
        }
        local.duplicate_entries = duplicates.clone();
        Ok((local, duplicates))
    });
    (result, commit_failed)
}


/// Applies per-job transaction/assignment options without retaining a connection lock.
pub(crate) struct ConfiguredDb<'a, D> { pub db: &'a mut D, pub options: BatchOptions }
impl<D: ImportDb> ImportDb for ConfiguredDb<'_, D> {
    fn batch_options(&self) -> BatchOptions { self.options.clone() }
    fn with_conn<R>(&mut self, f: impl FnOnce(&mut Connection) -> CmdResult<R>) -> CmdResult<R> { self.db.with_conn(f) }
    fn savepoint_control(&self) -> fn(&Connection, &str) -> rusqlite::Result<()> { self.db.savepoint_control() }
}

#[cfg(test)]
#[path = "import_batch_tests.rs"]
mod import_batch_tests;

#[cfg(test)]
mod web_reader_tests {
    use super::*;
    use std::io::Write;

    fn fixture(bytes: &[u8]) -> std::fs::File {
        let path = std::env::temp_dir().join(format!("web-reader-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let mut file = std::fs::OpenOptions::new().create_new(true).read(true).write(true).open(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        file.write_all(bytes).unwrap();
        file
    }

    #[test]
    fn web_reader_uses_handle_not_logical_path_and_hashes_same_bytes() {
        use sha2::{Digest, Sha256};
        let bytes = b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
        let doc = read_web_model(fixture(bytes), Path::new("/does/not/exist.obj"), "triangle.obj", WebImportLimits::default()).unwrap();
        assert_eq!(doc.content_hash, Some(format!("{:x}", Sha256::digest(bytes))));
        assert_eq!(doc.name, "triangle.obj");
        assert!(doc.dimensions_mm.is_some());
    }

    #[test]
    fn web_reader_rejects_small_byte_and_vertex_budgets() {
        let bytes = b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
        for limits in [WebImportLimits { max_mesh_file_bytes: 8, ..WebImportLimits::default() }, WebImportLimits { max_vertices: 2, ..WebImportLimits::default() }, WebImportLimits { max_triangles: 0, ..WebImportLimits::default() }] {
            assert!(read_web_model(fixture(bytes), Path::new("x.obj"), "x.obj", limits).is_err());
        }
    }
    #[test]
    fn web_reader_bounds_stl_and_zip_unpack_and_mesh() {
        let stl = b"solid x\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendsolid x\n";
        assert!(read_web_model(fixture(stl), Path::new("x.stl"), "x.stl", WebImportLimits { max_vertices: 2, ..WebImportLimits::default() }).is_err());
        let xml = br#"<model unit="millimeter"><resources><object id="1"><mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/></vertices><triangles><triangle v1="0" v2="1" v3="2"/></triangles></mesh></object></resources><build><item objectid="1"/></build></model>"#;
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.start_file("3D/3dmodel.model", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(xml).unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        assert!(read_web_model(fixture(&bytes), Path::new("x.3mf"), "x.3mf", WebImportLimits::default()).is_ok());
        for limits in [WebImportLimits { max_unpacked_bytes: 10, ..WebImportLimits::default() }, WebImportLimits { max_vertices: 2, ..WebImportLimits::default() }, WebImportLimits { max_triangles: 0, ..WebImportLimits::default() }] {
            assert!(read_web_model(fixture(&bytes), Path::new("x.3mf"), "x.3mf", limits).is_err());
        }
    }

}
