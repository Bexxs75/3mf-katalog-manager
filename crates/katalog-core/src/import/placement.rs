use super::*;
/// Moves a file without ever overwriting an existing target. The moved file
/// keeps the permissions and modification time of the source.
///
/// First tries `std::fs::hard_link(from, to)` and then removes the source. A
/// hard link fails with `AlreadyExists` when the target exists (returned
/// unchanged), so it is an atomic no-replace "rename": `std::fs::rename`
/// replaces an existing target on POSIX and Windows, a prior `exists()` check
/// would be a TOCTOU window, and rename-no-replace only exists as three
/// platform-specific syscalls. Because the inode stays the same, permissions,
/// timestamps, owner and extended attributes/ACLs all survive.
///
/// If linking fails for any other reason (other drive, file system without
/// hard links such as FAT/exFAT or a network share, missing rights on
/// Windows), it falls back to `move_file_by_copy`. Not atomic as a whole:
/// after a crash, source and target can briefly both exist.
pub fn move_file(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    // A link to a symlink would link the symlink itself (on Linux) instead of
    // the file it points to, which is not what a copy-based move did before.
    let is_symlink = std::fs::symlink_metadata(from).map(|m| m.file_type().is_symlink()).unwrap_or(false);
    if !is_symlink {
        match std::fs::hard_link(from, to) {
            Ok(()) => {
                if let Err(remove_err) = std::fs::remove_file(from) {
                    let _ = std::fs::remove_file(to);
                    return Err(remove_err);
                }
                return Ok(());
            }
            // The target exists: never replace it, report it as is.
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Err(e),
            // Anything else (EXDEV / ERROR_NOT_SAME_DEVICE, no hard-link
            // support, missing rights): copy instead.
            Err(_) => {}
        }
    }
    move_file_by_copy(from, to)
}

/// The permission bits a copy gets. The group bits of a file with a POSIX ACL are the
/// ACL mask, not the group's rights, so copying them could open the file to a group
/// the ACL excluded; for such a file only the owner bits are taken. Otherwise group and
/// other never get more than the owner has. The umask is deliberately not applied: the
/// source mode is the upper bound, and a hard-link move keeps it exactly as well.
#[cfg(unix)]
fn copied_mode(source_mode: u32, source_has_acl: bool) -> u32 {
    let owner = source_mode & 0o700;
    if source_has_acl {
        return owner;
    }
    let group = source_mode & 0o070 & (owner >> 3);
    let other = source_mode & 0o007 & (owner >> 6);
    owner | group | other
}

/// True when the file carries an extended POSIX ACL (Linux). Other systems report
/// false and rely on the plain mode rules.
#[cfg(unix)]
fn has_extended_acl(path: &std::path::Path) -> bool {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::ffi::OsStrExt;
        let Ok(path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else { return false };
        // SAFETY: both strings are NUL-terminated; a null buffer with size 0 only asks for the size.
        let size = unsafe { libc::getxattr(path.as_ptr(), c"system.posix_acl_access".as_ptr(), std::ptr::null_mut(), 0) };
        size >= 0
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        false
    }
}

/// Copy-based move used when a hard link is not possible; split out so tests
/// can exercise it on a file system that supports hard links.
///
/// Creates the target with `create_new` (O_EXCL/CREATE_NEW, never replaces),
/// copies, applies the source's permissions and modification time, and
/// deletes the source. The target is created owner-only (0600) and widened
/// only after the content is written (see `copied_mode`), so the copy is never
/// more open than the source at any moment. setuid/setgid/sticky bits are not
/// carried over; the owner cannot be preserved. Failing to apply time or
/// permissions is not fatal, an incomplete copy is. A partially written
/// target is always removed.
pub fn move_file_by_copy(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    let mut dst = crate::safe_file::create_new_private(to)?;

    let copy_result = (|| -> std::io::Result<()> {
        let mut src = std::fs::File::open(from)?;
        let src_meta = src.metadata()?;
        std::io::copy(&mut src, &mut dst)?;
        // Best effort: the content is what matters. Time first, because on
        // Windows a read-only target could no longer be touched afterwards.
        if let Ok(modified) = src_meta.modified() {
            let _ = dst.set_modified(modified);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = copied_mode(src_meta.permissions().mode(), has_extended_acl(from));
            let _ = dst.set_permissions(std::fs::Permissions::from_mode(mode));
        }
        #[cfg(not(unix))]
        {
            let _ = dst.set_permissions(src_meta.permissions());
        }
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

#[cfg(test)]
mod move_tests {
    use super::*;
    use crate::unique_test_dir;

    #[cfg(unix)]
    fn mode_of(path: &std::path::Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
    }
    #[cfg(unix)]
    fn old_time() -> std::time::SystemTime {
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_500_000_000)
    }
    /// Creates `source` with `mode` and an old modification time.
    #[cfg(unix)]
    fn make_source(dir: &std::path::Path, mode: u32) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let from = dir.join("source.3mf");
        std::fs::write(&from, b"model data").unwrap();
        std::fs::File::options().write(true).open(&from).unwrap().set_modified(old_time()).unwrap();
        std::fs::set_permissions(&from, std::fs::Permissions::from_mode(mode)).unwrap();
        from
    }
    #[cfg(unix)]
    #[test]
    fn moved_files_keep_mode_mtime_and_inode() {
        use std::os::unix::fs::MetadataExt;
        for mode in [0o644, 0o600, 0o755] {
            let dir = unique_test_dir("move_keeps_meta");
            let from = make_source(&dir, mode);
            let to = dir.join("target.3mf");
            let inode = std::fs::metadata(&from).unwrap().ino();
            move_file(&from, &to).unwrap();
            assert_eq!(mode_of(&to), mode, "mode {mode:o} must be kept");
            assert_eq!(std::fs::metadata(&to).unwrap().modified().unwrap(), old_time());
            assert_eq!(std::fs::metadata(&to).unwrap().ino(), inode, "same-device move must keep the inode");
            assert_eq!(std::fs::read(&to).unwrap(), b"model data");
            assert!(!from.exists());
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
    #[cfg(unix)]
    #[test]
    fn copy_fallback_keeps_mode_and_mtime_without_special_bits() {
        use std::os::unix::fs::MetadataExt;
        for mode in [0o644, 0o600, 0o755, 0o4755, 0o2755] {
            let dir = unique_test_dir("move_copy_meta");
            let from = make_source(&dir, mode);
            let to = dir.join("target.3mf");
            let inode = std::fs::metadata(&from).unwrap().ino();
            move_file_by_copy(&from, &to).unwrap();
            assert_eq!(mode_of(&to), mode & 0o777, "mode {mode:o}: permissions kept, setuid/setgid dropped");
            assert_eq!(std::fs::metadata(&to).unwrap().modified().unwrap(), old_time());
            assert_ne!(std::fs::metadata(&to).unwrap().ino(), inode, "the copy path creates a new file");
            assert_eq!(std::fs::read(&to).unwrap(), b"model data");
            assert!(!from.exists());
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
    #[cfg(unix)]
    #[test]
    fn copied_mode_is_never_more_open_than_the_source() {
        assert_eq!(copied_mode(0o644, false), 0o644);
        assert_eq!(copied_mode(0o664, false), 0o664, "a group-writable file stays group-writable");
        assert_eq!(copied_mode(0o604, false), 0o604);
        assert_eq!(copied_mode(0o464, false), 0o444, "group and other never exceed the owner");
        assert_eq!(copied_mode(0o2755, false), 0o755, "special bits dropped");
        assert_eq!(copied_mode(0o660, true), 0o600, "with an ACL the group bits are only the mask");
    }
    #[cfg(unix)]
    #[test]
    fn copy_fallback_keeps_the_group_closed_for_a_source_with_an_acl() {
        use std::os::unix::ffi::OsStrExt;
        let dir = unique_test_dir("move_copy_acl");
        let from = make_source(&dir, 0o600);
        // Entries: owner rw-, named user 12345 r--, group ---, mask rw-, other ---.
        let mut acl: Vec<u8> = 2u32.to_le_bytes().to_vec();
        for (tag, perm, id) in [(1u16, 6u16, u32::MAX), (2, 4, 12345), (4, 0, u32::MAX), (0x10, 6, u32::MAX), (0x20, 0, u32::MAX)] {
            acl.extend(tag.to_le_bytes());
            acl.extend(perm.to_le_bytes());
            acl.extend(id.to_le_bytes());
        }
        let path = std::ffi::CString::new(from.as_os_str().as_bytes()).unwrap();
        let set = unsafe { libc::setxattr(path.as_ptr(), c"system.posix_acl_access".as_ptr(), acl.as_ptr().cast(), acl.len(), 0) };
        if set != 0 {
            // This file system has no ACL support; the pure mode rules are covered above.
            std::fs::remove_dir_all(dir).unwrap();
            return;
        }
        assert_eq!(mode_of(&from) & 0o070, 0o060, "precondition: the group bits show the mask");
        let to = dir.join("target.3mf");
        move_file_by_copy(&from, &to).unwrap();
        assert_eq!(mode_of(&to) & 0o077, 0, "the copy is closed to group and others");
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn failed_source_removal_after_linking_removes_the_target_again() {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::geteuid() } == 0 {
            return; // root ignores directory permissions, the removal would succeed
        }
        let dir = unique_test_dir("move_link_remove_fails");
        let src_dir = dir.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let from = src_dir.join("source.3mf");
        std::fs::write(&from, b"keep me").unwrap();
        let to = dir.join("target.3mf");
        // A read-only directory forbids unlinking the source, but not linking it.
        std::fs::set_permissions(&src_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let result = move_file(&from, &to);
        std::fs::set_permissions(&src_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(result.is_err());
        assert!(!to.exists(), "target must be removed when the source cannot be deleted");
        assert_eq!(std::fs::read(&from).unwrap(), b"keep me");
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn copy_fallback_refuses_to_overwrite_an_existing_destination() {
        let dir = unique_test_dir("move_copy_no_clobber");
        let from = dir.join("source.3mf");
        let to = dir.join("dest.3mf");
        std::fs::write(&from, b"SOURCE").unwrap();
        std::fs::write(&to, b"EXISTING").unwrap();
        let err = move_file_by_copy(&from, &to).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&to).unwrap(), b"EXISTING");
        assert!(from.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
