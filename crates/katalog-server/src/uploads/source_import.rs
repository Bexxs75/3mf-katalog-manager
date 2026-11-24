//! Source copies share the upload job controls and catalog writer lifetime.
use super::*;
use crate::source_dirs::SourceAdapter;
use katalog_core::{
    error_code::ErrorCode,
    import::{sources, ImportError},
    upload_fs::Directory,
};
use std::io::{Read, Write};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartSource {
    root_idx: usize,
    #[serde(default)]
    rel: String,
    target_folder_id: Option<String>,
}

pub(super) async fn start(app: Arc<App>, session: Session, args: Value) -> Response {
    let Ok(args) = serde_json::from_value::<StartSource>(args) else {
        return crate::web::error_coded(422, ErrorCode::ReqInvalidArguments, &[]);
    };
    let guard = match app.geometry.barrier.writer() {
        Ok(g) => g,
        Err(_) => return crate::barrier::maintenance(),
    };
    let _change = app.changes.lock().await;
    if !authenticated(&app, &session) {
        return error(401, "session_expired");
    }
    // A forbidden root index or relative path is rejected up front instead of
    // failing later inside the job; link and file checks stay in the job.
    let adapter = SourceAdapter {
        roots: &app.config.source_roots,
    };
    if adapter.requested(args.root_idx, &args.rel).is_err() {
        return crate::web::error_coded(400, ErrorCode::SourceRootNotAllowed, &[]);
    }
    let rt = app.uploads.as_ref().unwrap().clone();
    if let Err(e) = rt.store.ensure_importable() {
        return fail(e);
    }
    let permit = match rt.active.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => return crate::write::retry(503),
    };
    let store = rt.store.clone();
    let upload_guard = match tokio::task::spawn_blocking(move || store.begin_source_import()).await
    {
        Ok(Ok(guard)) => guard,
        Ok(Err(e)) => return fail(e),
        Err(_) => return error(500, "import_internal"),
    };
    let id = match random_id() {
        Ok(id) => id,
        Err(r) => return r,
    };
    let job = Arc::new(Job {
        view: Mutex::new(JobView {
            job_id: id.clone(),
            state: "queued".into(),
            current: None,
            total: 0,
            done: 0,
            files: Vec::new(),
            kind: None,
            source: true,
        }),
        cancel: AtomicBool::new(false),
    });
    rt.jobs.lock().unwrap().insert(id.clone(), job.clone());
    rt.publish(&job, false);
    let response_id = id.clone();
    let worker = app.clone();
    tokio::task::spawn_blocking(move || {
        let (_guard, _permit, _upload_guard) = (guard, permit, upload_guard);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run(&worker, &rt, &job, args)
        }));
        {
            let mut view = job.view.lock().unwrap();
            view.current = None;
            view.state = if job.cancel.load(Ordering::Acquire) {
                "cancelled"
            } else if matches!(result, Ok(Ok(()))) {
                "completed"
            } else {
                "failed"
            }
            .into();
            view.kind = match result {
                Ok(Err(e)) if ErrorCode::ALL.iter().any(|code| code.as_str() == e.message) => {
                    Some(e.message)
                }
                Ok(Err(_)) | Err(_) => Some("import_failed".into()),
                _ => None,
            };
        }
        let value = serde_json::to_value(&*job.view.lock().unwrap()).unwrap();
        let saved = rt.store.save_job(&id, value).is_ok();
        if !saved {
            rt.store.require_restart();
            let mut view = job.view.lock().unwrap();
            view.state = "failed".into();
            view.kind = Some("restart_required".into());
        }
        rt.publish(&job, true);
        if saved {
            rt.jobs.lock().unwrap().remove(&id);
        }
    });
    Json(json!({"jobId":response_id})).into_response()
}

fn io_error(_: std::io::Error) -> ImportError {
    ImportError::coded(ErrorCode::SourceFolderUnreadable)
}

fn stopped(app: &App, job: &Job) -> bool {
    if app.geometry.barrier.snapshot().state != crate::barrier::State::Open {
        job.cancel.store(true, Ordering::Release);
    }
    job.cancel.load(Ordering::Acquire)
}

/// A destination is owned until its database insert has succeeded.
struct CopyFile<'a> {
    dir: &'a Directory,
    name: String,
    keep: bool,
}
impl Drop for CopyFile<'_> {
    fn drop(&mut self) {
        if !self.keep {
            let _ = self.dir.unlink(&self.name);
        }
    }
}

fn copy_file<'a>(
    dir: &'a Directory,
    name: &str,
    mut source: std::fs::File,
    size: u64,
    cancelled: impl Fn() -> bool,
) -> ImportResult<CopyFile<'a>> {
    let path = Path::new(name);
    let stem = path.file_stem().and_then(|n| n.to_str()).unwrap_or("model");
    let ext = path.extension().and_then(|n| n.to_str()).unwrap_or("");
    let mut attempt = 0u64;
    let (name, mut target) = loop {
        let candidate = if attempt == 0 {
            name.to_owned()
        } else {
            format!("{stem} ({attempt}).{ext}")
        };
        match dir.create(&candidate) {
            Ok(file) => break (candidate, file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => attempt += 1,
            Err(e) => return Err(io_error(e)),
        }
        if cancelled() {
            return Err("cancelled".into());
        }
    };
    let owned = CopyFile {
        dir,
        name,
        keep: false,
    };
    let mut remaining = size;
    let mut buffer = [0; 65536];
    while remaining > 0 {
        if cancelled() {
            return Err("cancelled".into());
        }
        let length = buffer.len().min(remaining as usize);
        let n = source.read(&mut buffer[..length]).map_err(io_error)?;
        if n == 0 {
            return Err(ImportError::coded(ErrorCode::SourceFolderMoved));
        }
        target.write_all(&buffer[..n]).map_err(io_error)?;
        remaining -= n as u64;
    }
    if cancelled() || source.read(&mut buffer[..1]).map_err(io_error)? != 0 {
        return Err(ImportError::coded(ErrorCode::SourceFolderMoved));
    }
    target.sync_all().map_err(io_error)?;
    dir.sync().map_err(io_error)?;
    Ok(owned)
}

fn run(app: &App, rt: &Runtime, job: &Job, args: StartSource) -> ImportResult<()> {
    let fs = SourceAdapter {
        roots: &app.config.source_roots,
    };
    let source_root = fs.requested(args.root_idx, &args.rel)?;
    let walked = fs.import_selection(args.root_idx, &args.rel)?;
    let in_place = app.config.roots.iter().any(|r| source_root.starts_with(r));
    let target = if in_place {
        source_root.clone()
    } else if let Some(id) = args.target_folder_id {
        let id: i64 = id
            .parse()
            .map_err(|_| ImportError::coded(ErrorCode::ReqInvalidArguments))?;
        let mut database = Database {
            app,
            cancel: &job.cancel,
            folder: FixedFolder {
                id: 0,
                root: source_root.clone(),
            },
        };
        database.with_conn(|conn| {
            db::list_folders(conn)?
                .into_iter()
                .find(|f| f.id == id)
                .map(|f| PathBuf::from(f.path))
                .ok_or_else(|| ImportError::coded(ErrorCode::SourceNoCatalogFolder))
        })?
    } else {
        app.config
            .roots
            .first()
            .cloned()
            .ok_or_else(|| ImportError::coded(ErrorCode::SourceNoCatalogFolder))?
    };
    if !app.config.roots.iter().any(|r| target.starts_with(r)) {
        return Err(ImportError::coded(ErrorCode::SourceRootNotAllowed));
    }
    let destination = Directory::open(&target).map_err(io_error)?;
    if !in_place {
        sources::check_copy_space(
            walked.files.iter().map(|(_, size)| *size),
            destination.available().map_err(io_error)?,
        )?;
    }
    {
        let mut view = job.view.lock().unwrap();
        view.state = "importing".into();
        view.total = walked.files.len();
    }
    for (index, (source_path, size)) in walked.files.into_iter().enumerate() {
        if stopped(app, job) {
            break;
        }
        let name = source_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| ImportError::coded(ErrorCode::SourcePathInvalid))?;
        job.view.lock().unwrap().current = Some(name.into());
        rt.publish(job, false);
        let source = fs.open_file(&source_path).map_err(io_error)?;
        if source.metadata().map_err(io_error)?.len() != size {
            return Err(ImportError::coded(ErrorCode::SourceFolderMoved));
        }
        let mut copied = if in_place {
            None
        } else {
            Some(copy_file(
                &destination,
                name,
                source.try_clone().map_err(io_error)?,
                size,
                || stopped(app, job),
            )?)
        };
        let path = copied
            .as_ref()
            .map_or_else(|| source_path.clone(), |c| target.join(&c.name));
        let input = match &copied {
            Some(c) => destination.read(&c.name).map_err(io_error)?,
            None => source,
        };
        let parsed = files::read_web_model(
            input,
            &path,
            name,
            app.config.upload.as_ref().unwrap().limits,
        );
        let mut record = FileResult {
            upload_id: format!("source-{index}"),
            name: name.into(),
            state: "invalid".into(),
            file_id: None,
            existing_file_id: None,
            existing_file_name: None,
            kind: None,
            models: None,
            skipped: None,
        };
        if let Ok(new_file) = parsed {
            let parent = path.parent().unwrap();
            let mut database = Database {
                app,
                cancel: &job.cancel,
                folder: FixedFolder {
                    id: 0,
                    root: parent.to_owned(),
                },
            };
            let folder = database.with_conn(|c| {
                db::ensure_folder_path(c, &target, parent).map_err(ImportError::from)
            })?;
            database.folder.id = folder;
            let mut batch = vec![PendingImport {
                path,
                folder_root: None,
                new_file,
            }];
            let mut result = files::empty_import_result();
            let mut duplicates = Vec::new();
            files::store_batch_detailed(
                &mut database,
                &mut batch,
                &mut result,
                files::ImportMode::Batched,
                &mut duplicates,
            )?;
            if let Some(imported) = result.imported.first() {
                if let Some(copy) = copied.as_mut() {
                    copy.keep = true;
                }
                record.state = "imported".into();
                record.file_id = Some(imported.id.clone());
            } else if let Some(duplicate) = duplicates.first() {
                record.state = "duplicate".into();
                record.existing_file_id = duplicate.existing_file_id.clone();
                record.existing_file_name = duplicate.existing_file_name.clone();
                record.kind = Some(duplicate.kind.clone());
            }
        }
        {
            let mut view = job.view.lock().unwrap();
            view.done += 1;
            view.files.push(record);
        }
        rt.publish(job, false);
    }
    Ok(())
}

#[cfg(test)]
#[path = "source_import/tests.rs"]
mod tests;
