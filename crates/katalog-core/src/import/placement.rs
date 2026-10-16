use super::*;
pub fn move_file(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    // Keep moved models private even when the source allowed broader access.
    let mut dst = crate::safe_file::create_new_private(to)?;

    let copy_result = (|| -> std::io::Result<()> {
        let mut src = std::fs::File::open(from)?;
        std::io::copy(&mut src, &mut dst)?;
        dst.sync_all()?;
        Ok(())
    })();

    if let Err(e) = copy_result {
        // A partially written target is always cleaned up.
        drop(dst);
        let _ = std::fs::remove_file(to);
        return Err(e);
    }
    drop(dst);

    if let Err(remove_err) = std::fs::remove_file(from) {
        let _ = std::fs::remove_file(to);
        return Err(remove_err);
    }
    Ok(())
}
pub fn reject_if_sensitive_path(path: &Path, sensitive_dirs: &[PathBuf]) -> CmdResult<()> {
    reject_if_sensitive_path_expanded(path, &expand_sensitive_dirs(sensitive_dirs))
}
/// Adds the canonical spelling of each protected folder (e.g. /bin -> /usr/bin).
/// Compute once when many paths are checked.
pub fn expand_sensitive_dirs(sensitive_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut expanded = Vec::with_capacity(sensitive_dirs.len());
    for dir in sensitive_dirs {
        if let Ok(canonical) = dir.canonicalize() {
            if canonical != *dir {
                expanded.push(canonical);
            }
        }
        expanded.push(dir.clone());
    }
    expanded
}
pub fn reject_if_sensitive_path_expanded(path: &Path, expanded_dirs: &[PathBuf]) -> CmdResult<()> {
    let resolved = resolve_path_for_sensitivity_check(path)?;
    for dir in expanded_dirs {
        if resolved == *dir || resolved.starts_with(dir) {
            return Err(format!(
                "Zielpfad liegt in einem geschuetzten Systemverzeichnis ({}) und wird abgelehnt",
                dir.display()
            )
            .into());
        }
    }
    Ok(())
}
pub fn resolve_path_for_sensitivity_check(path: &Path) -> CmdResult<PathBuf> {
    if let Ok(canonical) = path.canonicalize() {
        return Ok(canonical);
    }
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(
            "Pfad enthaelt \"..\"-Komponenten und wird abgelehnt".into(),
        );
    }

    let mut trailing: Vec<std::ffi::OsString> = Vec::new();
    let mut cursor = path;
    while let (Some(name), Some(parent)) = (cursor.file_name(), cursor.parent()) {
        trailing.push(name.to_os_string());
        if let Ok(canonical) = parent.canonicalize() {
            let mut resolved = canonical;
            for component in trailing.iter().rev() {
                resolved.push(component);
            }
            return Ok(resolved);
        }
        cursor = parent;
    }
    Ok(path.to_path_buf())
}
pub fn comparison_path(path: &Path) -> PathBuf {
    // Windows Path components already recognize both separator spellings.
    #[cfg(windows)]
    { PathBuf::from(path.to_string_lossy().to_lowercase()) }
    #[cfg(not(windows))]
    { path.to_path_buf() }
}pub fn move_file_to_folder_with_conn(
    conn: &Connection,
    file_id: i64,
    folder_id: Option<i64>,
    sensitive_dirs: &[PathBuf],
) -> CmdResult<()> {
    let file = db::get_file(conn, file_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "file not found".to_string())?;
    if file.deleted_at.is_some() {
        return Err(CmdError::expected("file is in trash"));
    }

    let target_dir: std::path::PathBuf = match folder_id {
        Some(fid) => {
            let folders = db::list_folders(conn).map_err(|e| e.to_string())?;
            let folder = folders
                .iter()
                .find(|f| f.id == fid)
                .ok_or_else(|| "folder not found".to_string())?;
            std::path::PathBuf::from(&folder.path)
        }
        None => std::path::Path::new(&file.path)
            .parent()
            .map(|p| p.to_path_buf())
            .ok_or_else(|| "invalid current path".to_string())?,
    };
    reject_if_sensitive_path(&target_dir, sensitive_dirs)?;

    let old_path = std::path::PathBuf::from(&file.path);
    let name = old_path.file_name().ok_or_else(|| "invalid file path".to_string())?;
    let new_path = target_dir.join(name);
    reject_if_sensitive_path(&old_path, sensitive_dirs)?;
    reject_if_sensitive_path(&new_path, sensitive_dirs)?;
    if comparison_path(&old_path) == comparison_path(&new_path) {
        // Importing the target's own contents only needs a catalog assignment.
        db::update_file_folder(conn, file_id, folder_id, &new_path.to_string_lossy())?;
        return Ok(());
    }
    move_file(&old_path, &new_path).map_err(|e| e.to_string())?;

    if let Err(db_err) =
        db::update_file_folder(conn, file_id, folder_id, &new_path.to_string_lossy())
    {
        // Undo the physical move so file system and DB don't drift apart. move_file
        // never overwrites anything.
        if let Err(rollback_err) = move_file(&new_path, &old_path) {
            return Err(format!(
                "DB-Update fehlgeschlagen ({db_err}) UND Rollback der Dateiverschiebung fehlgeschlagen ({rollback_err}) - Datei liegt jetzt unter {}, DB verweist weiter auf {}",
                new_path.display(),
                old_path.display()
            )
            .into());
        }
        return Err(db_err.to_string().into());
    }
    log::debug!(target: "datei", "verschoben: {} -> {}", old_path.display(), new_path.display());
    Ok(())
}
