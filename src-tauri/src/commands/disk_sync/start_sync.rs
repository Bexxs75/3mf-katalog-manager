//! Sync with the disk at app start: the same scan as the manual sync, optionally followed
//! by taking over new files and detected renames. Nothing is ever removed and copies of
//! present models are never added here; those stay a decision in the sync dialog.
use super::*;
use std::sync::Arc;

pub(super) const SETTING_ON_START: &str = "sync_on_start";
pub(super) const SETTING_AUTO_ADD: &str = "sync_auto_add";

pub(super) fn flag(conn: &Connection, key: &str) -> CmdResult<bool> {
    Ok(db::printer_link::get_setting(conn, key)
        .map_err(|e| format!("database: {e}"))?
        .as_deref()
        == Some("1"))
}
pub(super) fn set_flag(conn: &Connection, key: &str, enabled: bool) -> CmdResult<()> {
    db::printer_link::set_setting(conn, key, if enabled { "1" } else { "0" })
        .map_err(|e| e.to_string().into())
}

#[tauri::command]
pub fn get_sync_on_start(state: State<AppState>) -> CmdResult<bool> {
    let conn = lock_db(&state)?;
    flag(&conn, SETTING_ON_START)
}
#[tauri::command]
pub fn set_sync_on_start(state: State<AppState>, enabled: bool) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    set_flag(&conn, SETTING_ON_START, enabled)
}
#[tauri::command]
pub fn get_sync_auto_add(state: State<AppState>) -> CmdResult<bool> {
    let conn = lock_db(&state)?;
    flag(&conn, SETTING_AUTO_ADD)
}
#[tauri::command]
pub fn set_sync_auto_add(state: State<AppState>, enabled: bool) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    set_flag(&conn, SETTING_AUTO_ADD, enabled)
}

/// What the automatic take-over did.
#[derive(Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StartSyncApplied {
    pub(super) added: usize,
    pub(super) renamed: usize,
    pub(super) failed: usize,
}
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSyncResult {
    /// False when the setting is off or the catalog was busy.
    pub(super) ran: bool,
    pub(super) busy: bool,
    pub(super) new_files: usize,
    pub(super) renamed: usize,
    pub(super) missing_files: usize,
    pub(super) missing_folders: usize,
    pub(super) duplicates: usize,
    pub(super) unreachable_roots: usize,
    /// Set only when new files and renames were taken over automatically.
    pub(super) applied: Option<StartSyncApplied>,
}

/// Paths of new files that repeat the content of an earlier new file (by path order).
/// The scan only compares new files with catalog entries, so two identical files dropped
/// in together would both be taken over; only the first of them is. Files are grouped by
/// size first and only groups of more than one are hashed. A file that cannot be read is
/// never held back. Files above the import size limit are left out: the scan does not
/// hash them either, and hashing them here would keep the catalog gate busy for nothing.
fn repeated_new_files(scan: &DiskScan) -> HashSet<String> {
    repeated_new_files_within(scan, crate::safe_file::MAX_MODEL_FILE_BYTES)
}

pub(super) fn repeated_new_files_within(scan: &DiskScan, max_bytes: u64) -> HashSet<String> {
    let mut by_size: HashMap<u64, Vec<(&str, PathBuf)>> = HashMap::new();
    for entry in &scan.new_files {
        let Some(disk_path) = scan.roots.iter().find(|r| r.stored == Path::new(&entry.root)).and_then(|r| r.resolve(Path::new(&entry.path)))
        else {
            continue;
        };
        if let Ok(meta) = std::fs::symlink_metadata(&disk_path) {
            if meta.is_file() && meta.len() <= max_bytes {
                by_size.entry(meta.len()).or_default().push((entry.path.as_str(), disk_path));
            }
        }
    }
    let mut held_back = HashSet::new();
    for mut group in by_size.into_values().filter(|group| group.len() > 1) {
        group.sort_by(|a, b| a.0.cmp(b.0));
        let mut seen = HashSet::new();
        for (path, disk_path) in group {
            if let Ok(hash) = files::compute_content_hash(&disk_path) {
                if !seen.insert(hash) {
                    held_back.insert(path.to_string());
                }
            }
        }
    }
    held_back
}

/// Picks only what may happen without asking: new files (with the folders they need) and
/// detected renames. Missing entries and copies of present models are left out on purpose,
/// and so are repeats of another new file (`held_back`).
fn auto_selection(scan: &DiskScan, held_back: &HashSet<String>, sensitive_dirs: Vec<PathBuf>) -> DiskSelection {
    let files: Vec<&str> = scan.new_files.iter().map(|e| e.path.as_str()).filter(|p| !held_back.contains(*p)).collect();
    // A folder is only created for files that are actually taken over below it.
    let folders = scan
        .new_folders
        .iter()
        .filter(|f| files.iter().any(|p| Path::new(p).starts_with(&f.path)))
        .map(|f| f.path.clone());
    DiskSelection {
        new_paths: folders.chain(files.iter().map(|p| p.to_string())).collect(),
        renames: scan
            .renamed_files
            .iter()
            .map(|r| RenameChoice { id: r.id.clone(), new_path: r.new_path.clone() })
            .collect(),
        remove_missing: false,
        sensitive_dirs,
        ..Default::default()
    }
}

pub(super) fn sync_at_start(
    conn: &mut Connection,
    gate: &Arc<CatalogGate>,
    sensitive_dirs: Vec<PathBuf>,
    read: impl FnMut(&Path) -> CmdResult<db::models::NewFile>,
) -> SyncResult<StartSyncResult> {
    if !flag(conn, SETTING_ON_START)? {
        return Ok(StartSyncResult::default());
    }
    // Auto-add only counts together with the main switch (checked above).
    let auto_add = flag(conn, SETTING_AUTO_ADD)?;
    // Never wait: a running import or another sync means "not now", not a stalled start.
    let Some(_guard) = gate.try_background_exclusive().map_err(|e| e.to_string())? else {
        return Ok(StartSyncResult { busy: true, ..Default::default() });
    };
    let scan = scan(conn)?;
    if !scan.unreachable_roots.is_empty() {
        // An unplugged drive must never look like deleted files or trigger changes.
        return Ok(StartSyncResult { ran: true, unreachable_roots: scan.unreachable_roots.len(), ..Default::default() });
    }
    let mut result = StartSyncResult {
        ran: true,
        new_files: scan.new_files.len(),
        renamed: scan.renamed_files.len(),
        missing_files: scan.missing_files.len(),
        missing_folders: scan.missing_folders.len(),
        duplicates: scan.duplicate_files.len(),
        ..Default::default()
    };
    if auto_add && (result.new_files > 0 || result.renamed > 0) {
        let held_back = repeated_new_files(&scan);
        // They stay open for the manual sync, which then shows them as copies.
        result.duplicates += held_back.len();
        let applied = apply_with_reader(conn, auto_selection(&scan, &held_back, sensitive_dirs), read)?;
        result.applied = Some(StartSyncApplied {
            added: applied.file_ids.len(),
            renamed: applied.renamed_count,
            failed: applied.failed_paths.len(),
        });
    } else if auto_add {
        result.applied = Some(StartSyncApplied::default());
    }
    Ok(result)
}

#[tauri::command]
pub async fn run_start_sync(app: tauri::AppHandle) -> SyncResult<StartSyncResult> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        sync_at_start(
            &mut connection(&state)?,
            &state.import_jobs.gate,
            state.sensitive_dirs.clone(),
            read_model_with_hash,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}
