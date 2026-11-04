//! The copies the disk sync remembers: how many there are, the list for the settings and
//! forgetting them all. The sync itself reads and writes the table in `disk_sync.rs`.
use super::*;

/// Rows sent to the webview at most; the number in the settings is the real total.
const LIST_LIMIT: usize = 500;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownCopyEntry {
    pub(super) path: String,
    /// Path below the catalog root it lies in; the whole path when no root contains it.
    pub(super) relative_path: String,
    /// Name of the cataloged model with the same content, if one is still there.
    pub(super) copy_of_name: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownCopyList {
    pub(super) total: i64,
    pub(super) items: Vec<KnownCopyEntry>,
}

/// Top-level catalog folders: the roots a remembered path is shown relative to.
fn catalog_roots(conn: &Connection) -> CmdResult<Vec<PathBuf>> {
    let folders = db::list_folders(conn).map_err(|e| e.to_string())?;
    let all: HashSet<PathBuf> = folders.iter().filter(|f| !f.path.is_empty()).map(|f| PathBuf::from(&f.path)).collect();
    Ok(all.iter().filter(|p| !p.ancestors().skip(1).any(|a| all.contains(a))).cloned().collect())
}

pub(super) fn list_known_copies_with_conn(conn: &Connection) -> CmdResult<KnownCopyList> {
    let total = db::known_copies::count_known_copies(conn).map_err(|e| e.to_string())?;
    let roots = catalog_roots(conn)?;
    let mut original = conn
        .prepare("SELECT name FROM files WHERE deleted_at IS NULL AND content_hash = ?1 ORDER BY id LIMIT 1")
        .map_err(|e| e.to_string())?;
    let mut items = Vec::new();
    for row in db::known_copies::list_known_copies_limited(conn, LIST_LIMIT).map_err(|e| e.to_string())? {
        let path = Path::new(&row.path);
        let relative_path = roots
            .iter()
            .find_map(|root| path.strip_prefix(root).ok())
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        let copy_of_name = original.query_row([&row.content_hash], |r| r.get::<_, String>(0)).optional().map_err(|e| e.to_string())?;
        items.push(KnownCopyEntry { path: row.path, relative_path, copy_of_name });
    }
    Ok(KnownCopyList { total, items })
}

#[tauri::command]
pub fn count_known_copies(state: State<AppState>) -> CmdResult<i64> {
    let conn = lock_db(&state)?;
    Ok(db::known_copies::count_known_copies(&conn).map_err(|e| e.to_string())?)
}

/// The remembered copies for the list in the settings (first 500; `total` is the real number).
#[tauri::command]
pub fn list_known_copies(state: State<AppState>) -> CmdResult<KnownCopyList> {
    let conn = lock_db(&state)?;
    list_known_copies_with_conn(&conn)
}

/// Forgets all remembered copies, so the next sync offers them again. Returns how many.
#[tauri::command]
pub fn reset_known_copies(state: State<AppState>) -> CmdResult<i64> {
    let _catalog_share = state.import_jobs.gate.exclusive()?;
    let conn = lock_db(&state)?;
    Ok(db::known_copies::clear_known_copies(&conn).map_err(|e| e.to_string())?)
}
