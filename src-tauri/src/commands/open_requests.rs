//! Files handed over by the operating system ("Open with", a second start with a
//! file argument, the macOS `Opened` event).
//!
//! The paths wait in the backend (`PendingOpenFiles`) until the frontend collects them
//! with `take_open_requests`: a cold start delivers them before the window and its
//! frontend exist. Collecting turns each path into either a catalog hit (the file
//! is already cataloged, found by path or content hash) or an external source.
//! External files are remembered in `ExternalSources` under a random id, and later
//! commands only take that id, never a free path (the same proof pattern as
//! `DroppedImages`: a compromised frontend can't name arbitrary files).

use super::*;

use katalog_core::error_code::ErrorCode;
use katalog_core::import::files::{compute_content_hash, is_model_extension};

/// More files per hand-over are dropped (a selection of thousands must not flood the queue).
pub(crate) const MAX_OPEN_FILES: usize = 64;
/// External sources kept at once; a new one pushes out the oldest.
pub(crate) const MAX_EXTERNAL_SOURCES: usize = 16;
/// Frontend event: "there are paths to collect".
pub(crate) const OPEN_FILES_EVENT: &str = "open-files-requested";

/// Percent-decodes a URI path; `None` for broken escapes or non-UTF-8 bytes.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// `file:///a%20b/c.3mf` -> `/a b/c.3mf`; `file:///C:/x.stl` -> `C:/x.stl`. Other schemes: `None`.
fn path_from_file_uri(arg: &str) -> Option<PathBuf> {
    let rest = arg.get(..7).filter(|s| s.eq_ignore_ascii_case("file://")).map(|_| &arg[7..])?;
    // Empty host or "localhost"; any other host is a network share we don't open.
    let path = if rest.starts_with('/') {
        rest
    } else {
        rest.strip_prefix("localhost").filter(|p| p.starts_with('/'))?
    };
    let decoded = percent_decode(path)?;
    let bytes = decoded.as_bytes();
    // `/C:/dir` is how URIs spell a Windows drive path.
    if bytes.len() >= 3 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':' {
        return Some(PathBuf::from(&decoded[1..]));
    }
    Some(PathBuf::from(decoded))
}

/// Model files named in a command line. `args[0]` is the program and skipped. Options
/// (`-x`, `--x`, macOS `-psn_...`) are ignored until a lone `--`, after which every
/// argument counts as a path (so a file name may start with `-`). Relative paths resolve
/// against `cwd` (a second instance reports its own working directory). Unsupported
/// extensions, directories and missing files drop out; duplicates are removed.
pub(crate) fn open_requests_from_args(args: &[String], cwd: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut only_paths = false;
    for arg in args.iter().skip(1) {
        if !only_paths {
            if arg == "--" {
                only_paths = true;
                continue;
            }
            if arg.starts_with('-') {
                continue;
            }
        }
        if arg.is_empty() {
            continue;
        }
        let path = match path_from_file_uri(arg) {
            Some(path) => path,
            None if arg.contains("://") => continue,
            None => PathBuf::from(arg),
        };
        let path = if path.is_absolute() { path } else { cwd.join(path) };
        if !is_model_extension(&path) || !path.is_file() || paths.contains(&path) {
            continue;
        }
        paths.push(path);
        if paths.len() >= MAX_OPEN_FILES {
            break;
        }
    }
    paths
}

/// Paths waiting for the frontend.
#[derive(Default)]
pub struct PendingOpenFiles(Mutex<Vec<PathBuf>>);

impl PendingOpenFiles {
    /// Appends without duplicates; whatever exceeds [`MAX_OPEN_FILES`] is dropped.
    pub(crate) fn push(&self, paths: Vec<PathBuf>) {
        let mut queue = self.0.lock().unwrap_or_else(|e| e.into_inner());
        for path in paths {
            if queue.len() >= MAX_OPEN_FILES {
                break;
            }
            if !queue.contains(&path) {
                queue.push(path);
            }
        }
    }

    pub(crate) fn take(&self) -> Vec<PathBuf> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

/// Files that the user opened with the system but that are not in the catalog, by id.
/// Registering happens only while collecting (`take_open_requests`), so an id exists
/// only for a path the operating system really handed over.
#[derive(Default)]
pub struct ExternalSources(Mutex<Vec<(String, PathBuf)>>);

fn random_source_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    // RandomState is seeded randomly per instance, which is all the id needs.
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    format!("ext-{:016x}", hasher.finish())
}

impl ExternalSources {
    /// Returns the id of `path` (the same path keeps its id); at the limit the oldest source goes.
    pub(crate) fn register(&self, path: &Path) -> String {
        let mut sources = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((id, _)) = sources.iter().find(|(_, known)| known == path) {
            return id.clone();
        }
        if sources.len() >= MAX_EXTERNAL_SOURCES {
            sources.remove(0);
        }
        let mut id = random_source_id();
        while sources.iter().any(|(known, _)| *known == id) {
            id = random_source_id();
        }
        sources.push((id.clone(), path.to_path_buf()));
        id
    }

    /// The path behind an id; unknown ids (also forged ones) are an error.
    #[allow(dead_code)] // first caller: the view-only dialog (P5-4)
    pub(crate) fn resolve(&self, id: &str) -> CmdResult<PathBuf> {
        let sources = self.0.lock().unwrap_or_else(|e| e.into_inner());
        sources
            .iter()
            .find(|(known, _)| known == id)
            .map(|(_, path)| path.clone())
            .ok_or_else(|| CmdError::coded(ErrorCode::OpenSourceUnknown))
    }

    #[allow(dead_code)] // first caller: the view-only dialog (P5-4)
    pub(crate) fn release(&self, id: &str) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).retain(|(known, _)| known != id);
    }
}

/// One file the system asked us to open.
#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OpenRequest {
    /// Already in the catalog: show its detail page.
    #[serde(rename_all = "camelCase")]
    Catalog { file_id: String },
    /// Not cataloged: view only, `source_id` stands for the path from now on.
    #[serde(rename_all = "camelCase")]
    External { source_id: String, name: String, size_bytes: u64, extension: String },
}

/// Exactly one live catalog entry counts as a hit; two or more are ambiguous and the file
/// is treated as external rather than named wrongly.
fn single_hit(found: Vec<db::ExistingFileRef>) -> Option<i64> {
    match found.as_slice() {
        [one] => Some(one.id),
        _ => None,
    }
}

/// Catalog lookup by path (as given and resolved), then by content hash.
pub(crate) fn find_in_catalog(conn: &Connection, path: &Path, hash: impl FnOnce() -> Option<String>) -> Option<i64> {
    let mut spellings = vec![path.to_string_lossy().into_owned()];
    if let Ok(resolved) = std::fs::canonicalize(path) {
        let resolved = resolved.to_string_lossy().into_owned();
        if !spellings.contains(&resolved) {
            spellings.push(resolved);
        }
    }
    for spelling in &spellings {
        if let Some(id) = db::find_existing_by_path(conn, spelling).ok().and_then(single_hit) {
            return Some(id);
        }
    }
    db::find_existing_by_hash(conn, &hash()?).ok().and_then(single_hit)
}

/// Turns paths into requests; unreadable or vanished files drop out.
pub(crate) fn resolve_open_requests(conn: &Connection, sources: &ExternalSources, paths: Vec<PathBuf>) -> Vec<OpenRequest> {
    let mut requests = Vec::new();
    for path in paths {
        let Ok(meta) = std::fs::metadata(&path) else { continue };
        if !meta.is_file() || !is_model_extension(&path) {
            continue;
        }
        let hit = find_in_catalog(conn, &path, || compute_content_hash(&path).ok());
        requests.push(match hit {
            Some(id) => OpenRequest::Catalog { file_id: id.to_string() },
            None => OpenRequest::External {
                source_id: sources.register(&path),
                name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                size_bytes: meta.len(),
                extension: path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default(),
            },
        });
    }
    requests
}

/// Remembers paths from a second start or the macOS event and tells the frontend.
pub(crate) fn queue_and_notify<R: tauri::Runtime>(app: &tauri::AppHandle<R>, paths: Vec<PathBuf>) {
    use tauri::{Emitter, Manager};
    if paths.is_empty() {
        return;
    }
    if let Some(pending) = app.try_state::<PendingOpenFiles>() {
        pending.push(paths);
    }
    if let Err(error) = app.emit(OPEN_FILES_EVENT, ()) {
        log::warn!(target: "open_requests", "could not notify the frontend: {error}");
    }
}

/// Collects what the system handed over since the last call (cold start and events).
/// Hashing a large file can take a moment, so this runs off the main thread.
#[tauri::command]
pub async fn take_open_requests(app: tauri::AppHandle) -> CmdResult<Vec<OpenRequest>> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        let paths = app.state::<PendingOpenFiles>().take();
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        let state = app.state::<AppState>();
        let conn = lock_db(&state)?;
        Ok(resolve_open_requests(&conn, &app.state::<ExternalSources>(), paths))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
#[path = "open_requests_tests.rs"]
mod tests;
