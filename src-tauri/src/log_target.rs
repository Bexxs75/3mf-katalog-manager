//! Decides where the log plugin may write. The plugin aborts the whole start when its log
//! file cannot be created, which would end the app before the startup error dialog exists,
//! so the file target is only requested when both folder and log file are usable.

use std::path::{Path, PathBuf};

use tauri_plugin_log::TargetKind;

/// Mirrors how Tauri resolves its log folder (`PathResolver::app_log_dir`), which needs a
/// running app and therefore cannot be asked before the plugins are built.
pub(crate) fn log_dir(identifier: &str) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    let dir = dirs::home_dir().map(|home| home.join("Library/Logs").join(identifier));
    #[cfg(not(target_os = "macos"))]
    let dir = dirs::data_local_dir().map(|base| base.join(identifier).join("logs"));
    dir
}

/// True when the folder exists or can be created, and a file can be created in it.
pub(crate) fn log_dir_usable(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_PROBE: AtomicU64 = AtomicU64::new(0);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let probe = dir.join(format!(".write-probe-{}-{timestamp}-{}", std::process::id(), NEXT_PROBE.fetch_add(1, Ordering::Relaxed)));
    create_probe(&probe)
}

fn create_probe(path: &Path) -> bool {
    // Exclusive creation never truncates a pre-existing file or follows a symlink.
    let Ok(file) = std::fs::OpenOptions::new().write(true).create_new(true).open(path) else {
        return false;
    };
    drop(file);
    let _ = std::fs::remove_file(path);
    true
}

fn log_file_usable(dir: &Path, app_name: &str) -> bool {
    if !log_dir_usable(dir) {
        return false;
    }
    // Match the plugin's default LogDir name and append mode, without creating
    // or truncating the log. The plugin owns creation and rotation.
    let path = dir.join(app_name).with_extension("log");
    match std::fs::symlink_metadata(&path) {
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
        Ok(_) => std::fs::metadata(&path).is_ok_and(|metadata| metadata.is_file())
            && std::fs::OpenOptions::new().append(true).open(&path).is_ok(),
    }
}

pub(crate) fn log_targets(file_log_usable: bool) -> Vec<TargetKind> {
    let mut targets = Vec::new();
    if file_log_usable {
        targets.push(TargetKind::LogDir { file_name: None });
    }
    targets.push(TargetKind::Stderr);
    targets
}

/// Targets for the log plugin; falls back to stderr only when the log folder or file is unusable.
pub(crate) fn resolve_log_targets(identifier: &str, app_name: &str) -> Vec<TargetKind> {
    let usable = log_dir(identifier).is_some_and(|dir| {
        let ok = log_file_usable(&dir, app_name);
        if !ok {
            eprintln!("log folder {dir:?} is not usable; logging to stderr only");
        }
        ok
    });
    log_targets(usable)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_file_target(targets: &[TargetKind]) -> bool {
        targets.iter().any(|t| matches!(t, TargetKind::LogDir { .. }))
    }

    #[test]
    fn old_probe_file_is_preserved() {
        let dir = std::env::temp_dir().join(format!("mfk-old-probe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let probe = dir.join(".write-probe");
        std::fs::write(&probe, b"keep me").unwrap();
        assert!(log_dir_usable(&dir));
        assert_eq!(std::fs::read(&probe).unwrap(), b"keep me");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn old_probe_symlink_and_target_are_preserved() {
        let dir = std::env::temp_dir().join(format!("mfk-symlink-probe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("valuable");
        std::fs::write(&target, b"keep me").unwrap();
        let probe = dir.join(".write-probe");
        std::os::unix::fs::symlink(&target, &probe).unwrap();
        assert!(log_dir_usable(&dir));
        assert_eq!(std::fs::read(&target).unwrap(), b"keep me");
        assert!(probe.is_symlink());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn colliding_probe_is_never_overwritten_or_removed() {
        let path = std::env::temp_dir().join(format!("mfk-probe-collision-{}", std::process::id()));
        std::fs::write(&path, b"keep me").unwrap();
        assert!(!create_probe(&path));
        assert_eq!(std::fs::read(&path).unwrap(), b"keep me");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn existing_log_is_checked_without_changing_its_contents() {
        let dir = std::env::temp_dir().join(format!("mfk-existing-log-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("catalog.log");
        std::fs::write(&path, b"previous log").unwrap();
        assert!(has_file_target(&log_targets(log_file_usable(&dir, "catalog"))));
        assert_eq!(std::fs::read(&path).unwrap(), b"previous log");
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(!has_file_target(&log_targets(log_file_usable(&dir, "catalog"))));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn read_only_log_leaves_only_stderr() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("mfk-readonly-log-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("catalog.log");
        std::fs::write(&path, b"previous log").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
        // Elevated users may legitimately append despite the read-only mode.
        let can_append = std::fs::OpenOptions::new().append(true).open(&path).is_ok();
        assert_eq!(has_file_target(&log_targets(log_file_usable(&dir, "catalog"))), can_append);
        assert_eq!(std::fs::read(&path).unwrap(), b"previous log");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn usable_folder_keeps_the_file_target_and_stderr() {
        let targets = log_targets(true);
        assert!(has_file_target(&targets));
        assert!(targets.iter().any(|t| matches!(t, TargetKind::Stderr)));
    }

    #[test]
    fn unusable_folder_leaves_only_stderr() {
        let targets = log_targets(false);
        assert!(!has_file_target(&targets));
        assert_eq!(targets.len(), 1);
        assert!(matches!(targets[0], TargetKind::Stderr));
    }

    #[test]
    fn missing_folder_is_created_and_leaves_no_probe_behind() {
        let base = std::env::temp_dir().join(format!("mfk-logdir-ok-{}", std::process::id()));
        let dir = base.join("a").join("logs");
        assert!(log_dir_usable(&dir));
        assert!(dir.is_dir());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_file_in_the_way_makes_the_folder_unusable() {
        let base = std::env::temp_dir().join(format!("mfk-logdir-file-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let blocker = base.join("blocker");
        std::fs::write(&blocker, b"x").unwrap();
        assert!(!log_dir_usable(&blocker.join("logs")));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn read_only_parent_makes_the_folder_unusable() {
        use std::os::unix::fs::PermissionsExt;
        let base = std::env::temp_dir().join(format!("mfk-logdir-ro-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o555)).unwrap();
        // root ignores permission bits, so the folder can legitimately be created then.
        let running_as_root = std::fs::create_dir(base.join("probe-root")).is_ok();
        let usable = log_dir_usable(&base.join("logs"));
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o755)).unwrap();
        let _ = std::fs::remove_dir_all(&base);
        assert_eq!(usable, running_as_root);
    }
}
