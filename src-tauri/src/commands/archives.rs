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
///
/// `observed_drops` haelt Archive, die das BACKEND selbst als Drag & Drop
/// gesehen hat (Fenster-Ereignis, siehe `lib.rs`). `import_dropped` darf nur
/// diese freigeben - die Pfadliste des Frontends allein reicht dafuer nicht.
#[derive(Default)]
pub struct PendingArchives {
    pending: std::sync::Mutex<HashSet<PathBuf>>,
    observed_drops: std::sync::Mutex<HashSet<PathBuf>>,
}

/// Gleiche Regel wie `split_archives`: echte Datei mit Archiv-Endung.
fn is_archive_file(path: &Path) -> bool {
    path.is_file() && archive::is_archive_path(path)
}

impl PendingArchives {
    pub(crate) fn register(&self, paths: &[String]) {
        if let Ok(mut set) = self.pending.lock() {
            set.extend(paths.iter().map(PathBuf::from));
        }
    }

    /// Vom Fenster-Ereignis `DragDrop::Drop` aufgerufen.
    pub(crate) fn observe_drop(&self, paths: &[PathBuf]) {
        if let Ok(mut set) = self.observed_drops.lock() {
            set.extend(paths.iter().filter(|p| is_archive_file(p)).cloned());
        }
    }

    /// Uebernimmt aus `archives` nur die vom Backend beobachteten Drops in
    /// die Freigabeliste (und verbraucht die Beobachtung); alle anderen
    /// werden verworfen. Liefert die uebernommenen Pfade.
    pub(crate) fn claim_dropped(&self, archives: Vec<String>) -> Vec<String> {
        let claimed: Vec<String> = match self.observed_drops.lock() {
            Ok(mut observed) => archives
                .into_iter()
                .filter(|p| observed.remove(Path::new(p)))
                .collect(),
            Err(_) => Vec::new(),
        };
        self.register(&claimed);
        claimed
    }

    pub(crate) fn take_authorized(
        &self,
        requests: Vec<ArchiveRequest>,
    ) -> (Vec<ArchiveRequest>, Vec<ArchiveOutcomeDto>) {
        let mut allowed = Vec::new();
        let mut rejected = Vec::new();
        let mut set = match self.pending.lock() {
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

/// Ordner, die der Nutzer im nativen Ordner-Dialog (`pick_folder_path`)
/// gewaehlt hat. Zusammen mit den Katalogordnern die einzigen erlaubten
/// Entpack-Ziele - ein kompromittiertes Frontend kann so kein beliebiges
/// Ziel (z.B. das Home-Verzeichnis) unterschieben.
#[derive(Default)]
pub struct ApprovedTargets(std::sync::Mutex<HashSet<PathBuf>>);

impl ApprovedTargets {
    pub(crate) fn approve(&self, path: &Path) {
        if let Ok(mut set) = self.0.lock() {
            set.insert(path.to_path_buf());
        }
    }

    fn contains(&self, path: &Path) -> bool {
        self.0.lock().map(|set| set.contains(path)).unwrap_or(false)
    }
}

/// `true`, wenn `target` exakt ein Katalogordner ist oder in der App
/// gewaehlt wurde.
pub(crate) fn target_is_approved(conn: &Connection, approved: &ApprovedTargets, target: &Path) -> bool {
    if approved.contains(target) {
        return true;
    }
    let target = target.to_string_lossy();
    db::list_folders(conn)
        .map(|folders| folders.iter().any(|f| f.path == target))
        .unwrap_or(false)
}

/// Lehnt `path` ab, wenn er ein geschuetztes Verzeichnis ENTHAELT (Vorfahr
/// ist). Sonst waeren z.B. das Home-Verzeichnis (Vorfahr von ~/.ssh) oder
/// `/` als Zusammenfuehren-Ziel moeglich und Eintraege wie `.bash_profile`
/// landeten dort.
fn reject_if_ancestor_of_sensitive(path: &Path, expanded_sensitive: &[PathBuf]) -> CmdResult<()> {
    let resolved = resolve_path_for_sensitivity_check(path)?;
    match expanded_sensitive
        .iter()
        .find(|dir| **dir != resolved && dir.starts_with(&resolved))
    {
        Some(dir) => Err(format!(
            "Zielpfad enthaelt ein geschuetztes Verzeichnis ({}) und wird abgelehnt",
            dir.display()
        )),
        None => Ok(()),
    }
}

/// Alle Pruefungen des Zielordners, die NICHTS verbrauchen oder schreiben.
fn check_target_location(target_dir: &Path, expanded_sensitive: &[PathBuf]) -> CmdResult<()> {
    if !target_dir.is_dir() {
        return Err(format!("Zielordner existiert nicht: {}", target_dir.display()));
    }
    reject_if_sensitive_path_expanded(target_dir, expanded_sensitive)?;
    reject_if_ancestor_of_sensitive(target_dir, expanded_sensitive)
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

/// Unveraenderliche Parameter eines `extract_archives`-Durchlaufs.
struct ExtractContext<'a> {
    expanded_sensitive: &'a [PathBuf],
    target_dir: &'a Path,
    delete_archive: bool,
}

fn extract_one(
    ctx: &ExtractContext<'_>,
    request: &ArchiveRequest,
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
        ctx.target_dir.join(&folder_name)
    } else {
        unique_destination(ctx.target_dir, &folder_name)
    };
    if let Err(e) = reject_if_sensitive_path_expanded(&dest, ctx.expanded_sensitive)
        .and_then(|()| reject_if_ancestor_of_sensitive(&dest, ctx.expanded_sensitive))
    {
        outcome.error = Some(e);
        return outcome;
    }

    on_progress(&request.path, "extracting");
    // Jeder einzelne neue Pfad wird gegen die Schutzliste geprueft, nicht
    // nur `dest`: zweite Verteidigungslinie, falls ein geschuetzter Ordner
    // unterhalb von `dest` erst nach der Pruefung oben entsteht.
    let guard = |p: &Path| reject_if_sensitive_path_expanded(p, ctx.expanded_sensitive).is_ok();
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

    if ctx.delete_archive {
        let not_extracted = outcome.existing_skipped + outcome.unsafe_skipped + outcome.blocked_skipped;
        if not_extracted > 0 {
            // Sonst ginge der Inhalt der uebersprungenen Eintraege verloren.
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
    // Einmal vorberechnen - der Guard laeuft fuer jeden Eintrag.
    let expanded_sensitive = expand_sensitive_dirs(sensitive_dirs);
    check_target_location(target_dir, &expanded_sensitive)?;
    let ctx = ExtractContext {
        expanded_sensitive: &expanded_sensitive,
        target_dir,
        delete_archive: delete_archives,
    };

    let mut result = ArchiveImportResultDto {
        imported: Vec::new(),
        duplicate_count: 0,
        archives: Vec::new(),
    };
    for request in &requests {
        let outcome = extract_one(&ctx, request, &mut result, &mut import_dir, &mut on_progress);
        on_progress(&request.path, if outcome.error.is_some() { "failed" } else { "done" });
        result.archives.push(outcome);
    }
    Ok(result)
}

/// Ablauf von `extract_archives`: ZUERST alle Zielordner-Pruefungen, erst
/// danach werden die Archive aus der Freigabeliste verbraucht. So kann der
/// Nutzer nach einem abgelehnten Ziel mit einem anderen Ordner erneut
/// starten.
/// `extract` fuehrt das eigentliche Entpacken der freigegebenen Anfragen
/// aus (im Befehl: `extract_archives_core`).
pub(crate) fn authorize_and_extract(
    pending: &PendingArchives,
    sensitive_dirs: &[PathBuf],
    target_dir: &Path,
    target_approved: bool,
    requests: Vec<ArchiveRequest>,
    extract: impl FnOnce(Vec<ArchiveRequest>) -> CmdResult<ArchiveImportResultDto>,
) -> CmdResult<ArchiveImportResultDto> {
    check_target_location(target_dir, &expand_sensitive_dirs(sensitive_dirs))?;
    if !target_approved {
        return Err("Zielordner wurde nicht ueber die App ausgewaehlt".to_string());
    }
    let (allowed, rejected) = pending.take_authorized(requests);
    let mut result = extract(allowed)?;
    result.archives.extend(rejected);
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
    approved: State<'_, ApprovedTargets>,
    target_dir: String,
    requests: Vec<ArchiveRequest>,
    delete_archives: bool,
) -> CmdResult<ArchiveImportResultDto> {
    let target = Path::new(&target_dir);
    let target_approved = {
        let conn = lock_db(&state)?;
        target_is_approved(&conn, &approved, target)
    };
    authorize_and_extract(&pending, &state.sensitive_dirs, target, target_approved, requests, |allowed| {
        extract_archives_core(
            &state.sensitive_dirs,
            target,
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
        )
    })
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
            std::slice::from_ref(&target),
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
    fn merging_into_a_folder_that_contains_a_protected_path_is_refused() {
        // Nachbau des ".local.zip"-Angriffs: Das Ziel ist erlaubt, `dest`
        // enthaelt aber einen geschuetzten Bereich - seit F2c wird dann gar
        // nichts entpackt (vorher: nur die betroffenen Eintraege uebersprungen).
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
            |d| import_extracted_dir(&mut conn, d),
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
            |d| import_extracted_dir(&mut conn, d),
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

    fn run_core(conn: &mut Connection, target: &Path, requests: Vec<ArchiveRequest>) -> CmdResult<ArchiveImportResultDto> {
        extract_archives_core(&[], target, requests, false, |d| import_extracted_dir(conn, d), |_, _| {})
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

        let pending = PendingArchives::default();
        pending.observe_drop(&[dropped.clone(), model.clone(), dir.join("fehlt.zip")]);

        let s = |p: &Path| p.to_string_lossy().to_string();
        let claimed = pending.claim_dropped(vec![s(&dropped), s(&foreign)]);
        assert_eq!(claimed, vec![s(&dropped)], "nur beobachtete Archive werden uebernommen");

        let (allowed, rejected) =
            pending.take_authorized(vec![plain_request(&dropped), plain_request(&foreign)]);
        assert_eq!(allowed.len(), 1);
        assert_eq!(allowed[0].path, s(&dropped));
        assert_eq!(rejected.len(), 1);
        assert_eq!(rejected[0].path, s(&foreign));

        // Eine Beobachtung gilt nur fuer EINEN import_dropped-Aufruf.
        assert!(pending.claim_dropped(vec![s(&dropped)]).is_empty());
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
        let pending = PendingArchives::default();
        pending.register(&[archive.to_string_lossy().to_string()]);

        let mut conn = crate::db::connect_in_memory().unwrap();
        let first = authorize_and_extract(
            &pending,
            &[],
            &target,
            false,
            vec![plain_request(&archive)],
            |allowed| run_core(&mut conn, &target, allowed),
        );
        assert!(first.unwrap_err().contains("nicht ueber die App ausgewaehlt"));
        assert!(!target.join("Drache").exists());

        // Missing target folder: also nothing consumed.
        let missing = authorize_and_extract(
            &pending,
            &[],
            &dir.join("gibt-es-nicht"),
            true,
            vec![plain_request(&archive)],
            |allowed| run_core(&mut conn, &target, allowed),
        );
        assert!(missing.is_err());

        let retry = authorize_and_extract(
            &pending,
            &[],
            &target,
            true,
            vec![plain_request(&archive)],
            |allowed| run_core(&mut conn, &target, allowed),
        )
        .expect("zweiter Versuch mit gueltigem Ziel");
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

        // Zielordner = Home selbst (Vorfahr von ~/.ssh).
        let as_target = extract_archives_core(
            std::slice::from_ref(&ssh),
            &home,
            vec![plain_request(&archive)],
            false,
            |d| import_extracted_dir(&mut conn, d),
            |_, _| {},
        );
        assert!(as_target.is_err());

        // Zusammenfuehren in einen Unterordner, der per Symlink auf "Home"
        // zeigt: das Ziel selbst ist unverdaechtig, `dest` loest aber zu
        // einem Vorfahren von ~/.ssh auf.
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
                |d| import_extracted_dir(&mut conn, d),
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
            |_| Err("Datenbank weg".to_string()),
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
