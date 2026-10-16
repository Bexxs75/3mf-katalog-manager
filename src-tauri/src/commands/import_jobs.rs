//! Tauri adapters for the transport-neutral import service.
use super::*;
use std::sync::{Arc, atomic::Ordering};
#[cfg(test)] use std::sync::atomic::AtomicU64;
use std::time::Duration;
#[allow(unused_imports)] // Retain the existing desktop DTO/control API.
pub use katalog_core::import::{self as core, ArchivePath, ArchiveTypes, CatalogGate, CatalogShare,
    ImportSource, ImportState, ImportCounts, ImportedEntry, NotPlacedEntry, DuplicateEntry, SkippedEntry,
    ImportGroups, ImportJobError, ImportJobResult, ImportProgress, ImportClock, ImportEvents, ImportReader,
    TargetChecker, JobInput, NoEvents, RealClock, PathGrants, heartbeat, run_models};
use super::files;
#[path = "import_jobs_commands.rs"]
mod api;
#[path = "import_jobs_worker.rs"]
mod worker;
pub use api::*;
use worker::run_archives;
#[derive(Default)]
pub struct DesktopArchives;
impl ArchivePath for ArchiveRequest { fn path(&self) -> &Path { Path::new(&self.path) } }
impl ArchiveTypes for DesktopArchives { type Request = ArchiveRequest; type Result = ArchiveImportResultDto; }
pub type ImportJobs = core::ImportJobs<DesktopArchives>;
pub type ImportJob = core::ImportJob<DesktopArchives>;
struct FileReader;
impl ImportReader for FileReader {
    fn read(&self, path: &Path) -> core::ImportResult<NewFile> {
        let mut file = files::read_import_model_file(path, None).map_err(core::ImportError::from)?;
        file.content_hash = Some(files::compute_content_hash(path).map_err(core::ImportError::from)?);
        Ok(file)
    }
}
struct DiskTarget;
impl TargetChecker for DiskTarget {
    fn exists(&self, path: &Path) -> bool {
        path.is_dir()
    }
}
fn desktop_import_diagnostic(error: &core::ImportError) -> CmdError {
    CmdError::from(error.clone())
}
struct AppEvents(tauri::AppHandle);
impl ImportEvents for AppEvents {
    fn diagnostic(&self, error: &core::ImportError) { let _ = desktop_import_diagnostic(error); }
    fn finished(&self, result: &ImportJobResult, seconds: f64) {
        for line in skipped_log_lines(&result.groups, log::max_level() >= log::LevelFilter::Debug) { log::warn!(target: "import", "{line}"); }
        log::info!(target: "import", "{}", job_summary(result, seconds));
    }
    fn emit(&self, name: &str, payload: serde_json::Value) {
        use tauri::Emitter;
        let _ = self.0.emit(name, payload);
    }
}
pub trait DesktopJobs: Sized {
    #[cfg(test)] fn default() -> Self;
    fn for_app(app: tauri::AppHandle) -> Self;
    fn authorize_archives(&self, sensitive: &[PathBuf], target: &Path, target_approved: bool, requests: &[ArchiveRequest], parent: Option<&str>) -> CmdResult<Option<String>>;
    #[cfg(test)] fn test_handoff(&self, paths: Vec<PathBuf>) -> String;
}
impl DesktopJobs for ImportJobs {
    #[cfg(test)] fn default() -> Self { Self::new(Arc::new(FileReader), Arc::new(DiskTarget), Arc::new(RealClock::default()), Arc::new(NoEvents)) }
    fn for_app(app: tauri::AppHandle) -> Self { Self::new(Arc::new(FileReader), Arc::new(DiskTarget), Arc::new(RealClock::default()), Arc::new(AppEvents(app))) }
    fn authorize_archives(
        &self,
        sensitive: &[PathBuf],
        target: &Path,
        target_approved: bool,
        requests: &[ArchiveRequest],
        parent: Option<&str>,
    ) -> CmdResult<Option<String>> {
        archives::check_target_location(target, &expand_sensitive_dirs(sensitive))?;
        if !target_approved {
            return Err(CmdError::expected(
                "Zielordner wurde nicht ueber die App ausgewaehlt",
            ));
        }
        self.claim_archives(requests, parent)
            .ok_or_else(|| CmdError::expected("unauthorized"))
    }

    #[cfg(test)] fn test_handoff(&self, paths: Vec<PathBuf>) -> String {
        let job = self.enqueue_work(ImportSource::Dropped,None,false,None,JobInput::Models(paths.clone())).unwrap();
        self.next().unwrap();
        let mut conn = db::connect_in_memory().unwrap();
        run_models(self, &job, &mut &mut conn, &paths, &[]).unwrap();
        self.finish(&job,None); job.id.clone()
    }
}
#[cfg(test)]
#[path = "import_handoff_tests.rs"]
mod import_handoff_tests;
fn skip_reason(key: &str) -> &str {
    match key {
        "empty" => "Leere Datei (0 Byte)",
        "invalid" => "Datei ungültig – konnte nicht gelesen werden",
        "failed" => "Datei konnte nicht importiert werden",
        "unsupported" => "Dateityp wird nicht unterstützt",
        "path" => "Gleicher Pfad im Katalog",
        "hash" => "Gleicher Inhalt im Katalog oder Import",
        "unsafe" => "Unsicherer Pfad, übersprungen",
        "blocked" => "Blockierter Dateityp, übersprungen",
        "existing" => "Zieldatei vorhanden, übersprungen",
        "notStarted" => "Nicht begonnen",
        "cancelled" => "Importiert, aber nicht abgelegt: Import abgebrochen",
        "targetMissing" => "Importiert, aber nicht abgelegt: Zielordner wurde entfernt",
        "moveFailed" => "Importiert, aber nicht abgelegt: Verschieben fehlgeschlagen",
        "jobFailed" => "Importiert, aber nicht abgelegt: Auftrag fehlgeschlagen",
        "protected" => "Importiert, aber nicht abgelegt: geschützter Pfad",
        other => other,
    }
}

fn skipped_log_lines(groups: &ImportGroups, verbose: bool) -> Vec<String> {
    let mut entries: Vec<(&str, &str)> = groups.skipped.iter().map(|e| (e.path.as_str(), e.reason.as_str()))
        .chain(groups.duplicate.iter().map(|e| (e.path.as_str(), e.kind.as_str()))).collect();
    for archive in &groups.archive {
        for (group, reason) in [("skipped", "reason"), ("duplicates", "kind")] {
            if let Some(rows) = archive["models"][group].as_array() {
                for row in rows {
                    entries.push((row["entryPath"].as_str().unwrap_or(""), row[reason].as_str().unwrap_or("invalid")));
                }
            }
        }
    }
    if entries.is_empty() { return Vec::new(); }
    if !verbose { return vec![format!("übersprungen: {} {} (einschließlich Duplikate)", entries.len(), if entries.len() == 1 { "Datei" } else { "Dateien" })]; }
    entries.into_iter().map(|(path, reason)| format!("übersprungen: {} ({})", path.replace(['\n', '\r'], " "), skip_reason(reason))).collect()
}

fn job_summary(result: &ImportJobResult, secs: f64) -> String {
    let (mut imported, mut duplicate, mut skipped) = (result.counts.imported + result.counts.imported_not_placed, result.counts.duplicate, result.counts.skipped);
    for archive in &result.groups.archive {
        let count = |key: &str| archive["models"][key].as_array().map_or(0, Vec::len);
        imported += count("imported"); duplicate += count("duplicates"); skipped += count("skipped");
    }
    let pending = result.groups.archive.iter().filter(|a| a["state"] == "pending").count();
    let source = match result.source { ImportSource::Files => "Dateien", ImportSource::Folder => "Ordner", ImportSource::FolderContents => "Ordnerinhalt", ImportSource::Dropped => "Ablegen", ImportSource::SetupAdopt => "Übernahme", ImportSource::Archive => "Archiv" };
    format!("{source}: {imported} importiert, {duplicate} Duplikate, {skipped} übersprungen, {pending} Archive offen, {secs:.1} s")
}

#[cfg(test)]
mod logging_tests {
    use super::*;
    #[test]
    fn multiword_archive_names_are_removed_from_generated_verbose_lines() {
        use crate::diagnostics::anonymize::{anonymize, to_text, Context};
        let mut groups = ImportGroups::default();
        groups.archive.push(serde_json::json!({"models":{"skipped":[
            {"entryPath":"Anna Weber.stl","reason":"invalid"},
            {"entryPath":"Anna Weber (privat).stl","reason":"invalid"}
        ]}}));
        for line in skipped_log_lines(&groups, true) {
            let ctx = Context { replace_file_names: true, ..Default::default() };
            let text = to_text(&anonymize(&line, &ctx).unwrap());
            assert!(!text.contains("Anna"));
            assert!(!text.contains("Weber"));
            assert!(!text.contains("privat"));
            assert!(text.contains("übersprungen:"));
        }
    }
    #[test]
    fn skipped_logs_hide_names_until_verbose_mode_and_include_nested_duplicates() {
        let mut groups = ImportGroups::default();
        groups.skipped.push(SkippedEntry {path: "/private/broken.obj".into(), reason: "invalid".into()});
        groups.duplicate.push(DuplicateEntry {path: "/private/same.stl".into(), kind: "hash".into(), existing_file_id: None});
        groups.archive.push(serde_json::json!({"models":{"skipped":[{"entryPath":"../unsafe.stl","reason":"unsafe"}],"duplicates":[]}}));
        assert_eq!(skipped_log_lines(&groups, false), ["übersprungen: 3 Dateien (einschließlich Duplikate)"]);
        assert_eq!(skipped_log_lines(&groups, true), [
            "übersprungen: /private/broken.obj (Datei ungültig – konnte nicht gelesen werden)",
            "übersprungen: /private/same.stl (Gleicher Inhalt im Katalog oder Import)",
            "übersprungen: ../unsafe.stl (Unsicherer Pfad, übersprungen)",
        ]);
    }
    #[test]
    fn final_summary_includes_source_archive_models_and_pending_archives() {
        let service = ImportJobs::default();
        let job = service.enqueue(ImportSource::Archive, None, false, None).unwrap();
        job.push_archive(serde_json::json!({"state":"finished","models":{"imported":[{},{}],"duplicates":[{}],"skipped":[{}]}}));
        job.push_archive(serde_json::json!({"state":"pending"}));
        service.finish(&job, None);
        let result = job.result().unwrap();
        assert_eq!(job_summary(&result, 1.34), "Archiv: 2 importiert, 1 Duplikate, 1 übersprungen, 1 Archive offen, 1.3 s");
        assert!(skipped_log_lines(&ImportGroups::default(), false).is_empty());
    }
}

#[cfg(test)]
#[test]
fn skipped_log_filename_replacement_uses_the_existing_preview_rules() {
    use crate::diagnostics::anonymize::{anonymize, to_text, Context};
    let line = "WARN [import] übersprungen: /private/broken.obj (Datei ungültig – konnte nicht gelesen werden)";
    let ctx = Context { replace_file_names: true, ..Context::default() };
    let preview = to_text(&anonymize(line, &ctx).unwrap());
    assert!(!preview.contains("broken.obj"));
    assert!(preview.contains("<datei>"));
    assert!(preview.contains("WARN [import] übersprungen:"));
}

#[cfg(test)]
#[test]
fn desktop_reader_and_error_adapter_preserve_import_contract() {
    let dir = unique_test_dir("desktop_import_reader_adapter");
    let path = dir.join("model.stl");
    let mut bytes = vec![0; 80]; bytes.extend(1u32.to_le_bytes());
    for v in [0.0f32,0.,1.,0.,0.,0.,10.,0.,0.,0.,10.,1.] { bytes.extend(v.to_le_bytes()); }
    bytes.extend(0u16.to_le_bytes()); std::fs::write(&path,bytes).unwrap();
    let row = FileReader.read(&path).unwrap();
    assert_eq!(row.path, path.to_string_lossy());
    assert_eq!(row.content_hash, Some(files::compute_content_hash(&path).unwrap()));
    std::fs::write(&path, b"invalid").unwrap();
    assert!(!FileReader.read(&path).unwrap_err().expected);
    for expected in [true,false] {
        let original = core::ImportError { message: "unchanged".into(), expected, code: Some(GeometryErrorCode::Unreadable) };
        let encoded = serde_json::to_value(&original).unwrap();
        let desktop = CmdError::from(original);
        assert_eq!(serde_json::to_value(&desktop).unwrap(),encoded);
        assert_eq!(serde_json::to_value(core::ImportError::from(desktop)).unwrap(),encoded);
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(test)]
#[test]
fn desktop_placement_diagnostics_keep_failed_rollback_recovery_details() {
    use std::cell::RefCell;
    thread_local! { static BLOCK_ROLLBACK: RefCell<Option<PathBuf>> = const { RefCell::new(None) }; }
    struct Diagnostics(Mutex<Vec<CmdError>>);
    impl ImportEvents for Diagnostics {
        fn emit(&self, _: &str, _: serde_json::Value) {}
        fn diagnostic(&self, error: &core::ImportError) {
            // Same diagnostic boundary as AppEvents: preserve the original error
            // fields and let CmdError apply the existing desktop logging policy.
            self.0.lock().unwrap().push(desktop_import_diagnostic(error));
        }
    }
    let dir = unique_test_dir("import_failed_rollback_diagnostics");
    let source = dir.join("model.stl");
    let target = dir.join("target");
    std::fs::create_dir(&target).unwrap();
    let mut bytes = vec![0; 80]; bytes.extend(1u32.to_le_bytes());
    for v in [0.0f32,0.,1.,0.,0.,0.,10.,0.,0.,0.,10.,1.] { bytes.extend(v.to_le_bytes()); }
    bytes.extend(0u16.to_le_bytes()); std::fs::write(&source,&bytes).unwrap();
    let mut conn = db::connect_in_memory().unwrap();
    let target_id = db::insert_folder_with_parent(&conn,"target",None,target.to_str().unwrap()).unwrap();
    conn.execute_batch("CREATE TRIGGER fail_placement BEFORE UPDATE OF folder_id ON files BEGIN SELECT RAISE(ABORT, 'forced placement failure'); END;").unwrap();
    BLOCK_ROLLBACK.with(|path| *path.borrow_mut() = Some(source.clone()));
    conn.trace_v2(rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT, Some(|event| {
        let rusqlite::trace::TraceEvent::Stmt(_, sql) = event else { return; };
        if sql.starts_with("UPDATE files SET folder_id") {
            BLOCK_ROLLBACK.with(|path| {
                if let Some(path) = path.borrow_mut().take() { std::fs::write(path,b"concurrent replacement").unwrap(); }
            });
        }
    }));
    let diagnostics = Arc::new(Diagnostics(Mutex::new(Vec::new())));
    let service = ImportJobs::new(Arc::new(FileReader),Arc::new(DiskTarget),Arc::new(RealClock::default()),diagnostics.clone());
    let job = service.enqueue_work(ImportSource::Files,Some(target_id.to_string()),true,None,JobInput::Models(vec![source.clone()])).unwrap();
    service.next().unwrap();
    run_models(&service,&job,&mut &mut conn,std::slice::from_ref(&source),&[]).unwrap();
    service.finish(&job,None);
    let result = job.result().unwrap();
    assert_eq!(result.state,ImportState::Finished);
    assert_eq!(result.groups.imported_not_placed[0].reason,"moveFailed");
    assert_eq!(std::fs::read(target.join("model.stl")).unwrap(),bytes);
    assert_eq!(std::fs::read(&source).unwrap(),b"concurrent replacement");
    let recorded = diagnostics.0.lock().unwrap();
    assert_eq!(recorded.len(),1,"handled placement errors must reach desktop diagnostics");
    let error = &recorded[0];
    assert!(!error.expected);
    assert!(error.message.contains("forced placement failure"));
    assert!(error.message.contains("UND Rollback der Dateiverschiebung fehlgeschlagen"));
    assert!(error.message.contains(target.join("model.stl").to_str().unwrap()));
    assert!(error.message.contains(source.to_str().unwrap()));
    drop(recorded);
    BLOCK_ROLLBACK.with(|path| *path.borrow_mut() = None);
    let _ = std::fs::remove_dir_all(dir);
}
