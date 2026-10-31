//! Reading a stored companion file safely. The desktop commands and the web server
//! share these checks, so both agree on which file a row may open: the row must
//! describe an allowed companion, belong to the genuine `_extras` folder of its
//! model, and the file must be a regular file whose content matches its type.
//! A caller never passes a path: everything comes from the database row.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::Serialize;

use crate::companions::{
    classify, extras_dir_for, lowercase_extension, validate_stored_fields, CompanionKind, Refusal,
    CONTENT_CHECK_BYTES, MAX_COMPANIONS_PER_MODEL,
};
use crate::db;
use crate::db::models::CompanionRecord;
use crate::safe_file;

/// Largest image sent to the preview; bigger pictures are opened or downloaded instead.
pub const MAX_PREVIEW_BYTES: u64 = 10 * 1024 * 1024;

/// Windows and macOS volumes are usually case-insensitive.
pub const CASE_INSENSITIVE_PATHS: bool = cfg!(any(windows, target_os = "macos"));

/// Why the `_extras` folder of a model cannot be trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderProblem {
    Unverifiable,
    NotReal,
    Elsewhere,
    ModelPathInvalid,
}

/// Why a stored companion cannot be opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowError {
    /// The row is malformed (wrong type, name or location).
    Invalid,
    /// The row sits in the trash.
    Trashed,
    ModelMissing,
    /// The model is in the trash; its files are not served.
    ModelTrashed,
    ModelPathInvalid,
    /// The file is not in the `_extras` folder of its model.
    NotInModelFolder,
    Folder(FolderProblem),
    /// Gone, a link, a hard link with several names, or not a regular file.
    NotAFile,
    Unreadable,
    TooLarge,
    Content(Refusal),
}

pub fn image_mime(name: &str) -> &'static str {
    match lowercase_extension(name).as_deref() {
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "image/jpeg",
    }
}

/// Reads at most the first [`CONTENT_CHECK_BYTES`] of an open file.
pub fn read_head(file: &mut File) -> std::io::Result<Vec<u8>> {
    let mut head = Vec::new();
    Read::by_ref(file).take(CONTENT_CHECK_BYTES as u64).read_to_end(&mut head)?;
    Ok(head)
}

/// Whether two folder paths name the same folder, by the same rule everywhere.
pub fn same_dir_with(a: &Path, b: &Path, case_insensitive: bool) -> bool {
    let (a, b) = (crate::import::placement::comparison_path(a), crate::import::placement::comparison_path(b));
    if case_insensitive {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

pub fn same_dir(a: &Path, b: &Path) -> bool {
    same_dir_with(a, b, CASE_INSENSITIVE_PATHS)
}

/// Why `dir` cannot be trusted as the companion folder `expected` (the `_extras` folder
/// next to a model). A link in place of the folder, or a model folder reached through a
/// different route, would make a file operation land on someone else's file. A folder
/// that does not exist is fine: there is nothing to touch.
pub fn extras_dir_problem(dir: &Path, expected: &Path) -> Option<FolderProblem> {
    let meta = match std::fs::symlink_metadata(dir) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(_) => return Some(FolderProblem::Unverifiable),
    };
    if !meta.is_dir() {
        return Some(FolderProblem::NotReal);
    }
    let (Some(model_dir), Some(name)) = (expected.parent(), expected.file_name()) else {
        return Some(FolderProblem::ModelPathInvalid);
    };
    let (Ok(resolved_dir), Ok(resolved_model_dir)) = (dir.canonicalize(), model_dir.canonicalize()) else {
        return Some(FolderProblem::Unverifiable);
    };
    if !same_dir(&resolved_dir, &resolved_model_dir.join(name)) {
        return Some(FolderProblem::Elsewhere);
    }
    None
}

/// Structure of the row and not in the trash.
pub fn check_row_state(row: &CompanionRecord) -> Result<CompanionKind, RowError> {
    let kind = validate_stored_fields(&row.name, &row.path, &row.kind).map_err(|_| RowError::Invalid)?;
    if row.deleted_at.is_some() {
        return Err(RowError::Trashed);
    }
    Ok(kind)
}

/// The folder of a stored companion must be exactly the genuine `_extras` folder of the
/// model the row belongs to, before any file in it is opened or read. Returns the model's
/// trash state so a web caller can refuse trashed models.
pub fn check_row_folder(conn: &Connection, row: &CompanionRecord) -> Result<bool, RowError> {
    let model = db::get_file(conn, row.file_id).map_err(|_| RowError::ModelMissing)?.ok_or(RowError::ModelMissing)?;
    let expected = extras_dir_for(Path::new(&model.path)).ok_or(RowError::ModelPathInvalid)?;
    let dir = Path::new(&row.path).parent().ok_or(RowError::Invalid)?;
    if !same_dir(dir, &expected) {
        return Err(RowError::NotInModelFolder);
    }
    match extras_dir_problem(dir, &expected) {
        Some(problem) => Err(RowError::Folder(problem)),
        None => Ok(model.deleted_at.is_some()),
    }
}

/// Opens the file of a checked row and reads its head. `strict` (the web) opens the
/// folder first (`O_DIRECTORY|O_NOFOLLOW`, same inode as the canonical folder that was
/// checked) and the file relative to it with `O_NOFOLLOW`, and refuses a file with several
/// hard links; `roots` is then checked against where the opened file really is. This
/// narrows, but does not close, a race with someone who can rewrite the catalog tree:
/// the roots must not be writable by untrusted users. Without `strict`, a symlink is
/// refused by a prior metadata check only. The returned handle is the one that was checked.
pub fn open_row_file(
    row: &CompanionRecord,
    kind: CompanionKind,
    max_bytes: u64,
    strict: bool,
    roots: Option<&[PathBuf]>,
) -> Result<(File, Vec<u8>), RowError> {
    let path = Path::new(&row.path);
    if !std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file()) {
        return Err(RowError::NotAFile);
    }
    let mut file = if strict { open_strict(path, roots, || {})? } else { safe_file::open_regular(path).map_err(|_| RowError::Unreadable)? };
    let meta = file.metadata().map_err(|_| RowError::Unreadable)?;
    if !meta.is_file() {
        return Err(RowError::NotAFile);
    }
    #[cfg(unix)]
    if strict {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() != 1 {
            return Err(RowError::NotAFile);
        }
    }
    if meta.len() > max_bytes {
        return Err(RowError::TooLarge);
    }
    let head = read_head(&mut file).map_err(|_| RowError::Unreadable)?;
    let checked = classify(&row.name, &head).map_err(RowError::Content)?;
    if checked != kind {
        return Err(RowError::Content(Refusal::Content));
    }
    Ok((file, head))
}

/// Whether `dir` (resolved) lies below one of the configured roots.
pub fn dir_in_roots(dir: &Path, roots: &[PathBuf]) -> bool {
    let Ok(canonical) = dir.canonicalize() else { return false };
    roots.iter().any(|root| root.canonicalize().is_ok_and(|root| canonical.starts_with(root)))
}

#[cfg(unix)]
fn open_strict(path: &Path, roots: Option<&[PathBuf]>, between: impl FnOnce()) -> Result<File, RowError> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::MetadataExt;
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else { return Err(RowError::NotAFile) };
    let canonical = dir.canonicalize().map_err(|_| RowError::NotInModelFolder)?;
    if roots.is_some_and(|r| !dir_in_roots(&canonical, r)) {
        return Err(RowError::NotInModelFolder);
    }
    let checked = std::fs::symlink_metadata(&canonical).map_err(|_| RowError::Unreadable)?;
    if !checked.is_dir() {
        return Err(RowError::Folder(FolderProblem::NotReal));
    }
    between();
    let c_dir = CString::new(canonical.as_os_str().as_bytes()).map_err(|_| RowError::Invalid)?;
    let c_name = CString::new(name.as_bytes()).map_err(|_| RowError::Invalid)?;
    // SAFETY: valid NUL-terminated strings; the descriptors are wrapped at once.
    let dir_fd = unsafe { libc::open(c_dir.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC) };
    if dir_fd < 0 {
        return Err(RowError::Folder(FolderProblem::NotReal));
    }
    let dir_fd = unsafe { OwnedFd::from_raw_fd(dir_fd) };
    let opened = File::from(dir_fd.try_clone().map_err(|_| RowError::Unreadable)?).metadata().map_err(|_| RowError::Unreadable)?;
    if opened.dev() != checked.dev() || opened.ino() != checked.ino() {
        return Err(RowError::Folder(FolderProblem::Elsewhere));
    }
    let fd = unsafe {
        libc::openat(dir_fd.as_raw_fd(), c_name.as_ptr(), libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_NOCTTY | libc::O_CLOEXEC)
    };
    if fd < 0 {
        let error = std::io::Error::last_os_error();
        return Err(if error.raw_os_error() == Some(libc::ELOOP) { RowError::NotAFile } else { RowError::Unreadable });
    }
    let file = unsafe { File::from_raw_fd(fd) };
    // Where the opened file really is, where the system can tell (Linux).
    #[cfg(target_os = "linux")]
    if let (Some(roots), Ok(real)) = (roots, std::fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd()))) {
        if !roots.iter().any(|root| root.canonicalize().is_ok_and(|root| real.starts_with(root))) {
            return Err(RowError::NotInModelFolder);
        }
    }
    Ok(file)
}

#[cfg(not(unix))]
fn open_strict(path: &Path, roots: Option<&[PathBuf]>, between: impl FnOnce()) -> Result<File, RowError> {
    if roots.is_some_and(|r| !path.parent().is_some_and(|d| dir_in_roots(d, r))) {
        return Err(RowError::NotInModelFolder);
    }
    between();
    safe_file::open_regular(path).map_err(|_| RowError::Unreadable)
}

/// Everything the web needs before it serves a companion: the row exists and belongs to
/// a live model, the folder is genuine and inside a configured root, and the file passes
/// the strict open. Returns the row, its kind and the open handle.
pub fn open_for_web(
    conn: &Connection,
    companion_id: i64,
    roots: &[PathBuf],
    max_bytes: u64,
) -> Result<(CompanionRecord, CompanionKind, File, Vec<u8>), RowError> {
    let row = db::companions::get_companion(conn, companion_id).map_err(|_| RowError::Invalid)?.ok_or(RowError::Invalid)?;
    let kind = check_row_state(&row)?;
    if check_row_folder(conn, &row)? {
        return Err(RowError::ModelTrashed);
    }
    let (file, head) = open_row_file(&row, kind, max_bytes, true, Some(roots))?;
    Ok((row, kind, file, head))
}

/// What the web shows for one companion: no server path.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebCompanion {
    pub id: String,
    pub file_id: String,
    pub name: String,
    pub kind: String,
    pub size_bytes: Option<i64>,
    pub added_at: String,
    /// The file is gone from disk or fails the safety checks; it is listed but not served.
    pub missing: bool,
    /// An image small enough for `companion_image_data`.
    pub previewable: bool,
    /// Set for a file that was removed on its own and still waits in the trash.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
    /// Only for removed files: the web can bring it back (the desktop's trash is its own).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restorable: Option<bool>,
}

/// Active companions of a live model, then the ones removed on their own. A trashed
/// model lists none.
pub fn list_for_web(conn: &Connection, file_id: i64, roots: &[PathBuf]) -> Result<Vec<WebCompanion>, RowError> {
    list_for_web_with(conn, file_id, roots, false)
}

/// `include_removed` appends the files that were removed on their own, for the file-management UI.
pub fn list_for_web_with(conn: &Connection, file_id: i64, roots: &[PathBuf], include_removed: bool) -> Result<Vec<WebCompanion>, RowError> {
    let model = db::get_file(conn, file_id).map_err(|_| RowError::ModelMissing)?.ok_or(RowError::ModelMissing)?;
    if model.deleted_at.is_some() {
        return Ok(Vec::new());
    }
    let rows = db::companions::list_active_companions_limited(conn, file_id, MAX_COMPANIONS_PER_MODEL).map_err(|_| RowError::ModelMissing)?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let Ok(kind) = check_row_state(&row) else { continue };
        let genuine = check_row_folder(conn, &row).is_ok() && Path::new(&row.path).parent().is_some_and(|d| dir_in_roots(d, roots));
        let present = genuine && std::fs::symlink_metadata(&row.path).is_ok_and(|m| m.is_file());
        let size = row.size_bytes.filter(|s| *s >= 0);
        out.push(WebCompanion {
            id: row.id.to_string(),
            file_id: row.file_id.to_string(),
            kind: kind.as_str().to_owned(),
            previewable: present && kind == CompanionKind::Image && size.is_some_and(|s| s as u64 <= MAX_PREVIEW_BYTES),
            missing: !present,
            size_bytes: row.size_bytes,
            added_at: row.added_at,
            name: row.name,
            deleted_at: None,
            restorable: None,
        });
    }
    if !include_removed {
        return Ok(out);
    }
    let removed = db::companions::list_trashed_files_of_model(conn, file_id).map_err(|_| RowError::ModelMissing)?;
    for row in removed.into_iter().take(MAX_COMPANIONS_PER_MODEL as usize) {
        let Ok(kind) = validate_stored_fields(&row.name, &row.path, &row.kind) else { continue };
        // Restoring from the web trash exists on Linux only (the web server's platform); elsewhere the desktop restores.
        #[cfg(target_os = "linux")]
        let restorable = crate::fsops::web_restorable(roots, &model.path, row.id, row.trash_path.as_deref());
        #[cfg(not(target_os = "linux"))]
        let restorable = false;
        out.push(WebCompanion {
            id: row.id.to_string(),
            file_id: row.file_id.to_string(),
            kind: kind.as_str().to_owned(),
            previewable: false,
            missing: false,
            size_bytes: row.size_bytes,
            added_at: row.added_at,
            name: row.name,
            deleted_at: row.deleted_at,
            restorable: Some(restorable),
        });
    }
    Ok(out)
}

/// The picture as a data URI for the preview: images only, capped at [`MAX_PREVIEW_BYTES`].
pub fn image_data_uri(file: &mut File, head: Vec<u8>, name: &str) -> Result<String, RowError> {
    use base64::Engine;
    let mut bytes = head;
    Read::by_ref(file)
        .take(MAX_PREVIEW_BYTES + 1 - (bytes.len() as u64).min(MAX_PREVIEW_BYTES))
        .read_to_end(&mut bytes)
        .map_err(|_| RowError::Unreadable)?;
    if bytes.len() as u64 > MAX_PREVIEW_BYTES {
        return Err(RowError::TooLarge);
    }
    Ok(format!("data:{};base64,{}", image_mime(name), base64::engine::general_purpose::STANDARD.encode(bytes)))
}

/// Sniffs nothing: the download type comes from the stored kind, never from the file.
/// Images keep their image type, everything else (PDF, text) is served as opaque bytes.
pub fn download_content_type(kind: CompanionKind, name: &str) -> &'static str {
    match kind {
        CompanionKind::Image => image_mime(name),
        CompanionKind::Pdf | CompanionKind::Text => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::companions::kind_for_extension;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nrest";

    struct Fx {
        dir: PathBuf,
        conn: Connection,
        model: i64,
        extras: PathBuf,
    }
    impl Drop for Fx {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
    fn fx() -> Fx {
        let dir = std::env::temp_dir().join(format!("companion-read-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&dir).unwrap();
        let dir = dir.canonicalize().unwrap();
        let conn = db::connect(&dir.join("catalog.db")).unwrap();
        let model_path = dir.join("Foo.3mf");
        std::fs::write(&model_path, b"x").unwrap();
        let model = db::test_insert_minimal_file(&conn, model_path.to_str().unwrap(), None).unwrap();
        let extras = dir.join("Foo_extras");
        std::fs::create_dir(&extras).unwrap();
        Fx { dir, conn, model, extras }
    }
    impl Fx {
        fn add(&self, name: &str, body: &[u8]) -> i64 {
            let path = self.extras.join(name);
            std::fs::write(&path, body).unwrap();
            let kind = kind_for_extension(&lowercase_extension(name).unwrap()).unwrap();
            db::companions::insert_companion(&self.conn, self.model, name, path.to_str().unwrap(), kind.as_str(), Some(body.len() as i64), "now").unwrap()
        }
        fn roots(&self) -> Vec<PathBuf> {
            vec![self.dir.clone()]
        }
        fn open(&self, id: i64) -> Result<(CompanionRecord, CompanionKind, File, Vec<u8>), RowError> {
            open_for_web(&self.conn, id, &self.roots(), 1 << 20)
        }
    }

    #[test]
    fn a_genuine_companion_opens() {
        let f = fx();
        let id = f.add("pic.png", PNG);
        let (row, kind, _, head) = f.open(id).unwrap();
        assert_eq!((row.name.as_str(), kind), ("pic.png", CompanionKind::Image));
        assert_eq!(head, PNG);
    }

    #[test]
    fn unknown_trashed_and_foreign_rows_are_refused() {
        let f = fx();
        assert_eq!(f.open(999).err(), Some(RowError::Invalid));
        let id = f.add("pic.png", PNG);
        f.conn.execute("UPDATE companion_files SET deleted_at='x' WHERE id=?1", [id]).unwrap();
        assert_eq!(f.open(id).err(), Some(RowError::Trashed));
        let other = f.add("b.png", PNG);
        assert_eq!(open_for_web(&f.conn, other, &[PathBuf::from("/nonexistent-root")], 1 << 20).err(), Some(RowError::NotInModelFolder));
    }

    #[test]
    fn a_row_outside_the_model_folder_is_refused() {
        let f = fx();
        let other = f.dir.join("Bar_extras");
        std::fs::create_dir(&other).unwrap();
        std::fs::write(other.join("s.png"), PNG).unwrap();
        let id = db::companions::insert_companion(&f.conn, f.model, "s.png", other.join("s.png").to_str().unwrap(), "image", None, "now").unwrap();
        assert_eq!(f.open(id).err(), Some(RowError::NotInModelFolder));
    }

    #[test]
    fn traversal_in_the_stored_path_is_refused() {
        let f = fx();
        std::fs::write(f.dir.join("s.png"), PNG).unwrap();
        let path = format!("{}/../s.png", f.extras.display());
        let id = db::companions::insert_companion(&f.conn, f.model, "s.png", &path, "image", None, "now").unwrap();
        assert_eq!(f.open(id).err(), Some(RowError::Invalid));
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_file_or_folder_is_refused() {
        let f = fx();
        let id = f.add("pic.png", PNG);
        let real = f.dir.join("real.png");
        std::fs::write(&real, PNG).unwrap();
        std::fs::remove_file(f.extras.join("pic.png")).unwrap();
        std::os::unix::fs::symlink(&real, f.extras.join("pic.png")).unwrap();
        assert_eq!(f.open(id).err(), Some(RowError::NotAFile));
        // folder replaced by a link to a look-alike
        let g = fx();
        let id = g.add("pic.png", PNG);
        let moved = g.dir.join("elsewhere");
        std::fs::rename(&g.extras, &moved).unwrap();
        std::os::unix::fs::symlink(&moved, &g.extras).unwrap();
        assert_eq!(g.open(id).err(), Some(RowError::Folder(FolderProblem::NotReal)));
    }

    #[cfg(unix)]
    #[test]
    fn a_hard_linked_file_is_refused() {
        let f = fx();
        let id = f.add("pic.png", PNG);
        std::fs::hard_link(f.extras.join("pic.png"), f.dir.join("alias.png")).unwrap();
        assert_eq!(f.open(id).err(), Some(RowError::NotAFile));
    }

    #[test]
    fn content_must_match_and_size_is_capped() {
        let f = fx();
        let id = f.add("pic.png", PNG);
        std::fs::write(f.extras.join("pic.png"), b"MZ not a picture").unwrap();
        assert_eq!(f.open(id).err(), Some(RowError::Content(Refusal::Content)));
        std::fs::write(f.extras.join("pic.png"), [PNG, &[0u8; 100][..]].concat()).unwrap();
        assert_eq!(open_for_web(&f.conn, id, &f.roots(), 50).err(), Some(RowError::TooLarge));
    }

    #[test]
    fn a_trashed_model_serves_nothing_and_lists_nothing() {
        let f = fx();
        let id = f.add("pic.png", PNG);
        f.conn.execute("UPDATE files SET deleted_at='x'", []).unwrap();
        assert_eq!(f.open(id).err(), Some(RowError::ModelTrashed));
        assert!(list_for_web(&f.conn, f.model, &f.roots()).unwrap().is_empty());
    }

    #[test]
    fn the_listing_hides_trashed_rows_and_paths_and_flags_missing_files() {
        let f = fx();
        let a = f.add("b.png", PNG);
        let gone = f.add("a.pdf", b"%PDF-1");
        let trashed = f.add("c.txt", b"hi");
        f.conn.execute("UPDATE companion_files SET deleted_at='x' WHERE id=?1", [trashed]).unwrap();
        std::fs::remove_file(f.extras.join("a.pdf")).unwrap();
        let plain = list_for_web(&f.conn, f.model, &f.roots()).unwrap();
        assert_eq!(plain.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["a.pdf", "b.png"], "the plain listing hides removed rows");
        let list = list_for_web_with(&f.conn, f.model, &f.roots(), true).unwrap();
        // The removed one is listed last, with its timestamp and without any path.
        assert_eq!(
            list.iter().map(|c| (c.name.as_str(), c.missing)).collect::<Vec<_>>(),
            vec![("a.pdf", true), ("b.png", false), ("c.txt", false)]
        );
        assert_eq!(list[2].deleted_at.as_deref(), Some("x"));
        assert_eq!(list[2].restorable, Some(true), "no trash file: only the row comes back");
        assert!(list[0].deleted_at.is_none() && list[0].restorable.is_none());
        f.conn
            .execute("UPDATE companion_files SET trash_path='/desktop/trash/companion-3-x.txt' WHERE id=?1", [trashed])
            .unwrap();
        assert_eq!(list_for_web_with(&f.conn, f.model, &f.roots(), true).unwrap()[2].restorable, Some(false), "the desktop's trash");
        assert_eq!(list[1].id, a.to_string());
        assert_eq!(list[0].id, gone.to_string());
        assert!(list[1].previewable && !list[0].previewable && !list[2].previewable);
        assert!(!serde_json::to_string(&list).unwrap().contains(f.dir.to_str().unwrap()));
        assert_eq!(list_for_web(&f.conn, 424242, &f.roots()).err(), Some(RowError::ModelMissing));
    }

    #[cfg(unix)]
    #[test]
    fn the_listing_flags_a_symlinked_folder_and_a_folder_outside_the_roots() {
        let f = fx();
        f.add("pic.png", PNG);
        assert!(!list_for_web(&f.conn, f.model, &f.roots()).unwrap()[0].missing);
        assert!(list_for_web(&f.conn, f.model, &[PathBuf::from("/nonexistent-root")]).unwrap()[0].missing);
        let moved = f.dir.join("moved");
        std::fs::rename(&f.extras, &moved).unwrap();
        std::os::unix::fs::symlink(&moved, &f.extras).unwrap();
        let list = list_for_web(&f.conn, f.model, &f.roots()).unwrap();
        assert!(list[0].missing && !list[0].previewable);
    }

    #[cfg(unix)]
    #[test]
    fn the_strict_open_itself_refuses_a_symlink_name() {
        // Independent of the lstat pre-check in `open_row_file`.
        let f = fx();
        std::fs::write(f.dir.join("real.png"), PNG).unwrap();
        std::os::unix::fs::symlink(f.dir.join("real.png"), f.extras.join("link.png")).unwrap();
        assert_eq!(open_strict(&f.extras.join("link.png"), None, || {}).err(), Some(RowError::NotAFile));
        std::fs::write(f.extras.join("ok.png"), PNG).unwrap();
        assert!(open_strict(&f.extras.join("ok.png"), Some(&f.roots()), || {}).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn swapping_the_folder_between_check_and_open_is_refused() {
        let f = fx();
        std::fs::write(f.extras.join("pic.png"), PNG).unwrap();
        let swapped = open_strict(&f.extras.join("pic.png"), Some(&f.roots()), || {
            // A look-alike folder with the same file name takes the place.
            let decoy = f.dir.join("decoy");
            std::fs::create_dir(&decoy).unwrap();
            std::fs::write(decoy.join("pic.png"), b"other").unwrap();
            std::fs::rename(&f.extras, f.dir.join("old")).unwrap();
            std::fs::rename(&decoy, &f.extras).unwrap();
        });
        assert_eq!(swapped.err(), Some(RowError::Folder(FolderProblem::Elsewhere)));
        // Replaced by a link instead.
        let g = fx();
        std::fs::write(g.extras.join("pic.png"), PNG).unwrap();
        let linked = open_strict(&g.extras.join("pic.png"), Some(&g.roots()), || {
            let old = g.dir.join("old");
            std::fs::rename(&g.extras, &old).unwrap();
            std::os::unix::fs::symlink(&old, &g.extras).unwrap();
        });
        assert_eq!(linked.err(), Some(RowError::Folder(FolderProblem::NotReal)));
    }

    #[test]
    fn the_preview_is_a_capped_data_uri() {
        let f = fx();
        let id = f.add("pic.png", PNG);
        let (_, _, mut file, head) = f.open(id).unwrap();
        let uri = image_data_uri(&mut file, head, "pic.png").unwrap();
        assert!(uri.starts_with("data:image/png;base64,"));
        let big = vec![7u8; MAX_PREVIEW_BYTES as usize];
        std::fs::write(f.extras.join("pic.png"), [PNG, &big[..]].concat()).unwrap();
        let (_, _, mut file, head) = open_for_web(&f.conn, id, &f.roots(), u64::MAX).unwrap();
        assert_eq!(image_data_uri(&mut file, head, "pic.png").err(), Some(RowError::TooLarge));
    }

    #[test]
    fn downloads_never_get_a_renderable_type_except_images() {
        assert_eq!(download_content_type(CompanionKind::Pdf, "a.pdf"), "application/octet-stream");
        assert_eq!(download_content_type(CompanionKind::Text, "a.md"), "application/octet-stream");
        assert_eq!(download_content_type(CompanionKind::Image, "a.webp"), "image/webp");
    }
}
