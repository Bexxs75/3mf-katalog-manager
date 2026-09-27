use super::*;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderDto {
    pub id: String,
    pub name: String,
    pub path: String,
    pub parent_id: Option<String>,
    pub count: i64,
}
#[tauri::command]
pub fn list_folders(state: State<AppState>) -> CmdResult<Vec<FolderDto>> {
    let conn = lock_db(&state)?;
    let folders = db::list_folders(&conn).map_err(|e| e.to_string())?;
    let files = db::list_files(&conn).map_err(|e| e.to_string())?;

    fn is_descendant_or_self(folders: &[db::models::FolderRecord], candidate_id: i64, ancestor_id: i64) -> bool {
        if candidate_id == ancestor_id {
            return true;
        }
        let mut current = candidate_id;
        while let Some(f) = folders.iter().find(|f| f.id == current) {
            match f.parent_id {
                Some(pid) if pid == ancestor_id => return true,
                Some(pid) => current = pid,
                None => return false,
            }
        }
        false
    }

    let dtos = folders
        .iter()
        .map(|folder| {
            let count = files
                .iter()
                .filter(|f| f.folder_id.is_some_and(|fid| is_descendant_or_self(&folders, fid, folder.id)))
                .count() as i64;
            FolderDto {
                id: folder.id.to_string(),
                name: folder.name.clone(),
                path: folder.path.clone(),
                parent_id: folder.parent_id.map(|id| id.to_string()),
                count,
            }
        })
        .collect();

    Ok(dtos)
}
/// `name` ends up in `join`/`with_file_name` and must be a single harmless path
/// component: "../../etc/x" or "/etc/x" would otherwise create outside the
/// catalog, just by typing text (CWE-22).
fn validate_folder_name(name: &str) -> CmdResult<()> {
    if name.trim().is_empty() {
        return Err(CmdError::expected("Ordnername darf nicht leer sein"));
    }
    if name.contains('/') || name.contains('\\') {
        return Err(CmdError::expected("Ordnername darf keine Pfad-Trennzeichen enthalten"));
    }
    if name == "." || name == ".." {
        return Err(CmdError::expected("Ungueltiger Ordnername"));
    }
    Ok(())
}
/// Creates a real folder on disk (below an existing folder, or - with
/// `parent_id: None` - below a base directory the user picked in a dialog) and a
/// matching `folders` row. Fails if the target already exists (`std::fs::create_dir`,
/// not `create_dir_all`).
#[tauri::command]
pub async fn create_folder(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    parent_id: Option<String>,
    name: String,
) -> CmdResult<FolderDto> {
    validate_folder_name(&name)?;
    let parent: Option<i64> = parent_id
        .map(|s| s.parse::<i64>().map_err(|_| "invalid folder id".to_string()))
        .transpose()?;

    let new_dir = {
        let conn = lock_db(&state)?;
        match parent {
            Some(pid) => {
                let folders = db::list_folders(&conn).map_err(|e| e.to_string())?;
                let parent_folder = folders.iter().find(|f| f.id == pid).ok_or_else(|| "folder not found".to_string())?;
                std::path::PathBuf::from(&parent_folder.path).join(&name)
            }
            None => {
                let picked = app.dialog().file().blocking_pick_folder();
                let Some(picked) = picked else {
                    return Err(CmdError::expected("cancelled"));
                };
                picked.into_path().map_err(|e| e.to_string())?.join(&name)
            }
        }
    };

    reject_if_sensitive_path(&new_dir, &state.sensitive_dirs)?;
    std::fs::create_dir(&new_dir).map_err(|e| e.to_string())?;

    let conn = lock_db(&state)?;
    let new_id = db::insert_folder_with_parent(&conn, &name, parent, &new_dir.to_string_lossy())
        .map_err(|e| e.to_string())?;
    Ok(FolderDto {
        id: new_id.to_string(),
        name,
        path: new_dir.to_string_lossy().to_string(),
        parent_id: parent.map(|p| p.to_string()),
        count: 0,
    })
}
/// Core logic of `rename_folder`, without `State`, so it's testable.
fn rename_folder_with_conn(
    conn: &Connection,
    id: i64,
    name: String,
    sensitive_dirs: &[PathBuf],
) -> CmdResult<()> {
    validate_folder_name(&name)?;
    let folders = db::list_folders(conn).map_err(|e| e.to_string())?;
    let folder = folders.iter().find(|f| f.id == id).ok_or_else(|| "folder not found".to_string())?.clone();

    let old_path = std::path::PathBuf::from(&folder.path);
    let new_path = old_path.with_file_name(&name);
    reject_if_sensitive_path(&old_path, sensitive_dirs)?;
    reject_if_sensitive_path(&new_path, sensitive_dirs)?;

    if new_path.exists() {
        return Err(CmdError::expected(format!("Zielordner existiert bereits: {}", new_path.display())));
    }
    std::fs::rename(&old_path, &new_path).map_err(|e| e.to_string())?;

    let db_result = (|| -> Result<(), DbError> {
        let tx = conn.unchecked_transaction()?;
        db::rename_folder_name(&tx, id, &name)?;
        db::update_paths_under_folder(&tx, id, &folder.path, &new_path.to_string_lossy())?;
        tx.commit().map_err(DbError::from)
    })();

    if let Err(db_err) = db_result {
        if let Err(rollback_err) = std::fs::rename(&new_path, &old_path) {
            return Err(format!(
                "DB-Update fehlgeschlagen ({db_err}) UND Rollback der Ordner-Umbenennung fehlgeschlagen ({rollback_err}) - Ordner heisst jetzt {}, DB verweist teilweise noch auf {}",
                new_path.display(),
                old_path.display()
            )
            .into());
        }
        return Err(db_err.to_string().into());
    }
    Ok(())
}
/// Renames a real folder on disk (`std::fs::rename`) and updates `folders.name`
/// plus, recursively, `folders.path`/`files.path` for the folder and all its
/// descendants.
#[tauri::command]
pub fn rename_folder(state: State<AppState>, folder_id: String, name: String) -> CmdResult<()> {
    let id: i64 = folder_id.parse().map_err(|_| "invalid folder id".to_string())?;
    let conn = lock_db(&state)?;
    rename_folder_with_conn(&conn, id, name, &state.sensitive_dirs)
}
/// Whether `candidate_id` is a (direct or indirect) descendant of `ancestor_id`.
fn is_descendant(folders: &[db::models::FolderRecord], candidate_id: i64, ancestor_id: i64) -> bool {
    let mut current = candidate_id;
    while let Some(f) = folders.iter().find(|f| f.id == current) {
        match f.parent_id {
            Some(pid) if pid == ancestor_id => return true,
            Some(pid) => current = pid,
            None => return false,
        }
    }
    false
}
/// Core logic of `move_folder`, without `State`, so it's testable.
fn move_folder_with_conn(
    conn: &Connection,
    id: i64,
    target: Option<i64>,
    sensitive_dirs: &[PathBuf],
) -> CmdResult<()> {
    let Some(target) = target else {
        // There's no "move to root": the disk would stay unchanged, but the DB would
        // no longer see a parent.
        return Err("Ein Ordner kann nicht ohne Zielordner verschoben werden".into());
    };

    let folders = db::list_folders(conn).map_err(|e| e.to_string())?;
    let folder = folders.iter().find(|f| f.id == id).ok_or_else(|| "folder not found".to_string())?.clone();

    if target == id || is_descendant(&folders, target, id) {
        return Err(CmdError::expected("Ein Ordner kann nicht in einen eigenen Unterordner verschoben werden"));
    }

    let new_parent_path = folders
        .iter()
        .find(|f| f.id == target)
        .map(|f| f.path.clone())
        .ok_or_else(|| "target folder not found".to_string())?;
    let target = Some(target);

    let old_path = std::path::PathBuf::from(&folder.path);
    let new_path = std::path::PathBuf::from(&new_parent_path).join(&folder.name);
    reject_if_sensitive_path(&old_path, sensitive_dirs)?;
    reject_if_sensitive_path(&new_path, sensitive_dirs)?;

    if new_path.exists() {
        return Err(CmdError::expected(format!("Zielordner existiert bereits: {}", new_path.display())));
    }
    std::fs::rename(&old_path, &new_path).map_err(|e| e.to_string())?;

    // Parent and paths in one transaction, so the DB is never half-updated while
    // the disk has already moved. unchecked_transaction because we only have
    // `&Connection`.
    let db_result = (|| -> Result<(), DbError> {
        let tx = conn.unchecked_transaction()?;
        db::set_folder_parent(&tx, id, target)?;
        db::update_paths_under_folder(&tx, id, &folder.path, &new_path.to_string_lossy())?;
        tx.commit().map_err(DbError::from)
    })();

    if let Err(db_err) = db_result {
        if let Err(rollback_err) = std::fs::rename(&new_path, &old_path) {
            return Err(format!(
                "DB-Update fehlgeschlagen ({db_err}) UND Rollback des Ordner-Moves fehlgeschlagen ({rollback_err}) - Ordner liegt jetzt unter {}, DB verweist teilweise noch auf {}",
                new_path.display(),
                old_path.display()
            )
            .into());
        }
        return Err(db_err.to_string().into());
    }
    Ok(())
}
/// Moves a folder on disk below another parent folder and updates `parent_id`
/// and all paths below. Rejects cycles and `new_parent_id: None`.
#[tauri::command]
pub fn move_folder(state: State<AppState>, folder_id: String, new_parent_id: Option<String>) -> CmdResult<()> {
    let id: i64 = folder_id.parse().map_err(|_| "invalid folder id".to_string())?;
    let target: Option<i64> = new_parent_id
        .map(|s| s.parse::<i64>().map_err(|_| "invalid folder id".to_string()))
        .transpose()?;

    let conn = lock_db(&state)?;
    move_folder_with_conn(&conn, id, target, &state.sensitive_dirs)
}
// async because blocking_pick_folder() blocks until the dialog closes. The
// picked folder then counts as an allowed extraction target (`ApprovedTargets`).
#[tauri::command]
pub async fn pick_folder_path(
    app: tauri::AppHandle,
    approved: State<'_, ApprovedTargets>,
) -> CmdResult<Option<String>> {
    let picked = app.dialog().file().blocking_pick_folder();
    let path = picked.and_then(|p| p.into_path().ok());
    if let Some(path) = &path {
        approved.approve(path);
    }
    Ok(path.map(|p| p.to_string_lossy().to_string()))
}
/// Core logic of `register_catalog_base_dir`, without `State`, so it's testable.
fn register_catalog_base_dir_with_conn(conn: &Connection, dir: &Path) -> CmdResult<FolderDto> {
    if !dir.exists() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }

    let id = db::ensure_folder_path(conn, dir, dir).map_err(|e| e.to_string())?;

    let folders = db::list_folders(conn).map_err(|e| e.to_string())?;
    let folder = folders.iter().find(|f| f.id == id).ok_or_else(|| "folder not found".to_string())?;

    Ok(FolderDto {
        id: folder.id.to_string(),
        name: folder.name.clone(),
        path: folder.path.clone(),
        parent_id: folder.parent_id.map(|p| p.to_string()),
        count: 0,
    })
}
/// Registers a picked base directory as the catalog root: creates it if needed
/// and idempotently ensures a `folders` row.
#[tauri::command]
pub fn register_catalog_base_dir(state: State<AppState>, path: String) -> CmdResult<FolderDto> {
    let dir = std::path::PathBuf::from(&path);
    let conn = lock_db(&state)?;
    register_catalog_base_dir_with_conn(&conn, &dir)
}

/// Like `register_catalog_base_dir_with_conn`, but NEVER creates a directory:
/// at startup a storage location deleted in the meantime must not silently
/// reappear. `None` if the folder is missing.
fn register_existing_catalog_base_dir_with_conn(conn: &Connection, dir: &Path) -> CmdResult<Option<FolderDto>> {
    if !dir.is_dir() {
        return Ok(None);
    }
    register_catalog_base_dir_with_conn(conn, dir).map(Some)
}
/// Called at startup: makes sure a configured storage location has a folder row
/// (among other things so it counts as an extraction target, see
/// `target_is_approved`).
#[tauri::command]
pub fn register_existing_catalog_base_dir(state: State<AppState>, path: String) -> CmdResult<Option<FolderDto>> {
    let conn = lock_db(&state)?;
    register_existing_catalog_base_dir_with_conn(&conn, Path::new(&path))
}

// Windows forbids these in file names. They are rejected on every OS so a
// catalog set up on Linux or macOS keeps working after a move to Windows.
const FORBIDDEN_DIR_NAME_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

// Device names Windows refuses as file or folder names, also with an extension.
const WINDOWS_RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1",
    "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Name of a new catalog folder, typed freely by the user. Must be exactly one
/// harmless path component, otherwise "../x" or "/etc/x" would create the folder
/// outside the chosen place (CWE-22). The frontend shows the same rules as a hint,
/// this check is the one that counts.
fn validate_new_catalog_dir_name(name: &str) -> CmdResult<()> {
    if name.trim().is_empty() {
        return Err(CmdError::expected("Der Ordnername darf nicht leer sein"));
    }
    if let Some(c) = name.chars().find(|c| FORBIDDEN_DIR_NAME_CHARS.contains(c) || c.is_control()) {
        let shown = if c.is_control() { format!("U+{:04X}", c as u32) } else { c.to_string() };
        return Err(CmdError::expected(format!("Dieses Zeichen ist in Ordnernamen nicht erlaubt: {shown}")));
    }
    if name == "." || name == ".." {
        return Err(CmdError::expected("Dieser Ordnername ist nicht erlaubt"));
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Err(CmdError::expected("Ordnernamen dürfen nicht mit einem Punkt oder Leerzeichen enden"));
    }
    let stem = name.split('.').next().unwrap_or(name).trim_end().to_ascii_uppercase();
    if WINDOWS_RESERVED_NAMES.contains(&stem.as_str()) {
        return Err(CmdError::expected("Dieser Ordnername ist unter Windows reserviert"));
    }
    // Defence in depth: whatever slipped through above must still be a single
    // plain component, never a root, prefix or parent reference.
    let mut components = Path::new(name).components();
    if !matches!((components.next(), components.next()), (Some(std::path::Component::Normal(_)), None)) {
        return Err(CmdError::expected("Dieser Ordnername ist nicht erlaubt"));
    }
    Ok(())
}

/// Full path of the new catalog folder. The parent must have come from the
/// backend (folder picker or the Documents suggestion): a compromised frontend
/// could otherwise create folders anywhere the user can write.
fn new_catalog_dir_target(approved: &ApprovedTargets, parent: &Path, name: &str) -> CmdResult<PathBuf> {
    validate_new_catalog_dir_name(name)?;
    if !approved.contains(parent) {
        return Err("Der übergeordnete Ordner wurde nicht in der App ausgewählt".into());
    }
    Ok(parent.join(name))
}

/// Creates the catalog folder, or reuses it if it already exists (the user may
/// simply retype the name of a folder they created by hand). The result counts
/// as picked in the app, like a folder from `pick_folder_path`.
fn create_catalog_dir_at(
    approved: &ApprovedTargets,
    sensitive_dirs: &[PathBuf],
    parent: &Path,
    name: &str,
) -> CmdResult<PathBuf> {
    let target = new_catalog_dir_target(approved, parent, name)?;
    reject_if_sensitive_path(&target, sensitive_dirs)?;
    if target.is_dir() {
        log::info!(target: "setup", "Katalog-Ordner existiert bereits und wird verwendet");
    } else if target.exists() {
        return Err(CmdError::expected("Unter diesem Namen gibt es dort schon eine Datei"));
    } else {
        // create_dir, not create_dir_all: a parent deleted in the meantime must
        // not be recreated behind the user's back.
        std::fs::create_dir(&target).map_err(|e| e.to_string())?;
        log::info!(target: "setup", "Katalog-Ordner angelegt");
    }
    approved.approve(&target);
    Ok(target)
}

/// Suggested place for a new catalog folder: the user's Documents folder, if
/// the system has one. Approved right here, because the frontend passes it
/// back to `create_catalog_dir` like a picked folder.
#[tauri::command]
pub fn default_catalog_parent(app: tauri::AppHandle, approved: State<'_, ApprovedTargets>) -> Option<String> {
    use tauri::Manager;
    let dir = app.path().document_dir().ok().filter(|d| d.is_dir())?;
    approved.approve(&dir);
    Some(dir.to_string_lossy().into_owned())
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CatalogDirPreview {
    pub path: String,
    pub exists: bool,
}

/// Shows what `create_catalog_dir` would do, before anything happens on disk.
/// The path is joined here so it uses the separator of the running OS.
#[tauri::command]
pub fn preview_catalog_dir(
    approved: State<'_, ApprovedTargets>,
    parent: String,
    name: String,
) -> CmdResult<CatalogDirPreview> {
    let target = new_catalog_dir_target(&approved, Path::new(&parent), &name)?;
    Ok(CatalogDirPreview { exists: target.is_dir(), path: target.to_string_lossy().into_owned() })
}

/// Creates (or reuses) the folder `name` inside an approved `parent` and returns
/// its path; the caller then registers it with `register_catalog_base_dir`.
/// The OS folder picker can't create folders everywhere (e.g. GTK on Linux),
/// so the app does it itself.
#[tauri::command]
pub fn create_catalog_dir(
    state: State<'_, AppState>,
    approved: State<'_, ApprovedTargets>,
    parent: String,
    name: String,
) -> CmdResult<String> {
    create_catalog_dir_at(&approved, &state.sensitive_dirs, Path::new(&parent), &name)
        .map(|p| p.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_registration_of_the_base_dir_never_creates_a_missing_directory() {
        let conn = crate::db::connect_in_memory().expect("connect");
        let missing = unique_test_dir("register-existing-missing");
        std::fs::remove_dir_all(&missing).unwrap();
        assert!(register_existing_catalog_base_dir_with_conn(&conn, &missing).unwrap().is_none());
        assert!(!missing.exists(), "Startaufruf darf keinen Ordner anlegen");

        let existing = unique_test_dir("register-existing-present");
        let first = register_existing_catalog_base_dir_with_conn(&conn, &existing).unwrap().unwrap();
        let second = register_existing_catalog_base_dir_with_conn(&conn, &existing).unwrap().unwrap();
        assert_eq!(first.id, second.id, "idempotent");
        assert_eq!(first.path, existing.to_string_lossy());
    }

    #[test]
    fn register_catalog_base_dir_creates_missing_directory_and_is_idempotent() {
        // The target directory should still be missing: only use the unique path.
        let base = unique_test_dir("register-base-dir");
        std::fs::remove_dir_all(&base).expect("remove freshly created test dir");
        assert!(!base.exists());

        let conn = crate::db::connect_in_memory().expect("connect");

        let first = register_catalog_base_dir_with_conn(&conn, &base).expect("first call should succeed");
        assert!(base.exists(), "directory must be created on disk");
        assert_eq!(first.name, base.file_name().unwrap().to_string_lossy());
        assert_eq!(first.parent_id, None);

        let second = register_catalog_base_dir_with_conn(&conn, &base).expect("second call should succeed");
        assert_eq!(first.id, second.id, "same path must resolve to the same folder id");

        assert_eq!(db::list_folders(&conn).unwrap().len(), 1, "must not create a duplicate folders row");

        let _ = std::fs::remove_dir_all(&base);
    }
    #[test]
    fn move_folder_rejects_moving_into_own_descendant() {
        let tmp = unique_test_dir("move_folder_cycle");
        let a_dir = tmp.join("A");
        let b_dir = a_dir.join("B");
        std::fs::create_dir_all(&b_dir).unwrap();

        let conn = crate::db::connect_in_memory().expect("connect");
        let a_id = db::insert_folder_with_parent(&conn, "A", None, &a_dir.to_string_lossy()).expect("insert A");
        let b_id = db::insert_folder_with_parent(&conn, "B", Some(a_id), &b_dir.to_string_lossy()).expect("insert B");

        let result = move_folder_with_conn(&conn, a_id, Some(b_id), &[]);
        assert!(result.is_err(), "moving A into its own descendant B must be rejected");

        // Neither disk nor DB may have changed.
        assert!(a_dir.exists());
        assert!(b_dir.exists());
        let folders = db::list_folders(&conn).expect("list_folders");
        let a_after = folders.iter().find(|f| f.id == a_id).expect("A still present");
        assert_eq!(a_after.parent_id, None, "A's parent_id must be unchanged after rejected move");

        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn move_folder_rejects_none_target() {
        // move_folder(id, None) must be rejected (see move_folder_with_conn).
        let tmp = unique_test_dir("move_folder_none_target");
        let a_dir = tmp.join("A");
        let b_dir = a_dir.join("B");
        std::fs::create_dir_all(&b_dir).unwrap();

        let conn = crate::db::connect_in_memory().expect("connect");
        let a_id = db::insert_folder_with_parent(&conn, "A", None, &a_dir.to_string_lossy()).expect("insert A");
        let b_id = db::insert_folder_with_parent(&conn, "B", Some(a_id), &b_dir.to_string_lossy()).expect("insert B");

        let result = move_folder_with_conn(&conn, b_id, None, &[]);
        assert!(result.is_err(), "move_folder with new_parent_id=None must be rejected");

        assert!(b_dir.exists(), "B must remain at its old physical location");
        let folders = db::list_folders(&conn).expect("list_folders");
        let b_after = folders.iter().find(|f| f.id == b_id).expect("B still present");
        assert_eq!(b_after.parent_id, Some(a_id), "B's parent_id must be unchanged after rejected move");

        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn validate_folder_name_rejects_path_traversal_and_separators() {
        // create_folder/rename_folder must never use `name` unchecked (CWE-22).
        assert!(validate_folder_name("../etc").is_err());
        assert!(validate_folder_name("../../tmp/evil").is_err());
        assert!(validate_folder_name("a/b").is_err());
        assert!(validate_folder_name("a\\b").is_err());
        assert!(validate_folder_name("/etc").is_err());
        assert!(validate_folder_name("..").is_err());
        assert!(validate_folder_name(".").is_err());
        assert!(validate_folder_name("").is_err());
        assert!(validate_folder_name("   ").is_err());

        assert!(validate_folder_name("Tabletop").is_ok());
        assert!(validate_folder_name("Ersatzteile 2026").is_ok());
    }
    #[test]
    fn rename_folder_with_conn_rejects_sensitive_target_path() {
        let tmp = unique_test_dir("rename_folder_sensitive");
        let a_dir = tmp.join("A");
        std::fs::create_dir_all(&a_dir).unwrap();
        let sensitive = vec![tmp.clone()];

        let conn = crate::db::connect_in_memory().expect("connect");
        let a_id = db::insert_folder_with_parent(&conn, "A", None, &a_dir.to_string_lossy()).expect("insert A");

        let result = rename_folder_with_conn(&conn, a_id, "B".to_string(), &sensitive);
        assert!(result.is_err(), "rename_folder_with_conn must reject a target path under a sensitive directory");
        assert!(a_dir.exists(), "original directory must be untouched after a rejected rename");

        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn move_folder_with_conn_rejects_sensitive_target_path() {
        let tmp = unique_test_dir("move_folder_sensitive");
        let a_dir = tmp.join("A");
        let b_dir = tmp.join("B");
        std::fs::create_dir_all(&a_dir).unwrap();
        std::fs::create_dir_all(&b_dir).unwrap();
        let sensitive = vec![tmp.clone()];

        let conn = crate::db::connect_in_memory().expect("connect");
        let a_id = db::insert_folder_with_parent(&conn, "A", None, &a_dir.to_string_lossy()).expect("insert A");
        let b_id = db::insert_folder_with_parent(&conn, "B", None, &b_dir.to_string_lossy()).expect("insert B");

        let result = move_folder_with_conn(&conn, a_id, Some(b_id), &sensitive);
        assert!(result.is_err(), "move_folder_with_conn must reject a move into a sensitive directory");
        assert!(a_dir.exists(), "original directory must be untouched after a rejected move");

        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn rename_folder_with_conn_rejects_name_with_path_traversal() {
        let tmp = unique_test_dir("rename_folder_traversal");
        let a_dir = tmp.join("A");
        std::fs::create_dir_all(&a_dir).unwrap();

        let conn = crate::db::connect_in_memory().expect("connect");
        let a_id = db::insert_folder_with_parent(&conn, "A", None, &a_dir.to_string_lossy()).expect("insert A");

        let result = rename_folder_with_conn(&conn, a_id, "../../escaped".to_string(), &[]);
        assert!(result.is_err(), "rename_folder_with_conn must reject a name containing path separators");

        assert!(a_dir.exists(), "original directory must be untouched after a rejected rename");
        let folders = db::list_folders(&conn).expect("list_folders");
        let a_after = folders.iter().find(|f| f.id == a_id).expect("A still present");
        assert_eq!(a_after.name, "A", "name in the DB must be unchanged after a rejected rename");

        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn move_folder_updates_all_descendant_paths() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        use zip::ZipWriter;

        fn write_minimal_3mf(path: &std::path::Path) {
            let mut buf = Vec::new();
            {
                let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
                let options = SimpleFileOptions::default();
                zip.start_file("[Content_Types].xml", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#).unwrap();
                zip.start_file("_rels/.rels", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rel1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/3dmodel.model"/></Relationships>"#).unwrap();
                zip.start_file("3D/3dmodel.model", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources></resources><build></build></model>"#).unwrap();
                zip.finish().unwrap();
            }
            std::fs::write(path, &buf).expect("write temp file");
        }

        // Three levels, so a recursion bug in update_paths_under_folder shows up.
        let tmp = unique_test_dir("move_folder_descendants");
        let a_dir = tmp.join("A");
        let b_dir = a_dir.join("B");
        let c_dir = b_dir.join("C");
        let x_dir = tmp.join("X");
        std::fs::create_dir_all(&c_dir).unwrap();
        std::fs::create_dir_all(&x_dir).unwrap();
        let file_path = c_dir.join("model.3mf");
        write_minimal_3mf(&file_path);

        let conn = crate::db::connect_in_memory().expect("connect");
        let a_id = db::insert_folder_with_parent(&conn, "A", None, &a_dir.to_string_lossy()).expect("insert A");
        let b_id = db::insert_folder_with_parent(&conn, "B", Some(a_id), &b_dir.to_string_lossy()).expect("insert B");
        let c_id = db::insert_folder_with_parent(&conn, "C", Some(b_id), &c_dir.to_string_lossy()).expect("insert C");
        let x_id = db::insert_folder_with_parent(&conn, "X", None, &x_dir.to_string_lossy()).expect("insert X");

        let imported = import_one(&conn, &file_path, None, None, Some(c_id)).expect("import should succeed");
        let file_id: i64 = imported.id.parse().unwrap();

        move_folder_with_conn(&conn, b_id, Some(x_id), &[]).expect("move should succeed");

        let expected_b_dir = x_dir.join("B");
        let expected_c_dir = expected_b_dir.join("C");
        let expected_file_path = expected_c_dir.join("model.3mf");

        assert!(expected_b_dir.exists(), "B must physically exist under X after move");
        assert!(expected_c_dir.exists(), "C must physically exist under the new B location");
        assert!(expected_file_path.exists(), "file must physically exist under the new C location");
        assert!(!a_dir.join("B").exists(), "B must no longer exist under its old parent A");

        let folders = db::list_folders(&conn).expect("list_folders");
        let b_after = folders.iter().find(|f| f.id == b_id).expect("B present");
        assert_eq!(b_after.parent_id, Some(x_id), "B's parent_id must now be X");
        assert_eq!(b_after.path, expected_b_dir.to_string_lossy().to_string());

        let c_after = folders.iter().find(|f| f.id == c_id).expect("C present");
        assert_eq!(c_after.path, expected_c_dir.to_string_lossy().to_string(), "C's path must carry the new B prefix");

        let file_after = db::get_file(&conn, file_id).expect("get_file").expect("file exists");
        assert_eq!(
            file_after.path,
            expected_file_path.to_string_lossy().to_string(),
            "file path must carry the new B/C prefix"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn rename_folder_updates_own_and_descendant_paths() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        use zip::ZipWriter;

        fn write_minimal_3mf(path: &std::path::Path) {
            let mut buf = Vec::new();
            {
                let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
                let options = SimpleFileOptions::default();
                zip.start_file("[Content_Types].xml", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#).unwrap();
                zip.start_file("_rels/.rels", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rel1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/3dmodel.model"/></Relationships>"#).unwrap();
                zip.start_file("3D/3dmodel.model", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources></resources><build></build></model>"#).unwrap();
                zip.finish().unwrap();
            }
            std::fs::write(path, &buf).expect("write temp file");
        }

        // Three levels: B, C and the file must all follow.
        let tmp = unique_test_dir("rename_folder_descendants");
        let a_dir = tmp.join("A");
        let b_dir = a_dir.join("B");
        let c_dir = b_dir.join("C");
        std::fs::create_dir_all(&c_dir).unwrap();
        let file_path = c_dir.join("model.3mf");
        write_minimal_3mf(&file_path);

        let conn = crate::db::connect_in_memory().expect("connect");
        let a_id = db::insert_folder_with_parent(&conn, "A", None, &a_dir.to_string_lossy()).expect("insert A");
        let b_id = db::insert_folder_with_parent(&conn, "B", Some(a_id), &b_dir.to_string_lossy()).expect("insert B");
        let c_id = db::insert_folder_with_parent(&conn, "C", Some(b_id), &c_dir.to_string_lossy()).expect("insert C");

        let imported = import_one(&conn, &file_path, None, None, Some(c_id)).expect("import should succeed");
        let file_id: i64 = imported.id.parse().unwrap();

        rename_folder_with_conn(&conn, b_id, "B2".to_string(), &[]).expect("rename should succeed");

        let expected_b_dir = a_dir.join("B2");
        let expected_c_dir = expected_b_dir.join("C");
        let expected_file_path = expected_c_dir.join("model.3mf");

        assert!(expected_b_dir.exists(), "B2 must physically exist");
        assert!(expected_c_dir.exists(), "C must physically exist under the renamed B2");
        assert!(expected_file_path.exists(), "file must physically exist under the renamed B2/C");
        assert!(!b_dir.exists(), "old B directory must no longer exist");

        let folders = db::list_folders(&conn).expect("list_folders");
        let b_after = folders.iter().find(|f| f.id == b_id).expect("B present");
        assert_eq!(b_after.name, "B2");
        assert_eq!(b_after.path, expected_b_dir.to_string_lossy().to_string());

        let c_after = folders.iter().find(|f| f.id == c_id).expect("C present");
        assert_eq!(c_after.path, expected_c_dir.to_string_lossy().to_string(), "C's path must carry the new B2 prefix");

        let file_after = db::get_file(&conn, file_id).expect("get_file").expect("file exists");
        assert_eq!(
            file_after.path,
            expected_file_path.to_string_lossy().to_string(),
            "file path must carry the new B2/C prefix"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn rename_folder_with_non_ascii_name_produces_correct_child_paths() {
        // substr() counts characters, not bytes: with a non-ASCII character a byte
        // offset would swallow the separator ("Neumodel.3mf").
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        use zip::ZipWriter;

        fn write_minimal_3mf(path: &std::path::Path) {
            let mut buf = Vec::new();
            {
                let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
                let options = SimpleFileOptions::default();
                zip.start_file("[Content_Types].xml", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#).unwrap();
                zip.start_file("_rels/.rels", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rel1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/3dmodel.model"/></Relationships>"#).unwrap();
                zip.start_file("3D/3dmodel.model", options).unwrap();
                zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources></resources><build></build></model>"#).unwrap();
                zip.finish().unwrap();
            }
            std::fs::write(path, &buf).expect("write temp file");
        }

        let tmp = unique_test_dir("rename_folder_non_ascii");
        let src_dir = tmp.join("Pr\u{fc}fen");
        std::fs::create_dir_all(&src_dir).unwrap();
        let file_path = src_dir.join("model.3mf");
        write_minimal_3mf(&file_path);

        let conn = crate::db::connect_in_memory().expect("connect");
        let folder_id =
            db::insert_folder_with_parent(&conn, "Pr\u{fc}fen", None, &src_dir.to_string_lossy()).expect("insert folder");

        let imported = import_one(&conn, &file_path, None, None, Some(folder_id)).expect("import should succeed");
        let file_id: i64 = imported.id.parse().unwrap();

        rename_folder_with_conn(&conn, folder_id, "Neu".to_string(), &[]).expect("rename should succeed");

        let expected_dir = tmp.join("Neu");
        let expected_file_path = expected_dir.join("model.3mf");

        assert!(expected_dir.exists(), "renamed folder must physically exist");
        assert!(expected_file_path.exists(), "file must physically exist under the renamed folder");

        let file_after = db::get_file(&conn, file_id).expect("get_file").expect("file exists");
        assert_eq!(
            file_after.path,
            expected_file_path.to_string_lossy().to_string(),
            "file path must be exactly '.../Neu/model.3mf', not corrupted by a byte-vs-character substr offset"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn rename_folder_compensates_when_db_update_fails() {
        let dir = unique_test_dir("rename_folder_compensation");
        std::fs::create_dir_all(dir.join("Alt")).unwrap();

        let conn = db::connect_in_memory().unwrap();
        let folder_id = db::insert_folder_with_parent(&conn, "Alt", None, &dir.join("Alt").to_string_lossy()).unwrap();
        // folders.path is UNIQUE: forces a DB error after the rename.
        db::insert_folder_with_parent(&conn, "Neu", None, &dir.join("Neu").to_string_lossy()).unwrap();

        let result = rename_folder_with_conn(&conn, folder_id, "Neu".to_string(), &[]);

        assert!(result.is_err());
        assert!(dir.join("Alt").exists(), "folder must be renamed back after the DB update failed");
        assert!(!dir.join("Neu").is_dir() || std::fs::read_dir(dir.join("Neu")).unwrap().count() == 0);
    }

    #[test]
    fn new_catalog_dir_name_accepts_ordinary_names() {
        for name in ["3D-Katalog", "Meine Modelle", "Catálogo 3D", ".versteckt", "a.b"] {
            assert!(validate_new_catalog_dir_name(name).is_ok(), "{name} should be allowed");
        }
    }

    #[test]
    fn new_catalog_dir_name_rejects_empty_forbidden_and_reserved_names() {
        for name in ["", "   ", "a/b", "a\\b", "3D:Katalog", "a*", "a?", "a\"b", "a<b", "a>b", "a|b", "a\u{7}b", ".", "..", "Katalog.", "Katalog ", "CON", "nul.txt", "Com1"] {
            let err = validate_new_catalog_dir_name(name).expect_err(name);
            assert!(err.expected, "{name} is a user mistake, not a fault");
        }
        assert_eq!(
            validate_new_catalog_dir_name("3D:Katalog").unwrap_err(),
            "Dieses Zeichen ist in Ordnernamen nicht erlaubt: :"
        );
    }

    #[test]
    fn new_catalog_dir_cannot_escape_the_parent() {
        let base = unique_test_dir("catalog_dir_traversal");
        let parent = base.join("eltern");
        std::fs::create_dir_all(&parent).unwrap();
        let approved = ApprovedTargets::default();
        approved.approve(&parent);

        for name in ["..", "../raus", "/tmp/raus", "..\\raus"] {
            assert!(create_catalog_dir_at(&approved, &[], &parent, name).is_err(), "{name} must be rejected");
        }
        assert!(!base.join("raus").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn new_catalog_dir_requires_a_parent_picked_in_the_app() {
        let parent = unique_test_dir("catalog_dir_unapproved");
        let approved = ApprovedTargets::default();

        let err = create_catalog_dir_at(&approved, &[], &parent, "3D-Katalog").unwrap_err();
        assert!(!err.expected, "an unapproved parent means a misbehaving frontend");
        assert!(!parent.join("3D-Katalog").exists());
        let _ = std::fs::remove_dir_all(&parent);
    }

    #[test]
    fn new_catalog_dir_is_created_and_approved() {
        let parent = unique_test_dir("catalog_dir_create");
        let approved = ApprovedTargets::default();
        approved.approve(&parent);

        let created = create_catalog_dir_at(&approved, &[], &parent, "3D-Katalog").unwrap();
        assert_eq!(created, parent.join("3D-Katalog"));
        assert!(created.is_dir());
        assert!(approved.contains(&created), "the new folder counts as picked in the app");
        let _ = std::fs::remove_dir_all(&parent);
    }

    #[test]
    fn existing_catalog_dir_is_reused_without_error() {
        let parent = unique_test_dir("catalog_dir_reuse");
        std::fs::create_dir(parent.join("3D-Katalog")).unwrap();
        std::fs::write(parent.join("3D-Katalog").join("model.3mf"), b"x").unwrap();
        let approved = ApprovedTargets::default();
        approved.approve(&parent);

        let preview = new_catalog_dir_target(&approved, &parent, "3D-Katalog").unwrap();
        assert!(preview.is_dir());
        let reused = create_catalog_dir_at(&approved, &[], &parent, "3D-Katalog").unwrap();
        assert_eq!(reused, parent.join("3D-Katalog"));
        assert!(reused.join("model.3mf").exists(), "existing content stays untouched");
        let _ = std::fs::remove_dir_all(&parent);
    }

    #[test]
    fn a_file_with_the_catalog_dir_name_is_an_expected_error() {
        let parent = unique_test_dir("catalog_dir_file");
        std::fs::write(parent.join("3D-Katalog"), b"x").unwrap();
        let approved = ApprovedTargets::default();
        approved.approve(&parent);

        let err = create_catalog_dir_at(&approved, &[], &parent, "3D-Katalog").unwrap_err();
        assert!(err.expected);
        let _ = std::fs::remove_dir_all(&parent);
    }

    #[test]
    fn new_catalog_dir_inside_a_sensitive_dir_is_rejected() {
        let parent = unique_test_dir("catalog_dir_sensitive");
        let approved = ApprovedTargets::default();
        approved.approve(&parent);

        assert!(create_catalog_dir_at(&approved, std::slice::from_ref(&parent), &parent, "3D-Katalog").is_err());
        assert!(!parent.join("3D-Katalog").exists());
        let _ = std::fs::remove_dir_all(&parent);
    }
}
