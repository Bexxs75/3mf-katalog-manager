//! Private backup previews and preparation of copies. Activation belongs to restore.
//! POST accepts a raw ZIP and an `x-restore-password` header (UTF-8 encoded as
//! base64). DELETE uses `/api/backup/staging/{token}` and the creating session.
use super::*;
use katalog_core::{
    catalog_restore::{self, RestoreFs, TemporaryCatalog, TrashPolicy},
    import::ImportError,
    upload_fs::Directory,
};
use std::{
    fs::File,
    io::Write,
    os::{fd::AsRawFd, unix::fs::MetadataExt},
    path::{Path, PathBuf},
    time::Duration,
};

pub(super) const UPLOAD: &str = "/api/backup/upload";
pub(super) const DELETE_PREFIX: &str = "/api/backup/staging/";
pub(super) const MAX_ZIP_BYTES: usize = 258 * 1024 * 1024;
const TTL: Duration = Duration::from_secs(30 * 60);
const ROOT: &str = "restore-staging";
const MAX_PENDING: usize = 4;

/// Keep this lease alive throughout preparation by the activation code.
/// The catalog is the original inspected input, not a migrated/activated database.
/// Activation must still prepare it under the catalog barrier and recheck authorization.
pub struct CheckedCatalog {
    parent: Directory,
    directory: Directory,
    path: PathBuf,
    token: String,
    identity: (u64, u64),
}
impl CheckedCatalog {
    pub fn path(&self) -> std::io::Result<PathBuf> {
        self.directory
            .checked("catalog.db", self.identity.0, self.identity.1)?;
        let current = Directory::open(self.path.parent().expect("staging parent"))?.metadata()?;
        let held = self.directory.metadata()?;
        if (current.dev(), current.ino()) != (held.dev(), held.ino()) {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        Ok(self.path.clone())
    }
    pub(super) fn prepare_to(
        &self,
        config: &Config,
        local: &katalog_core::db::thingiverse::LocalSettings,
        output: &mut File,
    ) -> Result<(), ImportError> {
        self.path()
            .map_err(|_| ImportError::coded(ErrorCode::BackupInvalidDatabase))?;
        use std::io::Read;
        let mut bytes = Vec::new();
        self.directory
            .checked("catalog.db", self.identity.0, self.identity.1)
            .and_then(|mut f| f.read_to_end(&mut bytes))
            .map_err(|_| ImportError::coded(ErrorCode::BackupInvalidDatabase))?;
        let parent = db_directory(&config.db_path);
        let trash = parent.join("trash");
        let known: Vec<String> = config
            .file_roots
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        let prepared = catalog_restore::prepare_catalog(
            &bytes,
            &[parent.to_path_buf(), config.auth_file.clone()],
            &TrashPolicy {
                desktop_trash: Some(&trash),
                catalog_roots: &config.file_roots,
            },
            &known,
            &InspectionFs(&self.directory),
            &[],
            local,
        );
        let result = prepared.and_then(|prepared| {
            drop(prepared);
            (|| -> std::io::Result<()> {
                std::io::copy(&mut self.directory.read("inspection.db")?, output)?;
                output.sync_all()
            })()
            .map_err(|_| ImportError::coded(ErrorCode::RestoreFailed))
        });
        for name in [
            "inspection.db",
            "inspection.db-journal",
            "inspection.db-wal",
            "inspection.db-shm",
        ] {
            let _ = self.directory.unlink(name);
        }
        result
    }
}
impl Drop for CheckedCatalog {
    fn drop(&mut self) {
        clear_directory(&self.directory);
        let _ = self.parent.rmdir(&self.token);
    }
}
struct Entry {
    owner: String,
    created: std::time::Instant,
    catalog: Arc<CheckedCatalog>,
}
/// One upload/inspection at a time, at most four outstanding previews per server.
pub struct Runtime {
    entries: Mutex<HashMap<String, Entry>>,
    slot: Arc<tokio::sync::Semaphore>,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            slot: Arc::new(tokio::sync::Semaphore::new(1)),
        }
    }
}
impl Runtime {
    fn sweep(&self) {
        self.entries
            .lock()
            .unwrap()
            .retain(|_, e| e.created.elapsed() < TTL);
    }
    /// Only tokens belonging to this authenticated session resolve. A lease
    /// prevents expiry or DELETE from removing files while the caller uses them.
    pub fn checked_catalog(&self, token: &str, session: &str) -> Option<Arc<CheckedCatalog>> {
        self.sweep();
        self.entries
            .lock()
            .unwrap()
            .get(token)
            .filter(|e| e.owner == session)
            .map(|e| e.catalog.clone())
    }
}
pub(super) fn start_cleanup(app: &Arc<App>) {
    let weak = Arc::downgrade(app);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let Some(app) = weak.upgrade() else { break };
            app.restore.sweep();
        }
    });
}
fn valid_token(token: &str) -> bool {
    token.len() == 64
        && token
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
fn clear_directory(dir: &Directory) {
    // Only our fixed files; never recurse into an unknown directory or follow a link.
    for name in [
        "backup.zip",
        "catalog.db",
        "inspection.db",
        "inspection.db-journal",
        "inspection.db-wal",
        "inspection.db-shm",
    ] {
        let _ = dir.unlink(name);
    }
}
fn db_directory(db: &Path) -> &Path {
    db.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}
/// Opens the shared staging folder next to the database. It holds one private
/// subfolder per catalog file (see `namespace`) plus, from older installations,
/// token folders directly inside it.
fn staging_base(db: &Path) -> std::io::Result<(Directory, PathBuf)> {
    let parent = db_directory(db);
    let parent = std::fs::canonicalize(parent)?;
    let handle = Directory::open(&parent)?;
    match handle.mkdir(ROOT, 0o700) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e),
    }
    let root = handle.open_dir(ROOT)?;
    if root.metadata()?.mode() & 0o777 != 0o700 {
        return Err(std::io::ErrorKind::PermissionDenied.into());
    }
    Ok((root, parent.join(ROOT)))
}
/// Catalogs that share a folder (`a.db`, `b.db`) also share the staging folder, but
/// each runs under its own instance lock. Startup cleanup must therefore stay inside
/// the folder of the catalog whose lock this process holds, or it would delete the
/// previews another running catalog still leases. The name derives from the same
/// canonical identity the instance lock uses; it is a fixed-width hash so any file
/// name fits and it can never look like a preview token.
fn namespace(db: &Path) -> std::io::Result<String> {
    let lock = katalog_core::db::instance_lock::InstanceLock::path_for(db)?;
    let name = lock.file_name().unwrap_or_default();
    // FNV-1a: stable across Rust versions, unlike the std hasher.
    let hash = std::os::unix::ffi::OsStrExt::as_bytes(name)
        .iter()
        .fold(0xcbf29ce484222325_u64, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
        });
    Ok(format!("catalog-{hash:016x}"))
}
fn staging_root(db: &Path) -> std::io::Result<(Directory, PathBuf)> {
    let (base, base_path) = staging_base(db)?;
    let name = namespace(db)?;
    match base.mkdir(&name, 0o700) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e),
    }
    let root = base.open_dir(&name)?;
    if root.metadata()?.mode() & 0o777 != 0o700 {
        return Err(std::io::ErrorKind::PermissionDenied.into());
    }
    Ok((root, base_path.join(name)))
}
pub(super) fn initialize(config: &Config) -> std::io::Result<()> {
    let path = db_directory(&config.db_path).join(ROOT);
    if !config.write {
        match std::fs::symlink_metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
            Ok(_) => (),
        }
    }
    startup(&config.db_path)
}
pub(super) fn startup(db: &Path) -> std::io::Result<()> {
    let (root, _) = staging_root(db)?;
    for name in root.names_checked()? {
        if valid_token(&name) {
            let dir = root.open_dir(&name)?;
            clear_directory(&dir);
            root.rmdir(&name)?;
        }
    }
    // Token folders directly in the shared folder come from installations that did not
    // separate catalogs, so their owner is unknown and another catalog may still be
    // running one. A preview never lives longer than its lifetime, so only folders well
    // past it are provably abandoned; younger ones are left for a later start.
    let (base, _) = staging_base(db)?;
    for name in base.names_checked()? {
        if !valid_token(&name) {
            continue;
        }
        let dir = base.open_dir(&name)?;
        let age = dir
            .metadata()?
            .modified()
            .ok()
            .and_then(|m| m.elapsed().ok())
            .unwrap_or_default();
        if age > TTL * 2 {
            clear_directory(&dir);
            base.rmdir(&name)?;
        }
    }
    Ok(())
}

struct InspectionFs<'a>(&'a Directory);
struct Temporary {
    conn: rusqlite::Connection,
    _file: File,
}
impl TemporaryCatalog for Temporary {
    fn connection(&mut self) -> &mut rusqlite::Connection {
        &mut self.conn
    }
}
impl RestoreFs for InspectionFs<'_> {
    fn resolve(&self, path: &Path) -> Result<PathBuf, ImportError> {
        katalog_core::import::placement::resolve_path_for_sensitivity_check(path)
    }
    fn temporary_catalog(&self, bytes: &[u8]) -> Result<Box<dyn TemporaryCatalog>, ImportError> {
        let fail = |_| ImportError::coded(ErrorCode::BackupInvalidDatabase);
        let mut file = self.0.create("inspection.db").map_err(fail)?;
        file.write_all(bytes).map_err(fail)?;
        drop(file);
        let file = self.0.read("inspection.db").map_err(fail)?;
        // SQLite opens the already selected inode, never a path from the backup.
        // Its sidecars remain inside the private staging directory.
        let conn = rusqlite::Connection::open(format!("/proc/self/fd/{}", file.as_raw_fd()))
            .map_err(|_| ImportError::coded(ErrorCode::BackupInvalidDatabase))?;
        Ok(Box::new(Temporary { conn, _file: file }))
    }
}
fn core_failure(e: ImportError) -> Response {
    if e.expected {
        (StatusCode::BAD_REQUEST, Json(e)).into_response()
    } else {
        error_coded(500, ErrorCode::WebServerInternal, &[])
    }
}
type StagedPreview = (Arc<CheckedCatalog>, catalog_restore::RestoreReport);
fn stage(config: &Config, zip: &[u8]) -> Result<StagedPreview, Box<Response>> {
    let io_error = |_| boxed_coded(500, ErrorCode::WebServerInternal, &[]);
    let (root, path) = staging_root(&config.db_path).map_err(io_error)?;
    let mut random = [0_u8; 32];
    getrandom::fill(&mut random)
        .map_err(|_| boxed_coded(500, ErrorCode::WebServerInternal, &[]))?;
    let token: String = random.iter().map(|b| format!("{b:02x}")).collect();
    root.mkdir(&token, 0o700).map_err(io_error)?;
    let directory = match root.open_dir(&token) {
        Ok(dir) => dir,
        Err(e) => {
            let _ = root.rmdir(&token);
            return Err(io_error(e));
        }
    };
    let mut staged = CheckedCatalog {
        parent: root,
        directory,
        path: path.join(&token).join("catalog.db"),
        token,
        identity: (0, 0),
    };
    staged
        .directory
        .create("backup.zip")
        .and_then(|mut f| f.write_all(zip))
        .map_err(io_error)?;
    let (bytes, _settings) = catalog_restore::read_catalog_archive_strict(
        staged.directory.read("backup.zip").map_err(io_error)?,
    )
    .map_err(|e| Box::new(core_failure(e)))?;
    // The web has no server-side UI preference store. Even known BACKUP_KEYS
    // are ignored; settings never influence paths, configuration or authorization.
    let mut db = staged.directory.create("catalog.db").map_err(io_error)?;
    db.write_all(&bytes).map_err(io_error)?;
    let meta = db.metadata().map_err(io_error)?;
    staged.identity = (meta.dev(), meta.ino());
    drop(db);
    let trash = db_directory(&config.db_path).join("trash");
    let sensitive = vec![
        db_directory(&config.db_path).to_path_buf(),
        config.auth_file.clone(),
    ];
    let report = catalog_restore::inspect_backup(
        &bytes,
        &sensitive,
        &TrashPolicy {
            desktop_trash: Some(&trash),
            catalog_roots: &config.file_roots,
        },
        &config.file_roots,
        &InspectionFs(&staged.directory),
        &|path| {
            // Check the configured boundary before any filesystem probe; open then
            // rejects symlink components, including links escaping a source root.
            config
                .source_roots
                .iter()
                .any(|root| path.starts_with(root))
                && Directory::open(path).is_ok()
        },
    )
    .map_err(|e| Box::new(core_failure(e)))?;
    for name in [
        "inspection.db",
        "inspection.db-journal",
        "inspection.db-wal",
        "inspection.db-shm",
    ] {
        match staged.directory.unlink(name) {
            Ok(()) => (),
            Err(e) if name != "inspection.db" && e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(io_error(e)),
        }
    }
    Ok((Arc::new(staged), report))
}

/// The automatic fallback must pass the same limits and policy as a user upload.
pub(super) fn verify_automatic_backup(config: &Config, file: File) -> Result<(), ErrorCode> {
    use std::io::Read;
    let mut bytes = Vec::new();
    file.take(MAX_ZIP_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ErrorCode::RestoreBackupInvalid)?;
    if bytes.len() > MAX_ZIP_BYTES {
        return Err(ErrorCode::RestoreBackupInvalid);
    }
    stage(config, &bytes)
        .map(|_| ())
        .map_err(|_| ErrorCode::RestoreBackupInvalid)
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
        return error(401, "session_required");
    };
    if req.method() == Method::DELETE && req.uri().path().starts_with(DELETE_PREFIX) {
        let token = req.uri().path().strip_prefix(DELETE_PREFIX).unwrap();
        if app.restore.checked_catalog(token, &session.token).is_none() {
            return error_coded(404, ErrorCode::WebNotFound, &[]);
        }
        app.restore.entries.lock().unwrap().remove(token);
        return StatusCode::NO_CONTENT.into_response();
    }
    if req.method() != Method::POST || req.uri().path() != UPLOAD {
        return error_coded(405, ErrorCode::WebMethodNotAllowed, &[]);
    }
    let password = req
        .headers()
        .get("x-restore-password")
        .and_then(|v| v.to_str().ok())
        .filter(|v| v.len() <= 2048)
        .and_then(|v| base64::engine::general_purpose::STANDARD.decode(v).ok())
        .and_then(|v| String::from_utf8(v).ok());
    let Some(password) = password else {
        return error(403, "password_invalid");
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
    match verify_client(app.clone(), &peer, hash, password, true).await {
        Ok(true) => (),
        Ok(false) => return error(403, "password_invalid"),
        Err(e) => return *e,
    }
    let permit = match app.restore.slot.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => return error_coded(429, ErrorCode::WebTooManyRequests, &[]),
    };
    app.restore.sweep();
    if app.restore.entries.lock().unwrap().len() >= MAX_PENDING {
        return error_coded(429, ErrorCode::WebTooManyRequests, &[]);
    }
    let bytes = match tokio::time::timeout(
        Duration::from_secs(120),
        axum::body::to_bytes(req.into_body(), MAX_ZIP_BYTES),
    )
    .await
    {
        Ok(Ok(b)) => b,
        Ok(Err(_)) => return error_coded(413, ErrorCode::WebBodyTooLarge, &[]),
        Err(_) => return error_coded(408, ErrorCode::WebRequestTimeout, &[]),
    };
    let catalog_guard = match app.geometry.barrier.reader() {
        Ok(guard) => guard,
        Err(_) => return crate::barrier::maintenance(),
    };
    // Work owns both admission and cleanup even if the HTTP request disappears.
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let _catalog_guard = catalog_guard;
        if !uploads::authenticated(&app, &session) {
            return error(401, "session_expired");
        }
        match stage(&app.config, &bytes) {
            Err(e) => *e,
            Ok((catalog, report)) => {
                if !uploads::authenticated(&app, &session) {
                    return error(401, "session_expired");
                }
                let token = catalog.token.clone();
                app.restore.entries.lock().unwrap().insert(
                    token.clone(),
                    Entry {
                        owner: session.token,
                        created: std::time::Instant::now(),
                        catalog,
                    },
                );
                Json(json!({"token":token,"report":report,"settings_ignored":true})).into_response()
            }
        }
    })
    .await
    .unwrap_or_else(|_| error_coded(500, ErrorCode::WebServerInternal, &[]))
}

#[cfg(test)]
#[path = "restore_upload_tests.rs"]
mod tests;
