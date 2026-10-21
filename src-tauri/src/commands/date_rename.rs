use super::*;

use chrono::{DateTime, Local};

/// Upper bound per call so one request cannot hold the catalog gate and the
/// database lock for an unbounded time.
const MAX_ITEMS: usize = 5000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DateSource {
    /// Modification time of the file on disk.
    Modified,
    /// `imported_at` of the catalog entry.
    Imported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewStatus {
    Rename,
    SkipHasPrefix,
    SkipConflict,
    SkipMissing,
    SkipTrash,
    SkipTooLong,
    /// File lies outside the catalog folders (adopted in place).
    SkipExternal,
    /// The chosen date source has no usable value for this file.
    SkipNoDate,
    /// New name or location is not allowed (forbidden characters, protected folder).
    SkipInvalid,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DateRenamePreview {
    pub id: String,
    pub old_name: String,
    pub new_name: String,
    pub status: PreviewStatus,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DateRenameItem {
    pub id: String,
    pub new_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyStatus {
    Renamed,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DateRenameResult {
    pub id: String,
    pub status: ApplyStatus,
    /// Short code for the checks done here ("missing", "external", "trash",
    /// "hasPrefix", "nameRule", "targetTaken"), otherwise the backend message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

fn disk_name(file: &FileRecord) -> String {
    Path::new(&file.path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| file.name.clone())
}

fn source_time(file: &FileRecord, source: DateSource) -> Option<DateTime<Local>> {
    match source {
        DateSource::Modified => std::fs::metadata(&file.path)
            .and_then(|m| m.modified())
            .ok()
            .map(DateTime::<Local>::from),
        // Stored as UTC RFC 3339; the prefix is shown in the local zone like at import.
        DateSource::Imported => DateTime::parse_from_rfc3339(&file.imported_at)
            .ok()
            .map(|t| t.with_timezone(&Local)),
    }
}

/// True when the target is taken on disk or by another catalog entry. Catalog
/// entries whose file is missing still own their path.
fn target_taken(conn: &Connection, id: i64, old: &Path, new: &Path) -> CmdResult<bool> {
    if rename::target_collides(old, new).map_err(|e| e.to_string())? {
        return Ok(true);
    }
    let in_catalog: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM files WHERE path = ?1 AND id != ?2)",
            rusqlite::params![new.to_string_lossy(), id],
            |row| row.get(0),
        )
        .map_err(|e| format!("database: {e}"))?;
    Ok(in_catalog)
}

/// Checks that hold for preview and apply alike. `Err` carries the skip reason.
fn eligibility(
    file: &FileRecord,
    base: Option<&Path>,
) -> Result<(), PreviewStatus> {
    if file.deleted_at.is_some() {
        return Err(PreviewStatus::SkipTrash);
    }
    if std::fs::symlink_metadata(&file.path).is_err() {
        return Err(PreviewStatus::SkipMissing);
    }
    if !import_rename::is_internal(Path::new(&file.path), base) {
        return Err(PreviewStatus::SkipExternal);
    }
    if import_rename::has_date_prefix(&disk_name(file)) {
        return Err(PreviewStatus::SkipHasPrefix);
    }
    Ok(())
}

fn plan_file(
    conn: &Connection,
    file: &FileRecord,
    source: DateSource,
    base: Option<&Path>,
    sensitive: &[PathBuf],
    claimed: &mut HashSet<String>,
) -> CmdResult<(String, PreviewStatus)> {
    let old_name = disk_name(file);
    let skip = |status| Ok((old_name.clone(), status));
    if let Err(status) = eligibility(file, base) {
        return skip(status);
    }
    let Some(time) = source_time(file, source) else {
        return skip(PreviewStatus::SkipNoDate);
    };
    let new_name = format!("{}{old_name}", import_rename::date_prefix(time));
    // A forbidden character in the old name must not be silently rewritten here.
    if new_name.len() > folders::MAX_DIR_NAME_BYTES {
        return skip(PreviewStatus::SkipTooLong);
    }
    if folders::validate_name(&new_name, true, "Dateiname").is_err() {
        return skip(PreviewStatus::SkipInvalid);
    }
    let old_path = PathBuf::from(&file.path);
    let new_path = old_path.with_file_name(&new_name);
    if reject_if_sensitive_path(&old_path, sensitive).is_err()
        || reject_if_sensitive_path(&new_path, sensitive).is_err()
    {
        return skip(PreviewStatus::SkipInvalid);
    }
    // Case-insensitive file systems treat these spellings as one entry; the first
    // file of the selection keeps the name.
    let key = rename::normalized_lowercase(&new_path.to_string_lossy());
    if target_taken(conn, file.id, &old_path, &new_path)? || !claimed.insert(key) {
        return skip(PreviewStatus::SkipConflict);
    }
    Ok((new_name, PreviewStatus::Rename))
}

fn preview_with_conn(
    conn: &Connection,
    file_ids: &[String],
    source: DateSource,
    sensitive: &[PathBuf],
) -> CmdResult<Vec<DateRenamePreview>> {
    if file_ids.len() > MAX_ITEMS {
        return Err(CmdError::expected(format!(
            "Zu viele Dateien auf einmal (höchstens {MAX_ITEMS})"
        )));
    }
    let base = import_rename::catalog_base(conn)?;
    let mut claimed = HashSet::new();
    let mut out = Vec::with_capacity(file_ids.len());
    for raw in file_ids {
        let file = raw
            .parse::<i64>()
            .ok()
            .map(|id| db::get_file(conn, id).map_err(|e| format!("database: {e}")))
            .transpose()?
            .flatten();
        let (old_name, new_name, status) = match file {
            None => (String::new(), String::new(), PreviewStatus::SkipMissing),
            Some(file) => {
                let (new_name, status) =
                    plan_file(conn, &file, source, base.as_deref(), sensitive, &mut claimed)?;
                (disk_name(&file), new_name, status)
            }
        };
        out.push(DateRenamePreview {
            id: raw.clone(),
            old_name,
            new_name,
            status,
        });
    }
    Ok(out)
}

/// A name the preview could have produced: a real `JJJJ-MM-TT_HH-MM_` prefix
/// followed by the current name, nothing else.
fn follows_rule(new_name: &str, old_name: &str) -> bool {
    let Some((prefix, rest)) = new_name
        .get(..import_rename::PREFIX_LEN)
        .zip(new_name.get(import_rename::PREFIX_LEN..))
    else {
        return false;
    };
    rest == old_name
        && import_rename::has_date_prefix(prefix)
        && chrono::NaiveDate::parse_from_str(&prefix[..10], "%Y-%m-%d").is_ok()
        && chrono::NaiveTime::parse_from_str(&prefix[11..16], "%H-%M").is_ok()
}

fn apply_one(
    conn: &Connection,
    item: &DateRenameItem,
    base: Option<&Path>,
    sensitive: &[PathBuf],
) -> Result<(), String> {
    let id: i64 = item.id.parse().map_err(|_| "missing".to_string())?;
    let file = db::get_file(conn, id)
        .map_err(|e| format!("database: {e}"))?
        .ok_or_else(|| "missing".to_string())?;
    eligibility(&file, base).map_err(|status| {
        match status {
            PreviewStatus::SkipTrash => "trash",
            PreviewStatus::SkipMissing => "missing",
            PreviewStatus::SkipExternal => "external",
            _ => "hasPrefix",
        }
        .to_string()
    })?;
    // The UI may only send names the preview produced, never free input.
    if !follows_rule(&item.new_name, &disk_name(&file)) {
        return Err("nameRule".into());
    }
    let old_path = PathBuf::from(&file.path);
    let new_path = old_path.with_file_name(&item.new_name);
    if target_taken(conn, id, &old_path, &new_path).map_err(|e| e.message)? {
        return Err("targetTaken".into());
    }
    files::rename_file_with_conn(conn, id, item.new_name.clone(), sensitive).map_err(|e| e.message)
}

fn apply_with_conn(
    conn: &Connection,
    items: Vec<DateRenameItem>,
    sensitive: &[PathBuf],
) -> CmdResult<Vec<DateRenameResult>> {
    if items.len() > MAX_ITEMS {
        return Err(CmdError::expected(format!(
            "Zu viele Dateien auf einmal (höchstens {MAX_ITEMS})"
        )));
    }
    let base = import_rename::catalog_base(conn)?;
    Ok(items
        .into_iter()
        .map(|item| {
            // One failing file must not stop the others.
            let outcome = apply_one(conn, &item, base.as_deref(), sensitive);
            DateRenameResult {
                id: item.id,
                status: if outcome.is_ok() {
                    ApplyStatus::Renamed
                } else {
                    ApplyStatus::Failed
                },
                reason: outcome.err(),
            }
        })
        .collect())
}

/// Plans `<date>_<name>` renames without touching disk or database.
#[tauri::command]
pub fn preview_date_rename(
    state: State<AppState>,
    file_ids: Vec<String>,
    date_source: DateSource,
) -> CmdResult<Vec<DateRenamePreview>> {
    let conn = lock_db(&state)?;
    preview_with_conn(&conn, &file_ids, date_source, &state.sensitive_dirs)
}

/// Renames the confirmed files. Every entry is checked again, because disk and
/// catalog may have changed since the preview.
#[tauri::command]
pub fn apply_date_rename(
    state: State<AppState>,
    items: Vec<DateRenameItem>,
) -> CmdResult<Vec<DateRenameResult>> {
    let _catalog_share = state.import_jobs.gate.exclusive()?;
    let conn = lock_db(&state)?;
    apply_with_conn(&conn, items, &state.sensitive_dirs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn setup(name: &str) -> (Connection, PathBuf) {
        let dir = unique_test_dir(name).canonicalize().unwrap();
        let conn = crate::db::connect_in_memory().expect("connect");
        db::printer_link::set_setting(
            &conn,
            folders::SETTING_CATALOG_BASE_DIR,
            &dir.to_string_lossy(),
        )
        .unwrap();
        (conn, dir)
    }

    /// Fixed modification time so expected names do not depend on the clock.
    fn modified() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_760_000_000)
    }

    fn modified_prefix() -> String {
        import_rename::date_prefix(DateTime::<Local>::from(modified()))
    }

    fn add(conn: &Connection, dir: &Path, name: &str) -> String {
        let path = dir.join(name);
        let file = std::fs::File::create(&path).unwrap();
        file.set_modified(modified()).unwrap();
        db::test_insert_minimal_file(conn, &path.to_string_lossy(), None)
            .unwrap()
            .to_string()
    }

    fn status_of(preview: &[DateRenamePreview], id: &str) -> PreviewStatus {
        preview.iter().find(|p| p.id == id).unwrap().status
    }

    fn run_preview(conn: &Connection, ids: &[&String]) -> Vec<DateRenamePreview> {
        let ids: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
        preview_with_conn(conn, &ids, DateSource::Modified, &[]).unwrap()
    }

    #[test]
    fn preview_reports_every_status() {
        let (conn, dir) = setup("date_rename_status");
        let prefix = modified_prefix();
        let plain = add(&conn, &dir, "plain.3mf");
        let prefixed = add(&conn, &dir, "2020-01-02_03-04_old.3mf");
        let taken = add(&conn, &dir, "taken.3mf");
        // Untracked file already using the target name.
        std::fs::write(dir.join(format!("{prefix}taken.3mf")), b"x").unwrap();
        let upper = add(&conn, &dir, "Pair.3mf");
        let lower = add(&conn, &dir, "pair.3mf");
        let missing = add(&conn, &dir, "gone.3mf");
        std::fs::remove_file(dir.join("gone.3mf")).unwrap();
        let trashed = add(&conn, &dir, "trashed.3mf");
        conn.execute(
            "UPDATE files SET deleted_at = '2026-01-01T00:00:00Z' WHERE id = ?1",
            [&trashed],
        )
        .unwrap();
        let long = add(&conn, &dir, &format!("{}.3mf", "a".repeat(246)));
        let outside = unique_test_dir("date_rename_outside").canonicalize().unwrap();
        let external = add(&conn, &outside, "external.3mf");
        // A catalog entry owning the target path although its file is gone.
        let _ghost = add(&conn, &dir, &format!("{prefix}ghost.3mf"));
        std::fs::remove_file(dir.join(format!("{prefix}ghost.3mf"))).unwrap();
        let ghost_owner = add(&conn, &dir, "ghost.3mf");

        let ids = [
            &plain, &prefixed, &taken, &upper, &lower, &missing, &trashed, &long, &external,
            &ghost_owner,
        ];
        let preview = run_preview(&conn, &ids);
        assert_eq!(status_of(&preview, &plain), PreviewStatus::Rename);
        assert_eq!(
            preview.iter().find(|p| p.id == plain).unwrap().new_name,
            format!("{prefix}plain.3mf")
        );
        assert_eq!(status_of(&preview, &prefixed), PreviewStatus::SkipHasPrefix);
        assert_eq!(status_of(&preview, &taken), PreviewStatus::SkipConflict);
        assert_eq!(status_of(&preview, &upper), PreviewStatus::Rename);
        assert_eq!(status_of(&preview, &lower), PreviewStatus::SkipConflict);
        assert_eq!(status_of(&preview, &missing), PreviewStatus::SkipMissing);
        assert_eq!(status_of(&preview, &trashed), PreviewStatus::SkipTrash);
        assert_eq!(status_of(&preview, &long), PreviewStatus::SkipTooLong);
        assert_eq!(status_of(&preview, &external), PreviewStatus::SkipExternal);
        assert_eq!(status_of(&preview, &ghost_owner), PreviewStatus::SkipConflict);
        let unknown = run_preview(&conn, &[&"99999".to_string()]);
        assert_eq!(unknown[0].status, PreviewStatus::SkipMissing);
        // Planning writes nothing.
        assert!(dir.join("plain.3mf").exists());
        assert_eq!(
            db::get_file(&conn, plain.parse().unwrap()).unwrap().unwrap().name,
            "plain.3mf"
        );
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(outside);
    }

    #[test]
    fn preview_uses_the_selected_date_source() {
        let (conn, dir) = setup("date_rename_source");
        let id = add(&conn, &dir, "model.3mf");
        conn.execute(
            "UPDATE files SET imported_at = '2025-03-04T05:06:07Z' WHERE id = ?1",
            [&id],
        )
        .unwrap();
        let ids = [id.clone()];
        let modified_name = &preview_with_conn(&conn, &ids, DateSource::Modified, &[]).unwrap()[0];
        let imported_name = &preview_with_conn(&conn, &ids, DateSource::Imported, &[]).unwrap()[0];
        assert_eq!(modified_name.new_name, format!("{}model.3mf", modified_prefix()));
        let imported = DateTime::parse_from_rfc3339("2025-03-04T05:06:07Z")
            .unwrap()
            .with_timezone(&Local);
        assert_eq!(
            imported_name.new_name,
            format!("{}model.3mf", import_rename::date_prefix(imported))
        );
        assert_ne!(modified_name.new_name, imported_name.new_name);
        conn.execute("UPDATE files SET imported_at = 'garbage' WHERE id = ?1", [&id])
            .unwrap();
        let broken = &preview_with_conn(&conn, &ids, DateSource::Imported, &[]).unwrap()[0];
        assert_eq!(broken.status, PreviewStatus::SkipNoDate);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn apply_renames_disk_and_catalog() {
        let (conn, dir) = setup("date_rename_apply");
        let prefix = modified_prefix();
        let id = add(&conn, &dir, "model.3mf");
        let new_name = format!("{prefix}model.3mf");
        let results = apply_with_conn(
            &conn,
            vec![DateRenameItem { id: id.clone(), new_name: new_name.clone() }],
            &[],
        )
        .unwrap();
        assert_eq!(results[0].status, ApplyStatus::Renamed, "{:?}", results[0].reason);
        assert!(dir.join(&new_name).exists());
        assert!(!dir.join("model.3mf").exists());
        let row = db::get_file(&conn, id.parse().unwrap()).unwrap().unwrap();
        assert_eq!(row.name, new_name);
        assert_eq!(row.path, dir.join(&new_name).to_string_lossy());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn apply_rejects_names_the_preview_would_not_produce() {
        let (conn, dir) = setup("date_rename_rule");
        let id = add(&conn, &dir, "model.3mf");
        let bad = [
            "free.3mf".to_string(),
            "2026-10-12_14-41_other.3mf".to_string(),
            "2026-13-45_99-99_model.3mf".to_string(),
            "2026-10-12_14-41_model.3mf.bak".to_string(),
            "../2026-10-12_14-41_model.3mf".to_string(),
            "é".to_string(),
        ];
        for new_name in bad {
            let results = apply_with_conn(
                &conn,
                vec![DateRenameItem { id: id.clone(), new_name: new_name.clone() }],
                &[],
            )
            .unwrap();
            assert_eq!(results[0].status, ApplyStatus::Failed, "{new_name}");
            assert_eq!(results[0].reason.as_deref(), Some("nameRule"), "{new_name}");
        }
        assert!(dir.join("model.3mf").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn apply_rechecks_target_and_state_and_continues_after_failures() {
        let (conn, dir) = setup("date_rename_partial");
        let prefix = modified_prefix();
        let good = add(&conn, &dir, "good.3mf");
        let taken = add(&conn, &dir, "taken.3mf");
        // Appears between preview and apply.
        std::fs::write(dir.join(format!("{prefix}taken.3mf")), b"x").unwrap();
        let gone = add(&conn, &dir, "gone.3mf");
        std::fs::remove_file(dir.join("gone.3mf")).unwrap();
        let outside = unique_test_dir("date_rename_apply_outside").canonicalize().unwrap();
        let external = add(&conn, &outside, "external.3mf");
        let item = |id: &String, name: &str| DateRenameItem {
            id: id.clone(),
            new_name: format!("{prefix}{name}"),
        };
        let results = apply_with_conn(
            &conn,
            vec![
                item(&taken, "taken.3mf"),
                item(&gone, "gone.3mf"),
                item(&external, "external.3mf"),
                item(&good, "good.3mf"),
            ],
            &[],
        )
        .unwrap();
        let reasons: Vec<_> = results.iter().map(|r| r.reason.as_deref()).collect();
        assert_eq!(
            reasons,
            [Some("targetTaken"), Some("missing"), Some("external"), None]
        );
        assert_eq!(results[3].status, ApplyStatus::Renamed);
        assert_eq!(
            std::fs::read(dir.join(format!("{prefix}taken.3mf"))).unwrap(),
            b"x",
            "an existing file must never be overwritten"
        );
        assert!(dir.join("taken.3mf").exists());
        assert!(outside.join("external.3mf").exists());
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(outside);
    }

    #[test]
    fn apply_rolls_back_the_disk_when_the_catalog_update_fails() {
        let (conn, dir) = setup("date_rename_rollback");
        let prefix = modified_prefix();
        let id = add(&conn, &dir, "model.3mf");
        conn.execute_batch(&format!(
            "CREATE TRIGGER block_rename BEFORE UPDATE ON files
             WHEN NEW.id = {id} BEGIN SELECT RAISE(ABORT, 'simulated'); END;"
        ))
        .unwrap();
        let results = apply_with_conn(
            &conn,
            vec![DateRenameItem { id, new_name: format!("{prefix}model.3mf") }],
            &[],
        )
        .unwrap();
        assert_eq!(results[0].status, ApplyStatus::Failed);
        assert!(dir.join("model.3mf").exists());
        assert!(!dir.join(format!("{prefix}model.3mf")).exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn apply_refuses_protected_folders_and_oversized_requests() {
        let (conn, dir) = setup("date_rename_protected");
        let prefix = modified_prefix();
        let id = add(&conn, &dir, "model.3mf");
        let results = apply_with_conn(
            &conn,
            vec![DateRenameItem { id: id.clone(), new_name: format!("{prefix}model.3mf") }],
            std::slice::from_ref(&dir),
        )
        .unwrap();
        assert_eq!(results[0].status, ApplyStatus::Failed);
        assert!(dir.join("model.3mf").exists());
        let many: Vec<DateRenameItem> = (0..=MAX_ITEMS)
            .map(|n| DateRenameItem { id: n.to_string(), new_name: String::new() })
            .collect();
        assert!(apply_with_conn(&conn, many, &[]).is_err());
        let ids: Vec<String> = (0..=MAX_ITEMS).map(|n| n.to_string()).collect();
        assert!(preview_with_conn(&conn, &ids, DateSource::Modified, &[]).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn running_it_twice_skips_everything_the_second_time() {
        let (conn, dir) = setup("date_rename_idempotent");
        let id = add(&conn, &dir, "model.3mf");
        let first = run_preview(&conn, &[&id]);
        assert_eq!(first[0].status, PreviewStatus::Rename);
        let results = apply_with_conn(
            &conn,
            vec![DateRenameItem { id: id.clone(), new_name: first[0].new_name.clone() }],
            &[],
        )
        .unwrap();
        assert_eq!(results[0].status, ApplyStatus::Renamed);
        let second = run_preview(&conn, &[&id]);
        assert_eq!(second[0].status, PreviewStatus::SkipHasPrefix);
        assert_eq!(second[0].old_name, second[0].new_name);
        // Replaying the old request must not stack a second prefix.
        let replay = apply_with_conn(
            &conn,
            vec![DateRenameItem { id, new_name: first[0].new_name.clone() }],
            &[],
        )
        .unwrap();
        assert_eq!(replay[0].reason.as_deref(), Some("hasPrefix"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
