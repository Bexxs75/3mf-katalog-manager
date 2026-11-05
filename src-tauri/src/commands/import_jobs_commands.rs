//! Tauri entrypoints and compatibility wrappers for the import service.
use super::*;

fn kick(app: tauri::AppHandle) {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let service = &state.import_jobs;
        while let Some(job) = service.next() {
            let heart = job.clone();
            let heartbeat_thread = std::thread::spawn(move || heartbeat(heart));
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match job.input() {
                    JobInput::Models(roots) => {
                        run_models(service, &job, &mut &state.db, roots, &state.sensitive_dirs).map_err(CmdError::from)
                    }
                    JobInput::Archives {
                        target,
                        requests,
                        delete,
                    } => run_archives(&app, &state, &job, target, requests.clone(), *delete),
                }));
            let error = match result {
                Ok(Ok(())) => None,
                Ok(Err(e)) => Some(ImportJobError {
                    kind: "database".into(),
                    message: e.message,
                }),
                Err(_) => Some(ImportJobError {
                    kind: "internal".into(),
                    message: "import worker panicked".into(),
                }),
            };
            service.finish(&job, error);
            let _ = heartbeat_thread.join();
        }
    });
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartedImport {
    pub job_id: Option<String>,
}

pub(crate) fn submit_models(
    app: &tauri::AppHandle,
    source: ImportSource,
    paths: Vec<PathBuf>,
    target: Option<String>,
    allow_picker: bool,
) -> CmdResult<Arc<ImportJob>> {
    submit_models_confined(app, source, paths, target, allow_picker, Vec::new())
}

/// Whether the import resolves a target folder and moves the models into it. Files and
/// folder contents always do. A drop only does when the caller named a folder; without
/// one it is catalogued in place, as before.
fn places_into_target(source: ImportSource, target: Option<&str>) -> bool {
    matches!(source, ImportSource::Files | ImportSource::FolderContents)
        || (source == ImportSource::Dropped && target.is_some())
}
/// Target folder id, whether the models are moved into it, and whether the target is
/// unusable (not a catalog folder). A drop's target is never replaced by the catalog
/// root; the worker checks later that the folder exists on disk and is not protected.
fn resolve_target(
    source: ImportSource,
    target: Option<String>,
    base: Option<&str>,
    folders: &[db::models::FolderRecord],
) -> (Option<String>, bool, bool) {
    let dropped = source == ImportSource::Dropped;
    let placement = base.is_some() || dropped;
    let target = target.or_else(|| {
        folders
            .iter()
            .find(|f| base == Some(f.path.as_str()))
            .map(|f| f.id.to_string())
    });
    let missing = (placement || target.is_some())
        && !target
            .as_ref()
            .is_some_and(|id| folders.iter().any(|f| f.id.to_string() == *id));
    (target, placement, missing)
}
/// Like [`submit_models`], but the worker only takes files that still lie below one of
/// `confine_to` (and are reached without a link) when it gets to them.
pub(crate) fn submit_models_confined(
    app: &tauri::AppHandle,
    source: ImportSource,
    paths: Vec<PathBuf>,
    target: Option<String>,
    allow_picker: bool,
    confine_to: Vec<PathBuf>,
) -> CmdResult<Arc<ImportJob>> {
    use tauri::Manager;
    let state = app.state::<AppState>();
    let service = &state.import_jobs;
    // Hold the share while validating and consuming proofs, closing the race
    // with an exclusive catalog operation before actual queue insertion.
    let _admission = service.gate.import();
    let (source, mut error) =
        match service.authorize_models(source, &paths, allow_picker, &state.sensitive_dirs) {
            Ok(source) => (source, None),
            Err(error) => (
                source,
                Some(ImportJobError {
                    kind: "unauthorized".into(),
                    message: error.message,
                }),
            ),
        };
    let resolved: CmdResult<(Option<String>, bool)> = (|| {
        if !places_into_target(source, target.as_deref()) {
            return Ok((None, false));
        }
        let conn = lock_db(&state)?;
        let base = db::printer_link::get_setting(&conn, folders::SETTING_CATALOG_BASE_DIR)
            .map_err(|e| e.to_string())?;
        let folders = db::list_folders(&conn).map_err(|e| e.to_string())?;
        let (target, placement, missing) =
            resolve_target(source, target, base.as_deref(), &folders);
        if missing && error.is_none() {
            error = Some(ImportJobError {
                kind: "targetMissing".into(),
                message: "Importziel fehlt".into(),
            });
        }
        Ok((target, placement))
    })();
    let (target, placement) = match resolved {
        Ok(target) => target,
        Err(database_error) => {
            error = Some(ImportJobError {
                kind: "database".into(),
                message: database_error.message,
            });
            (None, false)
        }
    };
    let job = service.enqueue_confined(
        source,
        target,
        placement,
        None,
        JobInput::Models(paths),
        error,
        confine_to,
    )?;
    kick(app.clone());
    Ok(job)
}
pub(super) async fn wait_legacy(job: Arc<ImportJob>) -> CmdResult<ImportResultDto> {
    tauri::async_runtime::spawn_blocking(move || {
        while job.result().is_none() { std::thread::sleep(Duration::from_millis(10)); }
        let result = job.result().unwrap();
        if let Some(error) = result.job_error { return Err(if matches!(error.kind.as_str(), "database" | "internal") { error.message.into() } else { CmdError::expected(error.message) }); }
        let legacy = job.take_legacy().unwrap_or_else(files::empty_import_result);
        Ok(legacy)
    }).await.map_err(|e| e.to_string())?
}
pub(super) fn pick_models(
    app: &tauri::AppHandle,
    source: ImportSource,
) -> CmdResult<Option<Vec<PathBuf>>> {
    if matches!(source, ImportSource::Folder | ImportSource::FolderContents) {
        return Ok(app
            .dialog()
            .file()
            .blocking_pick_folder()
            .and_then(|p| p.into_path().ok())
            .map(|p| vec![p]));
    }
    if source != ImportSource::Files {
        return Err(CmdError::expected("unauthorized"));
    }
    let mut extensions = vec!["3mf", "stl", "stp", "step", "obj", "gcode", "bgcode"];
    extensions.extend_from_slice(katalog_core::import::files::RESIN_EXTENSIONS);
    extensions.extend_from_slice(crate::archive::DIALOG_EXTENSIONS);
    Ok(app
        .dialog()
        .file()
        .add_filter("3D-Modelle & Archive", &extensions)
        .blocking_pick_files()
        .map(|p| p.into_iter().filter_map(|p| p.into_path().ok()).collect()))
}
#[tauri::command]
pub async fn start_import(
    app: tauri::AppHandle,
    source: ImportSource,
    target_folder_id: Option<String>,
) -> CmdResult<StartedImport> {
    tauri::async_runtime::spawn_blocking(move || {
        if !matches!(source, ImportSource::Files | ImportSource::Folder | ImportSource::FolderContents) {
            use tauri::Manager;
            let state = app.state::<AppState>();
            let job = state.import_jobs.enqueue_checked(
                ImportSource::Files,
                None,
                false,
                None,
                JobInput::Models(Vec::new()),
                Some(ImportJobError {
                    kind: "unauthorized".into(),
                    message: "Ungültige Importquelle".into(),
                }),
            )?;
            return Ok(StartedImport {
                job_id: Some(job.id.clone()),
            });
        }
        let Some(paths) = pick_models(&app, source)? else {
            return Ok(StartedImport { job_id: None });
        };
        Ok(StartedImport {
            job_id: Some(
                submit_models(&app, source, paths, target_folder_id, false)?
                    .id
                    .clone(),
            ),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn start_dropped_import(
    app: tauri::AppHandle,
    paths: Vec<String>,
    target_folder_id: Option<String>,
) -> CmdResult<StartedImport> {
    tauri::async_runtime::spawn_blocking(move || {
        Ok(StartedImport {
            job_id: Some(
                submit_models(
                    &app,
                    ImportSource::Dropped,
                    paths.into_iter().map(PathBuf::from).collect(),
                    target_folder_id,
                    false,
                )?
                .id
                .clone(),
            ),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn start_adopt_import(app: tauri::AppHandle, path: String) -> CmdResult<StartedImport> {
    tauri::async_runtime::spawn_blocking(move || {
        Ok(StartedImport {
            job_id: Some(
                submit_models(
                    &app,
                    ImportSource::SetupAdopt,
                    vec![PathBuf::from(path)],
                    None,
                    false,
                )?
                .id
                .clone(),
            ),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn cancel_import(state: State<AppState>, job_id: String) -> Option<serde_json::Value> {
    state
        .import_jobs
        .cancel(&job_id)
        .map(|state| serde_json::json!({"state":state}))
}
#[tauri::command]
pub fn get_import_result(state: State<AppState>, job_id: String) -> Option<ImportJobResult> {
    state.import_jobs.get(&job_id)?.result()
}
#[tauri::command]
pub fn get_import_state(state: State<AppState>, job_id: String) -> Option<ImportProgress> {
    Some(state.import_jobs.get(&job_id)?.progress())
}

pub(crate) async fn legacy_models(
    app: tauri::AppHandle,
    source: ImportSource,
    paths: Option<Vec<PathBuf>>,
    target: Option<String>,
) -> CmdResult<ImportResultDto> {
    let job = tauri::async_runtime::spawn_blocking(move || {
        let paths = match paths {
            Some(paths) => paths,
            None => match pick_models(&app, source)? {
                Some(paths) => paths,
                None => return Ok(None),
            },
        };
        submit_models(&app, source, paths, target, true).map(Some)
    })
    .await
    .map_err(|e| e.to_string())??;
    match job {
        Some(job) => wait_legacy(job).await,
        None => Ok(files::empty_import_result()),
    }
}
fn submit_archives(
    app: &tauri::AppHandle,
    target: String,
    requests: Vec<ArchiveRequest>,
    delete: bool,
    parent: Option<String>,
) -> CmdResult<Arc<ImportJob>> {
    use tauri::Manager;
    let state = app.state::<AppState>();
    let service = &state.import_jobs;
    let _admission = service.gate.import();
    let target = PathBuf::from(target);
    let target_approved = {
        let conn = lock_db(&state)?;
        archives::target_is_approved(&conn, &app.state::<ApprovedTargets>(), &target)
    };
    let claimed = service.authorize_archives(
        &state.sensitive_dirs,
        &target,
        target_approved,
        &requests,
        parent.as_deref(),
    );
    let (derived_parent, authorized) = match claimed {
        Ok(parent) => (parent, true),
        Err(_) => (parent, false),
    };
    let job = service.enqueue_checked(
        ImportSource::Archive,
        None,
        false,
        derived_parent,
        JobInput::Archives {
            target,
            requests,
            delete,
        },
        (!authorized).then(|| ImportJobError {
            kind: "unauthorized".into(),
            message: "Archivfreigabe fehlt oder ist abgelaufen".into(),
        }),
    )?;
    kick(app.clone());
    Ok(job)
}
#[tauri::command]
pub async fn start_archive_import(
    app: tauri::AppHandle,
    target_dir: String,
    requests: Vec<ArchiveRequest>,
    delete_archives: bool,
    parent_job_id: Option<String>,
) -> CmdResult<StartedImport> {
    tauri::async_runtime::spawn_blocking(move || {
        Ok(StartedImport {
            job_id: Some(
                submit_archives(&app, target_dir, requests, delete_archives, parent_job_id)?
                    .id
                    .clone(),
            ),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
pub(crate) async fn legacy_archives(
    app: tauri::AppHandle,
    target: String,
    requests: Vec<ArchiveRequest>,
    delete: bool,
) -> CmdResult<ArchiveImportResultDto> {
    use tauri::Manager;
    let jobs = tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _admission = state.import_jobs.gate.import();
        state
            .import_jobs
            .partition_archives(requests)
            .into_iter()
            .map(|(parent, requests)| {
                submit_archives(&app, target.clone(), requests, delete, parent)
            })
            .collect::<CmdResult<Vec<_>>>()
    })
    .await
    .map_err(|e| e.to_string())??;
    tauri::async_runtime::spawn_blocking(move || {
        let mut combined = ArchiveImportResultDto {
            imported: Vec::new(),
            duplicate_count: 0,
            archives: Vec::new(),
            skipped: Vec::new(),
        };
        for job in jobs {
            while job.result().is_none() {
                std::thread::sleep(Duration::from_millis(10));
            }
            if let Some(result) = job.take_archive_result() {
                combined.imported.extend(result.imported);
                combined.duplicate_count += result.duplicate_count;
                combined.archives.extend(result.archives);
                combined.skipped.extend(result.skipped);
            } else {
                let message = job
                    .result()
                    .and_then(|r| r.job_error)
                    .map(|e| e.message)
                    .unwrap_or_else(|| "cancelled".into());
                if let JobInput::Archives { requests, .. } = job.input() {
                    combined
                        .archives
                        .extend(requests.iter().map(|r| ArchiveOutcomeDto {
                            path: r.path.clone(),
                            error: Some(message.clone()),
                            ..Default::default()
                        }));
                }
            }
        }
        Ok(combined)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn discard_archive_imports(state: State<AppState>, paths: Vec<String>) {
    let paths: HashSet<_> = paths.into_iter().map(PathBuf::from).collect();
    state.import_jobs.discard_archives(&paths.into_iter().collect::<Vec<_>>());
}

#[cfg(test)]
mod drop_target_tests {
    use super::*;
    use db::models::FolderRecord;

    fn folder(id: i64, path: &str) -> FolderRecord {
        FolderRecord { id, name: path.into(), parent_id: None, path: path.into() }
    }

    #[test]
    fn a_drop_without_target_is_not_placed() {
        assert!(!places_into_target(ImportSource::Dropped, None));
        assert!(places_into_target(ImportSource::Dropped, Some("1")));
        assert!(places_into_target(ImportSource::Files, None));
        assert!(!places_into_target(ImportSource::Folder, Some("1")));
    }

    #[test]
    fn a_drop_with_a_catalog_folder_is_placed_there_even_without_catalog_root() {
        let folders = [folder(1, "/a"), folder(2, "/b")];
        let (target, placement, missing) =
            resolve_target(ImportSource::Dropped, Some("2".into()), None, &folders);
        assert_eq!((target.as_deref(), placement, missing), (Some("2"), true, false));
        // The catalog root does not replace the named folder.
        let (target, _, _) =
            resolve_target(ImportSource::Dropped, Some("2".into()), Some("/a"), &folders);
        assert_eq!(target.as_deref(), Some("2"));
    }

    #[test]
    fn a_drop_target_outside_the_catalog_folders_is_missing() {
        let folders = [folder(1, "/a")];
        for bad in ["99", "x", ""] {
            let (_, placement, missing) =
                resolve_target(ImportSource::Dropped, Some(bad.into()), None, &folders);
            assert!(placement && missing, "{bad:?} must be refused");
        }
    }

    #[test]
    fn files_keep_their_catalog_root_fallback() {
        let folders = [folder(7, "/root")];
        let (target, placement, missing) =
            resolve_target(ImportSource::Files, None, Some("/root"), &folders);
        assert_eq!((target.as_deref(), placement, missing), (Some("7"), true, false));
        let (target, placement, missing) =
            resolve_target(ImportSource::Files, None, None, &folders);
        assert_eq!((target, placement, missing), (None, false, false));
    }
}
