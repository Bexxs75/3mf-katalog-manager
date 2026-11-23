//! Web source discovery. Stored absolute paths never cross the transport boundary.
use katalog_core::write::Connection;
use katalog_core::{
    db::{self, sources as store},
    error_code::ErrorCode,
    fs_portable::backend::{DirHandle, EntryKind, Name},
    import::{
        files,
        sources::{
            self, SourceEntries, SourceEntry, SourceFs, SourceKind, SourceMetadata, MAX_SCAN_DEPTH,
            MAX_SCAN_ENTRIES,
        },
        ImportError, ImportResult,
    },
    upload_fs::Directory,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io,
    path::{Component, Path, PathBuf},
};

pub const READ: &[&str] = &[
    "list_source_dirs",
    "list_source_folders",
    "scan_source_folders",
    "get_source_check_on_start",
];

fn denied() -> ImportError {
    ImportError::coded(ErrorCode::SourceRootNotAllowed)
}
fn invalid() -> ImportError {
    ImportError::coded(ErrorCode::ReqInvalidArguments)
}
fn io_denied() -> io::Error {
    io::ErrorKind::PermissionDenied.into()
}

/// Only slash-separated normal components; an empty path selects the root itself.
fn relative(rel: &str) -> io::Result<&Path> {
    let path = Path::new(rel);
    if rel.contains('\\')
        || rel.contains('\0')
        || rel.contains(':')
        || (!rel.is_empty()
            && rel
                .split('/')
                .any(|s| s.is_empty() || s == "." || s == ".."))
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(io_denied());
    }
    Ok(path)
}

pub struct SourceAdapter<'a> {
    pub roots: &'a [PathBuf],
}
impl SourceAdapter<'_> {
    fn locate<'a>(&self, path: &'a Path) -> io::Result<(usize, &'a Path)> {
        self.roots
            .iter()
            .enumerate()
            .find_map(|(i, root)| {
                let rel = path.strip_prefix(root).ok()?;
                relative(rel.to_str()?).ok()?;
                Some((i, rel))
            })
            .ok_or_else(io_denied)
    }
    fn requested(&self, idx: usize, rel: &str) -> ImportResult<PathBuf> {
        let root = self.roots.get(idx).ok_or_else(denied)?;
        let rel = relative(rel).map_err(|_| denied())?;
        Ok(root.join(rel))
    }
    fn directory(&self, path: &Path) -> io::Result<Directory> {
        let (idx, rel) = self.locate(path)?;
        // Reopen every component without following links, including a replaced root.
        let mut dir = Directory::open(&self.roots[idx])?;
        for component in rel.components() {
            let Component::Normal(name) = component else {
                return Err(io_denied());
            };
            dir = dir.open_dir(name.to_str().ok_or_else(io_denied)?)?;
        }
        Ok(dir)
    }
    fn token(&self, path: &Path) -> ImportResult<String> {
        let (idx, rel) = self.locate(path).map_err(|_| denied())?;
        Ok(format!("{idx}/{}", rel.to_str().ok_or_else(denied)?))
    }
    fn resolve_token(&self, token: &str) -> ImportResult<PathBuf> {
        let (idx, rel) = token.split_once('/').ok_or_else(denied)?;
        let idx: usize = idx.parse().map_err(|_| denied())?;
        if token != format!("{idx}/{rel}") {
            return Err(denied());
        }
        self.requested(idx, rel)
    }
    fn label(&self, path: &Path) -> String {
        match self.locate(path) {
            Ok((idx, rel)) => {
                let name = basename(&self.roots[idx]);
                if rel.as_os_str().is_empty() {
                    name
                } else {
                    format!("{name}/{}", rel.to_string_lossy())
                }
            }
            Err(_) => basename(path),
        }
    }
}
fn basename(path: &Path) -> String {
    path.to_string_lossy()
        .rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .unwrap_or_default()
        .to_owned()
}
fn name(path: &Path) -> io::Result<Name> {
    Name::new(
        path.file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(io_denied)?,
    )
    .map_err(|_| io_denied())
}
impl SourceFs for SourceAdapter<'_> {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        self.directory(path)?;
        Ok(path.to_owned())
    }
    fn stat(&self, path: &Path, _: bool) -> io::Result<SourceMetadata> {
        let (_, rel) = self.locate(path)?;
        if rel.as_os_str().is_empty() {
            let meta = self.directory(path)?.metadata()?;
            return Ok(SourceMetadata {
                kind: SourceKind::Directory,
                len: meta.len(),
            });
        }
        let parent = self.directory(path.parent().ok_or_else(io_denied)?)?;
        let meta = parent.entry(&name(path)?)?;
        let kind = match meta.kind {
            EntryKind::Dir => SourceKind::Directory,
            EntryKind::File => SourceKind::File,
            _ => SourceKind::Other,
        };
        Ok(SourceMetadata {
            kind,
            len: meta.len,
        })
    }
    fn list(&self, path: &Path) -> io::Result<SourceEntries<'_>> {
        let dir = self.directory(path)?;
        let names = dir.iter_names()?;
        let path = path.to_owned();
        Ok(Box::new(names.map(move |entry| {
            let child = path.join(entry?);
            let kind = name(&child)
                .and_then(|n| dir.entry(&n))
                .map(|meta| match meta.kind {
                    EntryKind::Dir => SourceKind::Directory,
                    EntryKind::File => SourceKind::File,
                    _ => SourceKind::Other,
                });
            Ok(SourceEntry { path: child, kind })
        })))
    }
    fn content_hash(&self, path: &Path) -> ImportResult<String> {
        self.locate(path).map_err(|_| denied())?;
        let parent = self
            .directory(path.parent().ok_or_else(denied)?)
            .map_err(|_| denied())?;
        let file = parent
            .read(
                path.file_name()
                    .and_then(|n| n.to_str())
                    .ok_or_else(denied)?,
            )
            .map_err(|_| denied())?;
        let size = file.metadata().map_err(|_| denied())?.len();
        if size > katalog_core::safe_file::MAX_MODEL_FILE_BYTES {
            return Err(denied());
        }
        // A growing file must not turn a bounded scan into an unbounded hash read.
        use std::io::Read;
        let mut bounded = file.take(size + 1);
        let hash = files::compute_reader_hash(&mut bounded)?;
        if bounded.limit() == 0 || bounded.get_ref().metadata().map_err(|_| denied())?.len() != size
        {
            return Err(denied());
        }
        Ok(hash)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Location {
    root_idx: usize,
    rel: String,
}
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Browse {
    root_idx: Option<usize>,
    #[serde(default)]
    rel: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    rel: String,
    name: String,
    directory: bool,
    file_count: usize,
    truncated: bool,
}

/// One budget covers both immediate children and their recursive counts.
fn browse(
    fs: &SourceAdapter<'_>,
    args: Browse,
    depth_limit: usize,
    entry_limit: usize,
) -> ImportResult<Value> {
    let roots: Vec<_> = fs
        .roots
        .iter()
        .enumerate()
        .map(|(i, p)| json!({"rootIdx": i, "name": basename(p)}))
        .collect();
    let Some(idx) = args.root_idx else {
        if !args.rel.is_empty() {
            return Err(invalid());
        }
        return Ok(json!({"roots": roots, "entries": [], "truncated": false}));
    };
    let root = fs.requested(idx, &args.rel)?;
    let mut pending = vec![(root.clone(), 0, None::<usize>)];
    let mut entries: Vec<Entry> = Vec::new();
    let mut visited = 0;
    let mut truncated = false;
    while let Some((dir, depth, owner)) = pending.pop() {
        let children = fs.list(&dir).map_err(|_| denied())?;
        for child in children {
            if visited == entry_limit {
                truncated = true;
                break;
            }
            visited += 1;
            let Ok(child) = child else {
                truncated = true;
                continue;
            };
            let Ok(kind) = child.kind else {
                truncated = true;
                continue;
            };
            let is_dir = kind == SourceKind::Directory;
            let supported = kind == SourceKind::File
                && files::is_model_extension(&child.path)
                && !files::is_ignored_import_path(&child.path);
            if !is_dir && !supported {
                continue;
            }
            let owner = if depth == 0 {
                entries.push(Entry {
                    rel: child
                        .path
                        .strip_prefix(&fs.roots[idx])
                        .map_err(|_| denied())?
                        .to_string_lossy()
                        .into_owned(),
                    name: basename(&child.path),
                    directory: is_dir,
                    file_count: usize::from(supported),
                    truncated: false,
                });
                Some(entries.len() - 1)
            } else {
                if supported {
                    if let Some(i) = owner {
                        entries[i].file_count += 1;
                    }
                }
                owner
            };
            if is_dir {
                if depth < depth_limit {
                    pending.push((child.path, depth + 1, owner));
                } else {
                    truncated = true;
                    if let Some(i) = owner {
                        entries[i].truncated = true;
                    }
                }
            }
        }
        if visited == entry_limit && (!pending.is_empty() || truncated) {
            truncated = true;
            break;
        }
    }
    // A shared exhausted budget makes every unfinished count a lower bound.
    if truncated {
        for entry in &mut entries {
            if entry.directory {
                entry.truncated = true;
            }
        }
    }
    entries.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(
        json!({"roots": roots, "rootIdx": idx, "rel": args.rel, "entries": entries, "truncated": truncated}),
    )
}
fn empty(args: &Value) -> ImportResult<()> {
    if args.as_object().is_some_and(|a| a.is_empty()) {
        Ok(())
    } else {
        Err(invalid())
    }
}
fn folder_value(fs: &SourceAdapter<'_>, mut folder: sources::SourceFolderDto) -> Value {
    let location = fs
        .locate(Path::new(&folder.path))
        .ok()
        .map(|(idx, rel)| (idx, rel.to_string_lossy().into_owned()));
    folder.path = fs.label(Path::new(&folder.path));
    let mut value = json!(folder);
    value["rootIdx"] = json!(location.as_ref().map(|(idx, _)| idx));
    value["rel"] = json!(location.map(|(_, rel)| rel));
    value
}
pub fn read(
    conn: &Connection,
    roots: &[PathBuf],
    command: &str,
    args: &Value,
) -> ImportResult<Value> {
    let fs = SourceAdapter { roots };
    if command == "list_source_dirs" {
        return browse(
            &fs,
            serde_json::from_value(args.clone()).map_err(|_| invalid())?,
            MAX_SCAN_DEPTH,
            MAX_SCAN_ENTRIES,
        );
    }
    empty(args)?;
    match command {
        "list_source_folders" => Ok(Value::Array(
            store::list_source_folders(conn)?
                .into_iter()
                .map(|r| folder_value(&fs, sources::source_dto(&fs, r)))
                .collect(),
        )),
        "get_source_check_on_start" => Ok(json!(store::source_check_on_start(conn)?)),
        "scan_source_folders" => {
            let mut scans = sources::scan_with_conn(&fs, conn, &[], MAX_SCAN_ENTRIES)?;
            for scan in &mut scans {
                scan.folder_path = fs.label(Path::new(&scan.folder_path));
                for file in &mut scan.files {
                    file.path = fs.token(Path::new(&file.path))?;
                }
            }
            Ok(json!(scans))
        }
        _ => Err(invalid()),
    }
}

pub fn write(
    conn: &Connection,
    roots: &[PathBuf],
    command: &str,
    args: &Value,
) -> ImportResult<(Value, u64)> {
    let fs = SourceAdapter { roots };
    match command {
        "add_source_folder" => {
            let args: Location = serde_json::from_value(args.clone()).map_err(|_| invalid())?;
            let path = fs.requested(args.root_idx, &args.rel)?;
            fs.directory(&path).map_err(|_| denied())?;
            Ok((
                folder_value(
                    &fs,
                    sources::add_source_folder_with_conn(&fs, conn, &path, &[])?,
                ),
                1,
            ))
        }
        "remove_source_folder" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Args {
                id: String,
            }
            let args: Args = serde_json::from_value(args.clone()).map_err(|_| invalid())?;
            let id = args
                .id
                .parse::<i64>()
                .ok()
                .filter(|id| *id > 0)
                .ok_or_else(invalid)?;
            sources::remove_source_folder_with_conn(conn, id)?;
            Ok((Value::Null, 1))
        }
        "reject_source_files" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Args {
                paths: Vec<String>,
            }
            let args: Args = serde_json::from_value(args.clone()).map_err(|_| invalid())?;
            if args.paths.len() > MAX_SCAN_ENTRIES {
                return Err(invalid());
            }
            let paths = args
                .paths
                .iter()
                .map(|p| fs.resolve_token(p).map(|p| p.to_string_lossy().into_owned()))
                .collect::<ImportResult<Vec<_>>>()?;
            let n = sources::reject_with_conn(&fs, conn, &[], &paths, MAX_SCAN_ENTRIES)?;
            Ok((json!(n), n as u64))
        }
        "reset_rejected_source_files" => {
            empty(args)?;
            let n = store::clear_rejected_sources(conn)?;
            Ok((json!(n), n as u64))
        }
        "set_source_check_on_start" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Args {
                enabled: bool,
            }
            let args: Args = serde_json::from_value(args.clone()).map_err(|_| invalid())?;
            store::set_source_check_on_start(conn, args.enabled)?;
            Ok((Value::Null, 1))
        }
        _ => Err(invalid()),
    }
}

pub fn response_error(error: ImportError) -> axum::response::Response {
    if error.expected {
        if let Some(code) = ErrorCode::ALL.iter().find(|c| c.as_str() == error.message) {
            return crate::web::error_coded(422, *code, &[]);
        }
    }
    crate::web::error_coded(500, ErrorCode::WebServerInternal, &[])
}

/// Reads use the existing bounded geometry worker pool and catalog barrier.
pub async fn invoke(
    db_path: PathBuf,
    roots: Vec<PathBuf>,
    command: String,
    args: Value,
    gate: std::sync::Arc<crate::invoke::GeometryGate>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let guard = match gate.barrier.reader() {
        Ok(g) => g,
        Err(_) => return crate::barrier::maintenance(),
    };
    let permits = match gate.acquire().await {
        Ok(p) => p,
        Err(r) => return *r,
    };
    tokio::task::spawn_blocking(move || {
        let (_guard, _permits) = (guard, permits);
        let result = db::open_read_only(&db_path)
            .map_err(ImportError::from)
            .and_then(|conn| read(&conn, &roots, &command, &args));
        match result {
            Ok(value) => axum::Json(value).into_response(),
            Err(error) => response_error(error),
        }
    })
    .await
    .unwrap_or_else(|_| crate::web::error_coded(500, ErrorCode::WebServerInternal, &[]))
}

#[cfg(test)]
#[path = "source_dirs_tests.rs"]
mod tests;
