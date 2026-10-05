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
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match &job.input {
                    JobInput::Models(roots) => {
                        run_models(service, &job, &mut &state.db, roots, &state.sensitive_dirs)
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

fn submit_models(
    app: &tauri::AppHandle,
    source: ImportSource,
    paths: Vec<PathBuf>,
    target: Option<String>,
    allow_picker: bool,
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
        Ok(if source == ImportSource::Files {
            let conn = lock_db(&state)?;
            let base = db::printer_link::get_setting(&conn, folders::SETTING_CATALOG_BASE_DIR)
                .map_err(|e| e.to_string())?;
            let folders = db::list_folders(&conn).map_err(|e| e.to_string())?;
            let placement = base.is_some();
            let target = target.or_else(|| {
                folders
                    .iter()
                    .find(|f| base.as_deref() == Some(f.path.as_str()))
                    .map(|f| f.id.to_string())
            });
            if (placement || target.is_some())
                && !target
                    .as_ref()
                    .is_some_and(|id| folders.iter().any(|f| f.id.to_string() == *id))
            {
                error = Some(ImportJobError {
                    kind: "targetMissing".into(),
                    message: "Importziel fehlt".into(),
                });
            }
            (target, placement)
        } else {
            (None, false)
        })
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
    let job = service.enqueue_checked(
        source,
        target,
        placement,
        None,
        JobInput::Models(paths),
        error,
    )?;
    kick(app.clone());
    Ok(job)
}
pub(super) async fn wait_legacy(job: Arc<ImportJob>) -> CmdResult<ImportResultDto> {
    tauri::async_runtime::spawn_blocking(move || {
        while job.result().is_none() { std::thread::sleep(Duration::from_millis(10)); }
        let result = job.result().unwrap();
        if let Some(error) = result.job_error { return Err(if matches!(error.kind.as_str(), "database" | "internal") { error.message.into() } else { CmdError::expected(error.message) }); }
        let legacy = job.legacy.lock().unwrap().take().unwrap_or_else(files::empty_import_result);
        Ok(legacy)
    }).await.map_err(|e| e.to_string())?
}
pub(super) fn pick_models(
    app: &tauri::AppHandle,
    source: ImportSource,
) -> CmdResult<Option<Vec<PathBuf>>> {
    if source == ImportSource::Folder {
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
    let mut extensions = vec!["3mf", "stl", "stp", "step", "obj"];
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
        if !matches!(source, ImportSource::Files | ImportSource::Folder) {
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
) -> CmdResult<StartedImport> {
    tauri::async_runtime::spawn_blocking(move || {
        Ok(StartedImport {
            job_id: Some(
                submit_models(
                    &app,
                    ImportSource::Dropped,
                    paths.into_iter().map(PathBuf::from).collect(),
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
            if let Some(result) = job.archive_legacy.lock().unwrap().take() {
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
                if let JobInput::Archives { requests, .. } = &job.input {
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
    state
        .import_jobs
        .archives
        .lock()
        .unwrap()
        .retain(|(_, path), grant| grant.expires.is_none() || !paths.contains(path));
}
