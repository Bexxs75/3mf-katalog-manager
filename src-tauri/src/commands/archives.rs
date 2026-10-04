//! Tauri commands for extracting INDIVIDUALLY imported archives (file dialog,
//! drag & drop). The archive logic itself lives in `crate::archive`; here only
//! flow control, catalog import and the optional deletion of the original
//! happen. The folder import deliberately extracts nothing.

use super::*;
use super::files::{is_supported_extension, SkippedFileDto};
#[cfg(test)]
use super::files::import_many_with_conn_atomic;

use serde::Deserialize;

use crate::archive::{self, InspectStatus, MAX_UNPACKED_BYTES};

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ArchiveStatusDto {
    Ok,
    NoModels,
    TooLarge,
    Encrypted,
    Unreadable,
    Unsupported,
}

impl From<InspectStatus> for ArchiveStatusDto {
    fn from(status: InspectStatus) -> Self {
        match status {
            InspectStatus::Ok => ArchiveStatusDto::Ok,
            InspectStatus::NoModels => ArchiveStatusDto::NoModels,
            InspectStatus::TooLarge => ArchiveStatusDto::TooLarge,
            InspectStatus::Encrypted => ArchiveStatusDto::Encrypted,
            InspectStatus::Unreadable => ArchiveStatusDto::Unreadable,
            InspectStatus::Unsupported => ArchiveStatusDto::Unsupported,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveInfoDto {
    pub path: String,
    pub suggested_folder_name: String,
    pub model_count: u32,
    pub entry_count: u32,
    pub unpacked_size: u64,
    pub file_size: u64,
    pub modified_unix_ms: i64,
    pub status: ArchiveStatusDto,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ConflictMode {
    New,
    Merge,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveRequest {
    pub path: String,
    pub folder_name: String,
    pub on_conflict: ConflictMode,
    pub expected_size: u64,
    pub expected_modified_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ArchiveModels {
    pub imported: Vec<ModelFileDto>,
    pub duplicates: Vec<serde_json::Value>,
    pub skipped: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveOutcomeDto {
    #[serde(skip)]
    pub models: ArchiveModels,
    pub path: String,
    pub extracted_to: Option<String>,
    pub stripped_root: Option<String>,
    pub existing_skipped: u32,
    pub unsafe_skipped: u32,
    pub blocked_skipped: u32,
    pub archive_deleted: bool,
    pub delete_error: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveImportResultDto {
    pub imported: Vec<ModelFileDto>,
    pub duplicate_count: i64,
    pub archives: Vec<ArchiveOutcomeDto>,
    /// Extracted files that could not be imported.
    pub skipped: Vec<SkippedFileDto>,
}

/// Folders the user picked in the native folder dialog (`pick_folder_path`) or
/// created during catalog setup (`create_catalog_dir`). Together with the
/// catalog folders the only allowed extraction targets - so a
/// compromised frontend can't slip in an arbitrary target (e.g. the home directory).
#[derive(Default)]
pub struct ApprovedTargets(std::sync::Mutex<HashSet<PathBuf>>);

impl ApprovedTargets {
    pub(crate) fn clear(&self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    pub(crate) fn approve(&self, path: &Path) {
        if let Ok(mut set) = self.0.lock() {
            set.insert(path.to_path_buf());
        }
    }

    pub(crate) fn contains(&self, path: &Path) -> bool {
        self.0.lock().map(|set| set.contains(path)).unwrap_or(false)
    }
}

/// `true` if `target` is exactly a catalog folder or was picked in the app.
pub(crate) fn target_is_approved(conn: &Connection, approved: &ApprovedTargets, target: &Path) -> bool {
    if approved.contains(target) {
        return true;
    }
    let target = target.to_string_lossy();
    db::list_folders(conn)
        .map(|folders| folders.iter().any(|f| f.path == target))
        .unwrap_or(false)
}

/// Rejects `path` if it CONTAINS a protected directory (is an ancestor).
/// Otherwise e.g. the home directory (ancestor of ~/.ssh) or `/` could be a merge
/// target, and entries like `.bash_profile` would land there.
fn reject_if_ancestor_of_sensitive(path: &Path, expanded_sensitive: &[PathBuf]) -> CmdResult<()> {
    let resolved = resolve_path_for_sensitivity_check(path)?;
    match expanded_sensitive
        .iter()
        .find(|dir| **dir != resolved && dir.starts_with(&resolved))
    {
        Some(dir) => Err(format!(
            "Zielpfad enthaelt ein geschuetztes Verzeichnis ({}) und wird abgelehnt",
            dir.display()
        )
        .into()),
        None => Ok(()),
    }
}

/// All target folder checks that consume or write NOTHING.
pub(super) fn check_target_location(target_dir: &Path, expanded_sensitive: &[PathBuf]) -> CmdResult<()> {
    if !target_dir.is_dir() {
        return Err(CmdError::expected(format!("Zielordner existiert nicht: {}", target_dir.display())));
    }
    reject_if_sensitive_path_expanded(target_dir, expanded_sensitive)?;
    reject_if_ancestor_of_sensitive(target_dir, expanded_sensitive)
}

/// Size and modification time (ms) - an "unchanged since inspect?" check before deleting the original.
fn file_fingerprint(path: &Path) -> Option<(u64, i64)> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis() as i64;
    Some((meta.len(), modified))
}

fn path_exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

pub(crate) fn inspect_one(path: &Path) -> ArchiveInfoDto {
    let summary = archive::inspect(path, is_supported_extension);
    let (file_size, modified_unix_ms, status) = match file_fingerprint(path) {
        Some((size, modified)) => (size, modified, summary.status.into()),
        None => (0, 0, ArchiveStatusDto::Unreadable),
    };
    ArchiveInfoDto {
        path: path.to_string_lossy().to_string(),
        suggested_folder_name: archive::folder_name_for(path),
        model_count: summary.model_count,
        entry_count: summary.entry_count,
        unpacked_size: summary.unpacked_size,
        file_size,
        modified_unix_ms,
        status,
    }
}

/// First free name `name`, `name (2)`, `name (3)`, ... under `target_dir`.
pub(crate) fn unique_destination(target_dir: &Path, folder_name: &str) -> PathBuf {
    let first = target_dir.join(folder_name);
    if !path_exists(&first) {
        return first;
    }
    (2u32..)
        .map(|n| target_dir.join(format!("{folder_name} ({n})")))
        .find(|candidate| !path_exists(candidate))
        .expect("unendliche Folge liefert immer einen freien Namen")
}

/// Imports a freshly extracted folder into the catalog: the existing folder
/// import plus attaching it below the target catalog folder. `created_dirs` are
/// the folders the extraction made; if nothing new reached the catalog, the
/// caller removes them from disk, so their catalog rows go as well.
#[cfg(test)]
pub(crate) fn import_extracted_dir(conn: &mut Connection, dir: &Path, created_dirs: &[PathBuf]) -> CmdResult<ImportResultDto> {
    let known_max_id = db::max_folder_id(conn).map_err(|e| e.to_string())?;
    let result = if created_dirs.is_empty() { import_many_with_conn_atomic(conn, vec![dir.to_path_buf()])? }
        else { super::files::import_many_selected(&mut &mut *conn, vec![dir.to_path_buf()], super::files::ImportMode::Atomic, None, Some(created_dirs))? };
    if result.imported.is_empty() {
        db::remove_new_folder_rows(conn, created_dirs, known_max_id).map_err(|e| e.to_string())?;
        return Ok(result);
    }
    // Only cosmetic (folder tree) - an error here must not undo the successful import.
    if let Err(e) = db::attach_folder_to_parent_by_path(conn, dir) {
        log::error!(target: "archive", "mounting {} failed: {e}", dir.display());
    }
    Ok(result)
}

/// Fixed parameters of one `extract_archives` run.
struct ExtractContext<'a> {
    cancel: Option<&'a std::sync::atomic::AtomicBool>,
    expanded_sensitive: &'a [PathBuf],
    target_dir: &'a Path,
    delete_archive: bool,
}

fn extract_one(
    ctx: &ExtractContext<'_>,
    request: &ArchiveRequest,
    result: &mut ArchiveImportResultDto,
    import_dir: &mut impl FnMut(&Path, &[PathBuf]) -> CmdResult<ImportResultDto>,
    on_progress: &mut impl FnMut(&str, &'static str),
) -> ArchiveOutcomeDto {
    let mut outcome = ArchiveOutcomeDto {
        path: request.path.clone(),
        ..Default::default()
    };
    let archive_path = Path::new(&request.path);
    let Some(format) = archive::detect_format(archive_path) else {
        outcome.error = Some("Archivformat wird nicht unterstuetzt".to_string());
        return outcome;
    };
    // The name comes from the frontend - sanitize again, never starting with ".".
    let clean_name = archive::safe_folder_name(&request.folder_name);
    let folder_name = if clean_name.is_empty() {
        archive::folder_name_for(archive_path)
    } else {
        clean_name
    };
    let merge = request.on_conflict == ConflictMode::Merge;
    let dest = if merge {
        ctx.target_dir.join(&folder_name)
    } else {
        unique_destination(ctx.target_dir, &folder_name)
    };
    if let Err(e) = reject_if_sensitive_path_expanded(&dest, ctx.expanded_sensitive)
        .and_then(|()| reject_if_ancestor_of_sensitive(&dest, ctx.expanded_sensitive))
    {
        outcome.error = Some(e.message);
        return outcome;
    }

    on_progress(&request.path, "extracting");
    // Every single new path is checked against the protection list, not just
    // `dest`: second line of defense in case a protected folder below `dest` only
    // appears after the check above.
    let guard = |p: &Path| !ctx.cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Acquire)) && reject_if_sensitive_path_expanded(p, ctx.expanded_sensitive).is_ok();
    let extraction = match archive::extract_archive(archive_path, format, &dest, merge, MAX_UNPACKED_BYTES, &guard) {
        Ok(extraction) => extraction,
        Err(e) => {
            outcome.error = Some(if ctx.cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Acquire)) { "cancelled".into() } else { e.to_string() });
            return outcome;
        }
    };
    outcome.existing_skipped = extraction.stats.skipped_entries.iter().filter(|(_, reason)| reason == "existing").count() as u32;
    outcome.unsafe_skipped = extraction.stats.skipped_entries.iter().filter(|(_, reason)| reason == "unsafe").count() as u32;
    outcome.blocked_skipped = extraction.stats.skipped_entries.iter().filter(|(_, reason)| reason == "blocked").count() as u32;

    outcome.models.skipped = extraction.stats.skipped_entries.iter().map(|(path, reason)| serde_json::json!({"entryPath":path,"reason":reason})).collect();
    if ctx.cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Acquire)) {
        extraction.rollback(); outcome.error = Some("cancelled".into()); return outcome;
    }
    on_progress(&request.path, "importing");
    let imported = match import_dir(&dest, extraction.created_paths()) {
        Ok(imported) => imported,
        Err(e) => {
            extraction.rollback();
            outcome.error = Some(e.message);
            return outcome;
        }
    };
    let entry_path = |path: &str| {
        let rel = Path::new(path).strip_prefix(&dest).unwrap_or(Path::new(path)).to_string_lossy().replace('\\', "/");
        match &extraction.stripped_root { Some(root) => format!("{root}/{rel}"), None => rel }
    };
    outcome.models.imported = imported.imported.clone();
    outcome.models.skipped.extend(imported.skipped.iter().map(|file| serde_json::json!({"entryPath":entry_path(&file.path),"reason":file.reason})));
    outcome.models.duplicates = imported.duplicate_entries.iter().map(|entry| serde_json::json!({"entryPath":entry_path(&entry.path), "kind":entry.kind})).collect();
    let nothing_new = imported.imported.is_empty();
    result.duplicate_count += outcome.models.duplicates.len() as i64;
    result.imported.extend(imported.imported);
    result.skipped.extend(imported.skipped);
    if nothing_new {
        // Only duplicates or unreadable files: keep nothing on disk, otherwise every
        // retry leaves another "Name (2)", "Name (3)" folder behind. The archive
        // stays, since nothing was taken from it.
        extraction.rollback();
        if ctx.delete_archive {
            outcome.delete_error = Some("Archiv behalten: nichts Neues fuer den Katalog".to_string());
        }
        return outcome;
    }
    outcome.stripped_root = extraction.stripped_root.clone();
    outcome.extracted_to = Some(dest.to_string_lossy().to_string());

    if ctx.delete_archive {
        let not_extracted = outcome.existing_skipped + outcome.unsafe_skipped + outcome.blocked_skipped;
        if not_extracted > 0 {
            // Otherwise the content of the skipped entries would be lost.
            outcome.delete_error =
                Some(format!("Archiv behalten: {not_extracted} Eintraege wurden nicht entpackt"));
            return outcome;
        }
        match file_fingerprint(archive_path) {
            Some((size, modified))
                if size == request.expected_size && modified == request.expected_modified_unix_ms =>
            {
                match std::fs::remove_file(archive_path) {
                    Ok(()) => outcome.archive_deleted = true,
                    Err(e) => outcome.delete_error = Some(e.to_string()),
                }
            }
            _ => {
                outcome.delete_error =
                    Some("Archiv wurde seit der Pruefung veraendert und bleibt erhalten".to_string())
            }
        }
    }
    outcome
}

/// Core of `extract_archives`, without Tauri state, so it's directly testable.
/// `import_dir` imports an extracted folder (in the command: with the DB lock
/// ONLY for that step, so extracting large archives doesn't block the app).
#[cfg(test)]
pub(crate) fn extract_archives_core(
    sensitive_dirs: &[PathBuf],
    target_dir: &Path,
    requests: Vec<ArchiveRequest>,
    delete_archives: bool,
    mut import_dir: impl FnMut(&Path, &[PathBuf]) -> CmdResult<ImportResultDto>,
    mut on_progress: impl FnMut(&str, &'static str),
) -> CmdResult<ArchiveImportResultDto> {
    extract_archives_controlled(sensitive_dirs, target_dir, requests, delete_archives, &mut import_dir, &mut on_progress, None)
}
pub(super) fn extract_archives_controlled(
    sensitive_dirs: &[PathBuf], target_dir: &Path, requests: Vec<ArchiveRequest>, delete_archives: bool,
    mut import_dir: impl FnMut(&Path, &[PathBuf]) -> CmdResult<ImportResultDto>,
    mut on_progress: impl FnMut(&str, &'static str), cancel: Option<&std::sync::atomic::AtomicBool>,
) -> CmdResult<ArchiveImportResultDto> {
    // Compute once - the guard runs for every entry.
    let expanded_sensitive = expand_sensitive_dirs(sensitive_dirs);
    check_target_location(target_dir, &expanded_sensitive)?;
    let ctx = ExtractContext {
        cancel,
        expanded_sensitive: &expanded_sensitive,
        target_dir,
        delete_archive: delete_archives,
    };

    let mut result = ArchiveImportResultDto {
        imported: Vec::new(),
        duplicate_count: 0,
        archives: Vec::new(),
        skipped: Vec::new(),
    };
    for request in &requests {
        if cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Acquire)) {
            result.archives.push(ArchiveOutcomeDto { path: request.path.clone(), error: Some("notStarted".into()), ..Default::default() });
            continue;
        }
        let outcome = extract_one(&ctx, request, &mut result, &mut import_dir, &mut on_progress);
        on_progress(&request.path, if outcome.error.is_some() { "failed" } else { "done" });
        result.archives.push(outcome);
    }
    Ok(result)
}

#[tauri::command]
pub async fn inspect_archives(paths: Vec<String>) -> CmdResult<Vec<ArchiveInfoDto>> {
    Ok(paths.iter().map(|p| inspect_one(Path::new(p))).collect())
}

#[tauri::command]
pub fn archive_target_conflicts(target_dir: String, folder_names: Vec<String>) -> CmdResult<Vec<bool>> {
    let target = Path::new(&target_dir);
    Ok(folder_names
        .iter()
        .map(|name| path_exists(&target.join(archive::safe_folder_name(name))))
        .collect())
}

#[tauri::command]
pub async fn extract_archives(
    app: tauri::AppHandle,
    target_dir: String,
    requests: Vec<ArchiveRequest>,
    delete_archives: bool,
) -> CmdResult<ArchiveImportResultDto> {
    super::import_jobs::legacy_archives(app, target_dir, requests, delete_archives).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Smallest valid ASCII STL (one triangle), so the real import accepts the file.
    const STL: &[u8] = b"solid t\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid t\n";
    const STL_OTHER: &[u8] = b"solid u\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 2 0 0\nvertex 0 2 0\nendloop\nendfacet\nendsolid u\n";

    fn make_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        for (name, data) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }

    fn request_for(info: &ArchiveInfoDto, on_conflict: ConflictMode) -> ArchiveRequest {
        ArchiveRequest {
            path: info.path.clone(),
            folder_name: info.suggested_folder_name.clone(),
            on_conflict,
            expected_size: info.file_size,
            expected_modified_unix_ms: info.modified_unix_ms,
        }
    }

    fn run(
        conn: &mut Connection,
        target: &Path,
        requests: Vec<ArchiveRequest>,
        delete: bool,
    ) -> ArchiveImportResultDto {
        extract_archives_core(
            &[],
            target,
            requests,
            delete,
            |dir, c| import_extracted_dir(conn, dir, c),
            |_, _| {},
        )
        .expect("extract_archives_core")
    }

    #[test]
    fn inspect_one_reports_models_and_suggested_folder_name() {
        let dir = unique_test_dir("archives_inspect");
        let archive = dir.join("Drache_v2.zip");
        make_zip(&archive, &[("Drache/koerper.stl", STL), ("Drache/liesmich.txt", b"x")]);

        let info = inspect_one(&archive);
        assert_eq!(info.status, ArchiveStatusDto::Ok);
        assert_eq!(info.model_count, 1);
        assert_eq!(info.entry_count, 2);
        assert_eq!(info.suggested_folder_name, "Drache_v2");
        assert_eq!(info.file_size, std::fs::metadata(&archive).unwrap().len());
    }

    #[test]
    fn extracted_root_is_reported_without_changing_the_target_folder_name() {
        let dir = unique_test_dir("archives_stripped_root");
        let target = dir.join("Katalog");
        std::fs::create_dir_all(&target).unwrap();
        let archive = dir.join("Drache.zip");
        make_zip(&archive, &[("Garten-Paket/koerper.stl", STL)]);
        let mut conn = crate::db::connect_in_memory().unwrap();
        let info = inspect_one(&archive);
        assert_eq!(info.suggested_folder_name, "Drache");
        let result = run(&mut conn, &target, vec![request_for(&info, ConflictMode::New)], false);
        assert_eq!(result.imported.len(), 1);
        let outcome = &result.archives[0];
        assert_eq!(outcome.error, None);
        assert_eq!(outcome.stripped_root.as_deref(), Some("Garten-Paket"));
        assert!(target.join("Drache/koerper.stl").exists());
        assert!(!target.join("Drache/Garten-Paket").exists());
        assert_eq!(serde_json::to_value(outcome).unwrap()["strippedRoot"], "Garten-Paket");
    }

    #[test]
    fn extract_imports_models_into_a_subfolder_of_the_catalogued_target() {
        let dir = unique_test_dir("archives_extract");
        let target = dir.join("Katalog");
        std::fs::create_dir_all(&target).unwrap();
        let archive = dir.join("Drache.zip");
        make_zip(&archive, &[("koerper.stl", STL), ("liesmich.txt", b"x")]);

        let mut conn = crate::db::connect_in_memory().unwrap();
        let target_id = db::ensure_folder_path(&conn, &target, &target).unwrap();
        let info = inspect_one(&archive);
        let result = run(&mut conn, &target, vec![request_for(&info, ConflictMode::New)], false);

        assert_eq!(result.imported.len(), 1);
        let outcome = &result.archives[0];
        assert_eq!(outcome.error, None);
        assert_eq!(outcome.extracted_to.as_deref(), Some(target.join("Drache").to_str().unwrap()));
        assert!(target.join("Drache/liesmich.txt").exists());
        assert!(archive.exists(), "ohne Loeschwunsch bleibt das Archiv liegen");

        let folders = db::list_folders(&conn).unwrap();
        let sub = folders.iter().find(|f| f.path == target.join("Drache").to_string_lossy()).unwrap();
        assert_eq!(sub.parent_id, Some(target_id));
    }

    #[test]
    fn an_archive_with_nothing_new_leaves_no_folder_behind() {
        let dir = unique_test_dir("archives_nothing_new");
        let target = dir.join("Katalog");
        std::fs::create_dir_all(&target).unwrap();
        let archive = dir.join("Drache.zip");
        make_zip(&archive, &[("Teile/koerper.stl", STL), ("liesmich.txt", b"x")]);

        let mut conn = crate::db::connect_in_memory().unwrap();
        db::ensure_folder_path(&conn, &target, &target).unwrap();
        let info = inspect_one(&archive);
        let first = run(&mut conn, &target, vec![request_for(&info, ConflictMode::New)], false);
        assert_eq!(first.imported.len(), 1);

        // The same archive again: every model is a duplicate.
        let second = run(&mut conn, &target, vec![request_for(&info, ConflictMode::New)], true);
        assert!(second.imported.is_empty());
        assert_eq!(second.duplicate_count, 1);
        let outcome = &second.archives[0];
        assert_eq!(outcome.error, None);
        assert_eq!(outcome.extracted_to, None);
        assert!(!target.join("Drache (2)").exists(), "nothing new, so nothing may stay on disk");
        assert!(archive.exists(), "the archive stays when nothing was taken from it");
        assert!(outcome.delete_error.is_some());
        let paths: Vec<String> = db::list_folders(&conn).unwrap().into_iter().map(|f| f.path).collect();
        assert!(!paths.iter().any(|p| p.contains("Drache (2)")), "no folder row for the removed folder: {paths:?}");
        // The first extraction is untouched.
        assert!(target.join("Drache/Teile/koerper.stl").exists());
    }

    #[test]
    fn undoing_an_extraction_keeps_older_folder_rows_for_the_same_path() {
        let dir = unique_test_dir("archives_keep_old_rows");
        let target = dir.join("Katalog");
        std::fs::create_dir_all(&target).unwrap();
        let archive = dir.join("Drache.zip");
        make_zip(&archive, &[("koerper.stl", STL)]);

        let mut conn = crate::db::connect_in_memory().unwrap();
        db::ensure_folder_path(&conn, &target, &target).unwrap();
        let info = inspect_one(&archive);
        run(&mut conn, &target, vec![request_for(&info, ConflictMode::New)], false);

        // An older catalog row for "Drache (2)" with a subfolder that holds a file,
        // whose directory was deleted outside the app.
        let old = target.join("Drache (2)");
        let old_id = db::ensure_folder_path(&conn, &target, &old).unwrap();
        let sub_id = db::ensure_folder_path(&conn, &target, &old.join("Teile")).unwrap();
        crate::db::test_insert_minimal_file(&conn, &old.join("Teile/alt.stl").to_string_lossy(), Some(sub_id)).unwrap();

        let second = run(&mut conn, &target, vec![request_for(&info, ConflictMode::New)], false);
        assert!(second.imported.is_empty());
        assert!(!old.exists());
        let ids: Vec<i64> = db::list_folders(&conn).unwrap().into_iter().map(|f| f.id).collect();
        assert!(ids.contains(&old_id), "the older row must stay");
        assert!(ids.contains(&sub_id), "its subfolder with a file must stay");
    }

    #[test]
    fn new_mode_numbers_the_folder_when_it_already_exists() {
        let dir = unique_test_dir("archives_numbering");
        let target = dir.join("Katalog");
        std::fs::create_dir_all(target.join("Drache")).unwrap();
        std::fs::create_dir_all(target.join("Drache (2)")).unwrap();
        let archive = dir.join("Drache.zip");
        make_zip(&archive, &[("koerper.stl", STL)]);

        assert_eq!(unique_destination(&target, "Drache"), target.join("Drache (3)"));

        let mut conn = crate::db::connect_in_memory().unwrap();
        let info = inspect_one(&archive);
        let result = run(&mut conn, &target, vec![request_for(&info, ConflictMode::New)], false);
        assert_eq!(
            result.archives[0].extracted_to.as_deref(),
            Some(target.join("Drache (3)").to_str().unwrap())
        );
    }

    #[test]
    fn merge_mode_keeps_existing_files_and_reports_them() {
        let dir = unique_test_dir("archives_merge");
        let target = dir.join("Katalog");
        std::fs::create_dir_all(target.join("Drache")).unwrap();
        std::fs::write(target.join("Drache/liesmich.txt"), b"meins").unwrap();
        let archive = dir.join("Drache.zip");
        make_zip(&archive, &[("koerper.stl", STL), ("liesmich.txt", b"x")]);

        let mut conn = crate::db::connect_in_memory().unwrap();
        let info = inspect_one(&archive);
        let result = run(&mut conn, &target, vec![request_for(&info, ConflictMode::Merge)], false);
        assert_eq!(result.archives[0].existing_skipped, 1);
        assert_eq!(std::fs::read(target.join("Drache/liesmich.txt")).unwrap(), b"meins");
        assert_eq!(result.imported.len(), 1);
    }

    #[test]
    fn delete_removes_the_archive_only_when_unchanged_since_inspect() {
        let dir = unique_test_dir("archives_delete");
        let target = dir.join("Katalog");
        std::fs::create_dir_all(&target).unwrap();
        let kept = dir.join("Behalten.zip");
        let deleted = dir.join("Weg.zip");
        make_zip(&kept, &[("a.stl", STL)]);
        // A different model, otherwise the second archive holds only a duplicate and stays.
        make_zip(&deleted, &[("b.stl", STL_OTHER)]);

        let mut conn = crate::db::connect_in_memory().unwrap();
        let mut stale = request_for(&inspect_one(&kept), ConflictMode::New);
        stale.expected_size += 1;
        let fresh = request_for(&inspect_one(&deleted), ConflictMode::New);
        let result = run(&mut conn, &target, vec![stale, fresh], true);

        assert!(kept.exists());
        assert!(!result.archives[0].archive_deleted);
        assert!(result.archives[0].delete_error.is_some());
        assert!(!deleted.exists());
        assert!(result.archives[1].archive_deleted);
    }

    #[test]
    fn a_broken_archive_fails_alone_and_is_never_deleted() {
        let dir = unique_test_dir("archives_broken");
        let target = dir.join("Katalog");
        std::fs::create_dir_all(&target).unwrap();
        let broken = dir.join("Kaputt.zip");
        std::fs::write(&broken, b"PK\x03\x04 abgeschnitten").unwrap();
        let good = dir.join("Gut.zip");
        make_zip(&good, &[("a.stl", STL)]);

        let mut conn = crate::db::connect_in_memory().unwrap();
        let broken_request = ArchiveRequest {
            path: broken.to_string_lossy().to_string(),
            folder_name: "Kaputt".to_string(),
            on_conflict: ConflictMode::New,
            expected_size: std::fs::metadata(&broken).unwrap().len(),
            expected_modified_unix_ms: file_fingerprint(&broken).unwrap().1,
        };
        let good_request = request_for(&inspect_one(&good), ConflictMode::New);
        let result = run(&mut conn, &target, vec![broken_request, good_request], true);

        assert!(result.archives[0].error.is_some());
        assert!(broken.exists());
        assert!(!target.join("Kaputt").exists(), "halb angelegter Ordner wird aufgeraeumt");
        assert_eq!(result.archives[1].error, None);
        assert_eq!(result.imported.len(), 1);
    }

    #[test]
    fn a_sensitive_target_is_rejected_before_anything_is_written() {
        let dir = unique_test_dir("archives_sensitive");
        let target = dir.join(".config");
        std::fs::create_dir_all(&target).unwrap();
        let archive = dir.join("Drache.zip");
        make_zip(&archive, &[("a.stl", STL)]);

        let mut conn = crate::db::connect_in_memory().unwrap();
        let request = request_for(&inspect_one(&archive), ConflictMode::New);
        let result = extract_archives_core(
            std::slice::from_ref(&target),
            &target,
            vec![request],
            false,
            |d, c| import_extracted_dir(&mut conn, d, c),
            |_, _| {},
        );
        assert!(result.is_err());
        assert!(!target.join("Drache").exists());
    }

    #[test]
    fn merging_into_a_folder_that_contains_a_protected_path_is_refused() {
        // ".local.zip" attack: the target is allowed, but `dest` contains a protected
        // area; then nothing is extracted at all.
        let dir = unique_test_dir("archives_guard");
        let target = dir.join("Home");
        let protected = target.join("Drache/share/applications");
        std::fs::create_dir_all(&protected).unwrap();
        let archive = dir.join("Drache.zip");
        make_zip(&archive, &[("share/applications/boese.stl", STL), ("ok.stl", STL)]);

        let mut conn = crate::db::connect_in_memory().unwrap();
        let request = request_for(&inspect_one(&archive), ConflictMode::Merge);
        let result = extract_archives_core(
            std::slice::from_ref(&protected),
            &dir,
            vec![request],
            false,
            |d, c| import_extracted_dir(&mut conn, d, c),
            |_, _| {},
        );
        assert!(result.is_err(), "Ziel 'Home' enthaelt den geschuetzten Ordner");

        let request = request_for(&inspect_one(&archive), ConflictMode::Merge);
        let mut conn = crate::db::connect_in_memory().unwrap();
        let result = extract_archives_core(
            std::slice::from_ref(&protected),
            &target,
            vec![request],
            false,
            |d, c| import_extracted_dir(&mut conn, d, c),
            |_, _| {},
        );
        assert!(result.is_err(), "auch der Zielordner selbst ist Vorfahr");
        assert!(!protected.join("boese.stl").exists());
        assert!(!target.join("Drache/ok.stl").exists());
    }

    #[test]
    fn hidden_folder_names_from_the_frontend_are_neutralized() {
        let dir = unique_test_dir("archives_hidden");
        let target = dir.join("Home");
        std::fs::create_dir_all(&target).unwrap();
        let archive = dir.join("x.zip");
        make_zip(&archive, &[("a.stl", STL)]);

        let mut conn = crate::db::connect_in_memory().unwrap();
        let mut request = request_for(&inspect_one(&archive), ConflictMode::Merge);
        request.folder_name = ".local".to_string();
        let result = run(&mut conn, &target, vec![request], false);
        assert_eq!(
            result.archives[0].extracted_to.as_deref(),
            Some(target.join("local").to_str().unwrap())
        );
        assert!(!target.join(".local").exists());
    }

    #[test]
    fn blocked_file_types_are_reported_in_the_outcome() {
        let dir = unique_test_dir("archives_blocked");
        let target = dir.join("Katalog");
        std::fs::create_dir_all(&target).unwrap();
        let archive = dir.join("Paket.zip");
        make_zip(&archive, &[("a.stl", STL), ("setup.exe", b"MZ"), ("Info.url", b"x")]);

        let mut conn = crate::db::connect_in_memory().unwrap();
        let request = request_for(&inspect_one(&archive), ConflictMode::New);
        let result = run(&mut conn, &target, vec![request], false);
        assert_eq!(result.archives[0].blocked_skipped, 2);
        assert!(!target.join("Paket/setup.exe").exists());
    }

    #[test]
    fn target_conflicts_are_reported_per_sanitized_name() {
        let dir = unique_test_dir("archives_conflicts");
        std::fs::create_dir_all(dir.join("Drache")).unwrap();
        let result = archive_target_conflicts(
            dir.to_string_lossy().to_string(),
            vec!["Drache".to_string(), "Neu".to_string()],
        )
        .unwrap();
        assert_eq!(result, vec![true, false]);
    }

    #[test]
    fn only_registered_archives_are_authorized_and_only_once() {
        let dir = unique_test_dir("archives_single_use");
        let archive = dir.join("a.zip"); make_zip(&archive, &[("a.stl", STL)]);
        let service = ImportJobs::default(); let parent = service.test_handoff(vec![archive.clone()]);
        assert!(service.authorize_archives(&[], &dir, true, &[plain_request(&archive), ArchiveRequest { path: "/foreign.zip".into(), ..plain_request(&archive) }], Some(&parent)).is_err());
        assert_eq!(service.authorize_archives(&[], &dir, true, &[plain_request(&archive)], Some(&parent)).unwrap(), Some(parent.clone()));
        assert!(service.authorize_archives(&[], &dir, true, &[plain_request(&archive)], Some(&parent)).is_err());

    }

    fn run_core(conn: &mut Connection, target: &Path, requests: Vec<ArchiveRequest>) -> CmdResult<ArchiveImportResultDto> {
        extract_archives_core(&[], target, requests, false, |d, c| import_extracted_dir(conn, d, c), |_, _| {})
    }

    fn plain_request(path: &Path) -> ArchiveRequest {
        let info = inspect_one(path);
        request_for(&info, ConflictMode::New)
    }

    #[test]
    fn import_dropped_only_accepts_archives_the_backend_saw_being_dropped() {
        let dir = unique_test_dir("archives_observed_drop");
        let dropped = dir.join("Fallen.zip");
        let foreign = dir.join("Fremd.zip");
        let model = dir.join("teil.stl");
        make_zip(&dropped, &[("a.stl", STL)]);
        make_zip(&foreign, &[("a.stl", STL)]);
        std::fs::write(&model, STL).unwrap();

        let service = ImportJobs::default();
        service.observe_drop(&[dropped.clone(), model.clone(), dir.join("fehlt.zip")]);
        assert!(service.authorize_models(ImportSource::Dropped, &[dropped.clone(), foreign.clone()], false, &[]).is_err(), "foreign input rejects the entire job");
        assert_eq!(service.authorize_models(ImportSource::Dropped, &[dropped.clone(), model], false, &[]).unwrap(), ImportSource::Dropped);
        let parent = service.test_handoff(vec![dropped.clone()]);
        assert!(service.authorize_archives(&[], &dir, true, &[plain_request(&foreign)], Some(&parent)).is_err());
        assert_eq!(service.authorize_archives(&[], &dir, true, &[plain_request(&dropped)], Some(&parent)).unwrap(), Some(parent));
        assert!(service.authorize_models(ImportSource::Dropped, &[dropped], false, &[]).is_err());

    }

    #[test]
    fn target_dir_must_be_a_catalog_folder_or_picked_in_the_app() {
        let dir = unique_test_dir("archives_approved_target");
        let catalog = dir.join("Katalog");
        let picked = dir.join("Gewaehlt");
        let other = dir.join("Anderswo");
        for d in [&catalog, &picked, &other] {
            std::fs::create_dir_all(d).unwrap();
        }
        let conn = crate::db::connect_in_memory().unwrap();
        db::ensure_folder_path(&conn, &catalog, &catalog).unwrap();
        let approved = ApprovedTargets::default();
        approved.approve(&picked);

        assert!(target_is_approved(&conn, &approved, &catalog));
        assert!(target_is_approved(&conn, &approved, &picked));
        assert!(!target_is_approved(&conn, &approved, &other));
    }

    #[test]
    fn an_unapproved_target_is_rejected_without_consuming_the_archives() {
        let dir = unique_test_dir("archives_retry_target");
        let target = dir.join("Katalog");
        std::fs::create_dir_all(&target).unwrap();
        let archive = dir.join("Drache.zip");
        make_zip(&archive, &[("koerper.stl", STL)]);
        let service = ImportJobs::default();
        let parent = service.test_handoff(vec![archive.clone()]);

        let mut conn = crate::db::connect_in_memory().unwrap();
        let first = service.authorize_archives(&[], &target, false, &[plain_request(&archive)], Some(&parent));
        assert!(first.unwrap_err().contains("nicht ueber die App ausgewaehlt"));
        assert!(!target.join("Drache").exists());
        let missing = service.authorize_archives(&[], &dir.join("gibt-es-nicht"), true, &[plain_request(&archive)], Some(&parent));
        assert!(missing.is_err());
        assert_eq!(service.authorize_archives(&[], &target, true, &[plain_request(&archive)], Some(&parent)).unwrap(), Some(parent));
        let retry = run_core(&mut conn, &target, vec![plain_request(&archive)]).expect("zweiter Versuch mit gueltigem Ziel");
        assert_eq!(retry.archives.len(), 1);
        assert_eq!(retry.archives[0].error, None, "{:?}", retry.archives[0].error);
        assert!(target.join("Drache/koerper.stl").exists());
    }

    #[test]
    fn a_target_or_destination_containing_a_protected_folder_is_rejected() {
        let dir = unique_test_dir("archives_ancestor");
        let home = dir.join("Home");
        let ssh = home.join(".ssh");
        std::fs::create_dir_all(&ssh).unwrap();
        let archive = dir.join("Home.zip");
        make_zip(&archive, &[(".bash_profile", b"boese"), ("a.stl", STL)]);
        let mut conn = crate::db::connect_in_memory().unwrap();

        // Target folder = home itself (ancestor of ~/.ssh).
        let as_target = extract_archives_core(
            std::slice::from_ref(&ssh),
            &home,
            vec![plain_request(&archive)],
            false,
            |d, c| import_extracted_dir(&mut conn, d, c),
            |_, _| {},
        );
        assert!(as_target.is_err());

        // Merging into a subfolder that symlinks to "home": the target itself looks
        // harmless, but `dest` resolves to an ancestor of ~/.ssh.
        #[cfg(unix)]
        {
            let katalog = dir.join("Katalog");
            std::fs::create_dir_all(&katalog).unwrap();
            std::os::unix::fs::symlink(&home, katalog.join("Home")).unwrap();
            let mut request = plain_request(&archive);
            request.on_conflict = ConflictMode::Merge;
            let result = extract_archives_core(
                std::slice::from_ref(&ssh),
                &katalog,
                vec![request],
                false,
                |d, c| import_extracted_dir(&mut conn, d, c),
                |_, _| {},
            )
            .unwrap();
            let error = result.archives[0].error.as_deref().unwrap_or_default();
            assert!(error.contains("enthaelt ein geschuetztes Verzeichnis"), "{error}");
            assert!(!home.join(".bash_profile").exists());
            assert!(!home.join("a.stl").exists());
        }
    }

    #[test]
    fn the_archive_is_kept_when_some_entries_were_not_extracted() {
        let dir = unique_test_dir("archives_keep_partial");
        let target = dir.join("Katalog");
        std::fs::create_dir_all(&target).unwrap();
        let archive = dir.join("Paket.zip");
        make_zip(&archive, &[("a.stl", STL), ("setup.exe", b"MZ")]);

        let mut conn = crate::db::connect_in_memory().unwrap();
        let result = run(&mut conn, &target, vec![plain_request(&archive)], true);
        let outcome = &result.archives[0];
        assert_eq!(outcome.error, None);
        assert_eq!(outcome.blocked_skipped, 1);
        assert!(!outcome.archive_deleted);
        assert_eq!(
            outcome.delete_error.as_deref(),
            Some("Archiv behalten: 1 Eintraege wurden nicht entpackt")
        );
        assert!(archive.exists());
    }

    #[test]
    fn a_failed_catalog_import_removes_the_extracted_folder_and_keeps_the_archive() {
        let dir = unique_test_dir("archives_import_fails");
        let target = dir.join("Katalog");
        std::fs::create_dir_all(&target).unwrap();
        let archive = dir.join("Drache.zip");
        make_zip(&archive, &[("koerper.stl", STL)]);

        let result = extract_archives_core(
            &[],
            &target,
            vec![plain_request(&archive)],
            true,
            |_, _| Err("Datenbank weg".into()),
            |_, _| {},
        )
        .unwrap();
        let outcome = &result.archives[0];
        assert_eq!(outcome.error.as_deref(), Some("Datenbank weg"));
        assert!(!outcome.archive_deleted);
        assert_eq!(outcome.extracted_to, None);
        assert!(!target.join("Drache").exists(), "entpackter Ordner wird entfernt");
        assert!(archive.exists());
    }
}

#[cfg(test)]
#[path = "import_archive_tests.rs"]
mod import_archive_tests;
