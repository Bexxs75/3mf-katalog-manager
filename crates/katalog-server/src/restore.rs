//! Restore owns its maintenance window independently of the HTTP request.
use super::*;
use crate::restore_journal::{self, Files, Phase, Record};
use katalog_core::{catalog_export, error_code::ErrorCode};
use std::time::Duration;

pub(super) const ROUTE: &str = "/api/backup/restore";
const WAIT: Duration = Duration::from_secs(30);
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    token: String,
    password: String,
}

pub(super) async fn handle(
    app: Arc<App>,
    current: Option<Session>,
    peer: String,
    req: Request,
) -> Response {
    if !app.config.write {
        return error_coded(404, ErrorCode::WebCommandUnavailable, &[]);
    }
    let Some(session) = current.filter(|s| uploads::authenticated(&app, s)) else {
        return error_coded(401, ErrorCode::WebLoginRequired, &[]);
    };
    let restart = req.extensions().get::<crate::transport::Restart>().cloned();
    let bytes = match read_body(req.into_body()).await {
        Ok(b) => b,
        Err(e) => return *e,
    };
    let input: Input = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return error_coded(400, ErrorCode::ReqInvalidArguments, &[]),
    };
    let hash = app
        .data
        .lock()
        .unwrap()
        .auth
        .as_ref()
        .expect("authenticated")
        .password_hash
        .clone();
    match verify_client(app.clone(), &peer, hash, input.password, true).await {
        Ok(true) => (),
        Ok(false) => return error(403, "password_invalid"),
        Err(e) => return *e,
    }
    let Some(catalog) = app.restore.checked_catalog(&input.token, &session.token) else {
        return error_coded(404, ErrorCode::WebNotFound, &[]);
    };
    // Detaching the coordinator keeps both transitions and blocking workers owned
    // even when the browser disconnects or middleware reaches its deadline.
    tokio::spawn(async move {
        let response = activate(app.clone(), session, catalog, WAIT).await;
        if app.geometry.barrier.snapshot().state == crate::barrier::State::Exclusive {
            if let Some(restart) = restart {
                restart.request();
            }
        }
        response
    })
    .await
    .unwrap_or_else(|_| error_coded(500, ErrorCode::RestoreFailed, &[]))
}
fn failure(code: ErrorCode) -> Response {
    error_coded(
        if code == ErrorCode::RestoreFailed {
            500
        } else {
            409
        },
        code,
        &[],
    )
}
fn barrier_failure(e: crate::barrier::BarrierError) -> Response {
    match e {
        crate::barrier::BarrierError::Timeout => {
            error_coded(503, ErrorCode::RestoreDrainTimeout, &[])
        }
        crate::barrier::BarrierError::Maintenance => crate::barrier::maintenance(),
    }
}
pub(super) async fn activate(
    app: Arc<App>,
    session: Session,
    catalog: Arc<restore_upload::CheckedCatalog>,
    wait: Duration,
) -> Response {
    let window = match app.geometry.barrier.enter_draining(wait).await {
        Ok(w) => w,
        Err(e) => return barrier_failure(e),
    };
    // Serialize password changes/logout with the authorization-to-commit interval.
    let changes = app.changes.clone().lock_owned().await;
    if !uploads::authenticated(&app, &session) {
        return error_coded(401, ErrorCode::WebLoginRequired, &[]);
    }
    let worker = app.clone();
    let prepared = tokio::task::spawn_blocking(move || prepare(&worker, &catalog)).await;
    let (files, mut record) = match prepared {
        Ok(Ok(v)) => v,
        Ok(Err(code)) => return failure(code),
        Err(_) => return failure(ErrorCode::RestoreFailed),
    };
    let window = match window.enter_exclusive(wait).await {
        Ok(w) => w,
        Err(e) => {
            let _ = restore_journal::remove_if_exists(
                &files.db,
                &format!(".restore-new-{}", record.id),
            );
            return barrier_failure(e);
        }
    };
    let worker = app.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _changes = changes;
        if !uploads::authenticated(&worker, &session) {
            let _ = restore_journal::remove_if_exists(
                &files.db,
                &format!(".restore-new-{}", record.id),
            );
            return Err(ErrorCode::WebLoginRequired);
        }
        // Once the durable intent exists, every failure requires startup recovery.
        // Never reopen admission onto an in-memory writer or a replaced catalog.
        window.complete();
        if files.save(&record).is_err() {
            return Err(ErrorCode::RestoreFailed);
        }
        let outcome = exchange(&worker, &files, &mut record, |_| Ok(()));
        let mut data = worker.data.lock().unwrap();
        for s in data.sessions.values() {
            s.validity.revoke();
        }
        data.sessions.clear();
        data.revision += 1;
        outcome
    })
    .await;
    match result {
        Ok(Err(ErrorCode::WebLoginRequired)) => error_coded(401, ErrorCode::WebLoginRequired, &[]),
        Ok(Ok(())) => Json(json!({"restored":true,"restart":true})).into_response(),
        _ => failure(ErrorCode::RestoreFailed),
    }
}
type Prepared = (Files, Record);
pub(super) fn prepare(
    app: &App,
    catalog: &restore_upload::CheckedCatalog,
) -> Result<Prepared, ErrorCode> {
    if let Some(ops) = &app.fileops {
        ops.recover_for_restore()
            .map_err(|_| ErrorCode::RestoreJournalPending)?;
    } else if let Some(ops) = crate::fileops_startup::prepare(
        &app.config,
        app.writer
            .as_ref()
            .ok_or(ErrorCode::RestoreFailed)?
            .conn
            .clone(),
        app.uploads.as_ref().map(|u| u.store.clone()),
    )
    .map_err(|_| ErrorCode::RestoreJournalPending)?
    {
        if !ops.journal.is_empty() {
            return Err(ErrorCode::RestoreJournalPending);
        }
    }
    let entries = if let Some(rt) = &app.uploads {
        rt.store
            .list()
            .map_err(|_| ErrorCode::RestoreUploadsPending)?
    } else {
        crate::upload_store::OfflineLedger::load(
            &restore_journal::parent(&app.config.auth_file)
                .canonicalize()
                .map_err(|_| ErrorCode::RestoreUploadsPending)?,
        )
        .map_err(|_| ErrorCode::RestoreUploadsPending)?
        .entries()
    };
    if entries
        .iter()
        .any(|e| e.state != crate::upload_store::UploadState::Imported || e.archive)
    {
        return Err(ErrorCode::RestoreUploadsPending);
    }
    let files = Files::open(&app.config).map_err(|_| ErrorCode::RestoreFailed)?;
    let random = crate::upload_store::random_id().map_err(|_| ErrorCode::RestoreFailed)?;
    let record = Record {
        id: format!("{:016x}{}", app.clock.now(), &random[..16]),
        phase: Phase::Prepared,
    };
    // Reject an aliased active name before creating a replacement intent.
    files
        .db
        .read(&files.db_name)
        .and_then(|f| f.sync_all())
        .map_err(|_| ErrorCode::RestoreFailed)?;
    match files.db.read(&format!(".bak-{}", record.id)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        _ => return Err(ErrorCode::RestoreFailed),
    }
    let new = format!(".restore-new-{}", record.id);
    let result = (|| {
        let conn = app
            .writer
            .as_ref()
            .ok_or(ErrorCode::RestoreFailed)?
            .conn
            .lock()
            .map_err(|_| ErrorCode::RestoreFailed)?;
        let local = katalog_core::db::thingiverse::local_settings(&conn)
            .map_err(|_| ErrorCode::RestoreFailed)?;
        let mut output = files
            .db
            .create(&new)
            .map_err(|_| ErrorCode::RestoreFailed)?;
        catalog
            .prepare_to(&app.config, &local, &mut output)
            .map_err(|_| ErrorCode::BackupInvalidDatabase)?;
        files.db.sync().map_err(|_| ErrorCode::RestoreFailed)?;
        let backup = format!("restore-{}.db", record.id);
        files
            .backups
            .create(&backup)
            .map_err(|_| ErrorCode::RestoreFailed)?;
        let path = restore_journal::parent(&app.config.db_path)
            .join("backups")
            .join(&backup);
        catalog_export::snapshot(&conn, &path).map_err(|_| ErrorCode::RestoreFailed)?;
        files
            .backups
            .read(&backup)
            .and_then(|f| f.sync_all())
            .map_err(|_| ErrorCode::RestoreFailed)?;
        let zip = files
            .backups
            .create(&format!("restore-{}.zip", record.id))
            .map_err(|_| ErrorCode::RestoreFailed)?;
        catalog_export::write_zip(&path, &catalog_export::settings_json(), zip)
            .and_then(|f| f.sync_all())
            .map_err(|_| ErrorCode::RestoreFailed)?;
        files.backups.sync().map_err(|_| ErrorCode::RestoreFailed)?;
        restore_upload::verify_automatic_backup(
            &app.config,
            files
                .backups
                .read(&format!("restore-{}.zip", record.id))
                .map_err(|_| ErrorCode::RestoreFailed)?,
        )?;
        Ok(())
    })();
    if let Err(e) = result {
        let _ = restore_journal::remove_if_exists(&files.db, &new);
        return Err(e);
    }
    Ok((files, record))
}
pub(super) fn exchange(
    app: &App,
    files: &Files,
    record: &mut Record,
    checkpoint: impl Fn(Phase) -> Result<(), ErrorCode>,
) -> Result<(), ErrorCode> {
    // Checkpoints follow durable decisions so tests can interrupt the real exchange.
    checkpoint(Phase::Prepared)?;
    fn fail<E>(_: E) -> ErrorCode {
        ErrorCode::RestoreFailed
    }
    let mut conn = app
        .writer
        .as_ref()
        .ok_or(ErrorCode::RestoreFailed)?
        .conn
        .lock()
        .map_err(fail)?;
    let replacement = rusqlite::Connection::open_in_memory().map_err(fail)?;
    // Rollback-journal mode is the server contract; reject unexpected sidecars.
    for suffix in ["-wal", "-shm", "-journal"] {
        match files.db.read(&format!("{}{suffix}", files.db_name)) {
            Ok(_) => return Err(ErrorCode::RestoreFailed),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err(ErrorCode::RestoreFailed),
        }
    }
    drop(std::mem::replace(&mut *conn, replacement));
    files
        .db
        .rename_noreplace(&files.db_name, &files.db, &format!(".bak-{}", record.id))
        .map_err(fail)?;
    files.db.sync().map_err(fail)?;
    record.phase = Phase::OldMoved;
    files.save(record).map_err(fail)?;
    checkpoint(Phase::OldMoved)?;
    files
        .db
        .rename_noreplace(
            &format!(".restore-new-{}", record.id),
            &files.db,
            &files.db_name,
        )
        .map_err(fail)?;
    files.db.sync().map_err(fail)?;
    record.phase = Phase::Installed;
    files.save(record).map_err(fail)?;
    checkpoint(Phase::Installed)?;
    files.reset_ledgers().map_err(fail)?;
    files.finish().map_err(fail)
}
