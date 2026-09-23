//! Tauri-Befehle zum Entpacken EINZELN importierter Archive (Dateidialog,
//! Drag & Drop). Die Archivlogik selbst liegt in `crate::archive`; hier
//! passieren nur Ablaufsteuerung, Katalog-Import und das optionale Loeschen
//! des Originals. Der Ordner-Import entpackt bewusst nichts (Spezifikation
//! 2026-09-23, Entscheidung 1).

use super::*;
use super::files::{import_many_with_conn, is_supported_extension};

use serde::Deserialize;
use tauri::Emitter;

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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveRequest {
    pub path: String,
    pub folder_name: String,
    pub on_conflict: ConflictMode,
    pub expected_size: u64,
    pub expected_modified_unix_ms: i64,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveOutcomeDto {
    pub path: String,
    pub extracted_to: Option<String>,
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
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ArchiveProgressDto {
    path: String,
    state: &'static str,
}

/// Serverseitige Freigabeliste: Nur Archive, die der Nutzer tatsaechlich
/// per Dateidialog oder Drag & Drop hereingegeben hat, duerfen entpackt
/// (und ggf. geloescht) werden - jedes hoechstens einmal. Ohne diese Liste
/// koennte ein kompromittiertes Frontend beliebige Dateien mit
/// Archiv-Endung entpacken und loeschen lassen.
#[derive(Default)]
pub struct PendingArchives(std::sync::Mutex<std::collections::HashSet<PathBuf>>);

impl PendingArchives {
    pub(crate) fn register(&self, paths: &[String]) {
        if let Ok(mut set) = self.0.lock() {
            set.extend(paths.iter().map(PathBuf::from));
        }
    }

    pub(crate) fn take_authorized(
        &self,
        requests: Vec<ArchiveRequest>,
    ) -> (Vec<ArchiveRequest>, Vec<ArchiveOutcomeDto>) {
        let mut allowed = Vec::new();
        let mut rejected = Vec::new();
        let mut set = match self.0.lock() {
            Ok(set) => set,
            Err(_) => {
                return (
                    Vec::new(),
                    requests
                        .into_iter()
                        .map(|r| ArchiveOutcomeDto {
                            path: r.path,
                            error: Some("interner Fehler (Sperre)".to_string()),
                            ..Default::default()
                        })
                        .collect(),
                )
            }
        };
        for request in requests {
            if set.remove(Path::new(&request.path)) {
                allowed.push(request);
            } else {
                rejected.push(ArchiveOutcomeDto {
                    path: request.path,
                    error: Some("Archiv wurde nicht ueber den Import freigegeben".to_string()),
                    ..Default::default()
                });
            }
        }
        (allowed, rejected)
    }
}

/// Groesse und Aenderungszeit (ms) - dient als "unveraendert seit
/// inspect?"-Pruefung vor dem Loeschen des Originals.
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

/// Erster freier Name `name`, `name (2)`, `name (3)`, … unter `target_dir`.
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

/// Import eines frisch entpackten Ordners in den Katalog: bestehender
/// Ordner-Import plus Einhaengen unter den Ziel-Katalogordner.
pub(crate) fn import_extracted_dir(conn: &mut Connection, dir: &Path) -> CmdResult<ImportResultDto> {
    let result = import_many_with_conn(conn, vec![dir.to_path_buf()])?;
    // Nur kosmetisch (Ordnerbaum) - ein Fehler hier soll den bereits
    // erfolgreichen Import nicht rueckgaengig machen.
    if let Err(e) = db::attach_folder_to_parent_by_path(conn, dir) {
        eprintln!("[archive] Einhaengen von {} fehlgeschlagen: {e}", dir.display());
    }
    Ok(result)
}

fn extract_one(
    sensitive_dirs: &[PathBuf],
    expanded_sensitive: &[PathBuf],
    target_dir: &Path,
    request: &ArchiveRequest,
    delete_archive: bool,
    result: &mut ArchiveImportResultDto,
    import_dir: &mut impl FnMut(&Path) -> CmdResult<ImportResultDto>,
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
    // Der Name kommt vom Frontend - erneut bereinigen, nie mit "." beginnend.
    let clean_name = archive::safe_folder_name(&request.folder_name);
    let folder_name = if clean_name.is_empty() {
        archive::folder_name_for(archive_path)
    } else {
        clean_name
    };
    let merge = request.on_conflict == ConflictMode::Merge;
    let dest = if merge {
        target_dir.join(&folder_name)
    } else {
        unique_destination(target_dir, &folder_name)
    };
    if let Err(e) = reject_if_sensitive_path(&dest, sensitive_dirs) {
        outcome.error = Some(e);
        return outcome;
    }

    on_progress(&request.path, "extracting");
    // Jeder einzelne neue Pfad wird gegen die Schutzliste geprueft, nicht
    // nur `dest`: sonst koennte ein Archiv beim Zusammenfuehren ueber
    // Unterordner (z.B. `share/applications/`) in geschuetzte Bereiche
    // schreiben, obwohl `dest` selbst unverdaechtig ist.
    let guard = |p: &Path| reject_if_sensitive_path_expanded(p, expanded_sensitive).is_ok();
    let extraction = match archive::extract_archive(archive_path, format, &dest, merge, MAX_UNPACKED_BYTES, &guard) {
        Ok(extraction) => extraction,
        Err(e) => {
            outcome.error = Some(e.to_string());
            return outcome;
        }
    };
    outcome.existing_skipped = extraction.stats.existing_skipped;
    outcome.unsafe_skipped = extraction.stats.unsafe_skipped;
    outcome.blocked_skipped = extraction.stats.blocked_skipped;

    on_progress(&request.path, "importing");
    let imported = match import_dir(&dest) {
        Ok(imported) => imported,
        Err(e) => {
            extraction.rollback();
            outcome.error = Some(e);
            return outcome;
        }
    };
    result.duplicate_count += imported.duplicate_count;
    result.imported.extend(imported.imported);
    outcome.extracted_to = Some(dest.to_string_lossy().to_string());

    if delete_archive {
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

/// Kern von `extract_archives`, ohne Tauri-State, damit direkt testbar.
/// `import_dir` importiert einen entpackten Ordner (im Befehl: mit
/// DB-Sperre NUR fuer diesen Schritt, damit das Entpacken grosser Archive
/// die App nicht blockiert).
pub(crate) fn extract_archives_core(
    sensitive_dirs: &[PathBuf],
    target_dir: &Path,
    requests: Vec<ArchiveRequest>,
    delete_archives: bool,
    mut import_dir: impl FnMut(&Path) -> CmdResult<ImportResultDto>,
    mut on_progress: impl FnMut(&str, &'static str),
) -> CmdResult<ArchiveImportResultDto> {
    if !target_dir.is_dir() {
        return Err(format!("Zielordner existiert nicht: {}", target_dir.display()));
    }
    reject_if_sensitive_path(target_dir, sensitive_dirs)?;
    // Einmal vorberechnen - der Guard laeuft fuer jeden Eintrag.
    let expanded_sensitive = expand_sensitive_dirs(sensitive_dirs);

    let mut result = ArchiveImportResultDto {
        imported: Vec::new(),
        duplicate_count: 0,
        archives: Vec::new(),
    };
    for request in &requests {
        let outcome = extract_one(
            sensitive_dirs,
            &expanded_sensitive,
            target_dir,
            request,
            delete_archives,
            &mut result,
            &mut import_dir,
            &mut on_progress,
        );
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
    state: State<'_, AppState>,
    pending: State<'_, PendingArchives>,
    target_dir: String,
    requests: Vec<ArchiveRequest>,
    delete_archives: bool,
) -> CmdResult<ArchiveImportResultDto> {
    let (allowed, rejected) = pending.take_authorized(requests);
    let mut result = extract_archives_core(
        &state.sensitive_dirs,
        Path::new(&target_dir),
        allowed,
        delete_archives,
        |dir| {
            let mut conn = lock_db(&state)?;
            import_extracted_dir(&mut conn, dir)
        },
        |path, stage| {
            let _ = app.emit(
                "archive-progress",
                ArchiveProgressDto {
                    path: path.to_string(),
                    state: stage,
                },
            );
        },
    )?;
    result.archives.extend(rejected);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Kleinste gueltige ASCII-STL (ein Dreieck), damit der echte Import
    /// die Datei annimmt.
    const STL: &[u8] = b"solid t\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid t\n";

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
            |dir| import_extracted_dir(conn, dir),
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
        make_zip(&deleted, &[("b.stl", STL)]);

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
            &[target.clone()],
            &target,
            vec![request],
            false,
            |d| import_extracted_dir(&mut conn, d),
            |_, _| {},
        );
        assert!(result.is_err());
        assert!(!target.join("Drache").exists());
    }

    #[test]
    fn protected_subpaths_are_skipped_even_when_the_destination_is_allowed() {
        // Nachbau des ".local.zip"-Angriffs: Ziel ist erlaubt, ein
        // Unterordner des Archivs zeigt aber in einen geschuetzten Bereich.
        let dir = unique_test_dir("archives_guard");
        let target = dir.join("Home");
        let protected = target.join("Drache/share/applications");
        std::fs::create_dir_all(&protected).unwrap();
        let archive = dir.join("Drache.zip");
        make_zip(&archive, &[("share/applications/boese.stl", STL), ("ok.stl", STL)]);

        let mut conn = crate::db::connect_in_memory().unwrap();
        let request = request_for(&inspect_one(&archive), ConflictMode::Merge);
        let result = extract_archives_core(
            &[protected.clone()],
            &target,
            vec![request],
            false,
            |d| import_extracted_dir(&mut conn, d),
            |_, _| {},
        )
        .unwrap();
        assert_eq!(result.archives[0].unsafe_skipped, 1);
        assert!(!protected.join("boese.stl").exists());
        assert!(target.join("Drache/ok.stl").exists());
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
        let pending = PendingArchives::default();
        pending.register(&["/dl/a.zip".to_string()]);
        let request = |path: &str| ArchiveRequest {
            path: path.to_string(),
            folder_name: "x".to_string(),
            on_conflict: ConflictMode::New,
            expected_size: 0,
            expected_modified_unix_ms: 0,
        };

        let (allowed, rejected) =
            pending.take_authorized(vec![request("/dl/a.zip"), request("/home/u/wichtig.zip")]);
        assert_eq!(allowed.len(), 1);
        assert_eq!(allowed[0].path, "/dl/a.zip");
        assert_eq!(rejected.len(), 1);
        assert_eq!(rejected[0].path, "/home/u/wichtig.zip");
        assert!(rejected[0].error.is_some());

        let (allowed_again, rejected_again) = pending.take_authorized(vec![request("/dl/a.zip")]);
        assert!(allowed_again.is_empty());
        assert_eq!(rejected_again.len(), 1);
    }
}
