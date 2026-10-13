//! Decides where the log plugin may write. The plugin aborts the whole start when its log
//! file cannot be created, which would end the app before the startup error dialog exists,
//! so the file target is only requested when the folder is known to be usable.

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
    let probe = dir.join(".write-probe");
    let writable = std::fs::File::create(&probe).is_ok();
    let _ = std::fs::remove_file(&probe);
    writable
}

pub(crate) fn log_targets(file_log_usable: bool) -> Vec<TargetKind> {
    let mut targets = Vec::new();
    if file_log_usable {
        targets.push(TargetKind::LogDir { file_name: None });
    }
    targets.push(TargetKind::Stderr);
    targets
}

/// Targets for the log plugin; falls back to stderr only when the log folder is unusable.
pub(crate) fn resolve_log_targets(identifier: &str) -> Vec<TargetKind> {
    let usable = log_dir(identifier).is_some_and(|dir| {
        let ok = log_dir_usable(&dir);
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
        assert!(!dir.join(".write-probe").exists());
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
