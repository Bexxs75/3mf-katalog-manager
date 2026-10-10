use super::{CmdError, CmdResult};
use serde::Serialize;
use std::path::Path;
use tauri::{Manager, State};

#[derive(Clone, Copy, Debug, Serialize)]
pub struct RuntimeEnvironment {
    pub container: bool,
}

#[derive(Clone, Copy)]
pub(super) enum DesktopFeature { Slicer, FileManager, Updater }

impl DesktopFeature {
    fn message(self) -> &'static str {
        match self {
            Self::Slicer => "containerSlicerUnavailable",
            Self::FileManager => "containerFileManagerUnavailable",
            Self::Updater => "container",
        }
    }
}

impl RuntimeEnvironment {
    fn from_value(value: Option<&str>) -> Self {
        Self { container: matches!(value, Some("1" | "true")) }
    }

    pub(crate) fn current() -> Self {
        Self::from_value(std::env::var("THREEMF_CONTAINER").ok().as_deref())
    }

    pub(super) fn require_desktop(self, feature: DesktopFeature) -> CmdResult<()> {
        if self.container { return Err(CmdError::expected(feature.message())); }
        Ok(())
    }

    fn network_filesystem_warning(self, home: Option<&Path>) -> bool {
        self.container && home.is_some_and(|dir| dir.join(".network-filesystem-warning").is_file())
    }
}

#[tauri::command]
pub fn get_runtime_environment(environment: State<'_, RuntimeEnvironment>) -> RuntimeEnvironment {
    *environment
}

#[tauri::command]
pub fn has_network_filesystem_warning<R: tauri::Runtime>(app: tauri::AppHandle<R>, environment: State<'_, RuntimeEnvironment>) -> bool {
    // The image places the marker in HOME, outside the per-build app identifier.
    environment.network_filesystem_warning(app.path().home_dir().ok().as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_explicit_container_values_enable_the_mode() {
        for value in [Some("1"), Some("true")] {
            assert!(RuntimeEnvironment::from_value(value).container);
        }
        for value in [None, Some(""), Some("0"), Some("false"), Some("TRUE"), Some(" true"), Some("1 ")] {
            assert!(!RuntimeEnvironment::from_value(value).container);
        }
        assert_eq!(serde_json::to_value(RuntimeEnvironment::from_value(Some("1"))).unwrap(), serde_json::json!({"container": true}));
    }

    #[test]
    fn desktop_guards_have_expected_translatable_errors() {
        for feature in [DesktopFeature::Slicer, DesktopFeature::FileManager, DesktopFeature::Updater] {
            let error = RuntimeEnvironment::from_value(Some("true")).require_desktop(feature).unwrap_err();
            assert!(error.expected);
            assert_eq!(error.message, feature.message());
            assert!(RuntimeEnvironment::from_value(None).require_desktop(feature).is_ok());
        }
    }

    #[test]
    fn marker_requires_a_container_and_a_file_in_home() {
        let dir = std::env::temp_dir().join(format!("container-marker-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&dir).unwrap();
        let container = RuntimeEnvironment::from_value(Some("1"));
        assert!(!container.network_filesystem_warning(Some(dir.as_path())));
        std::fs::write(dir.as_path().join(".network-filesystem-warning"), "").unwrap();
        assert!(container.network_filesystem_warning(Some(dir.as_path())));
        assert!(!RuntimeEnvironment::from_value(None).network_filesystem_warning(Some(dir.as_path())));
        assert!(!container.network_filesystem_warning(None));
        std::fs::remove_file(dir.as_path().join(".network-filesystem-warning")).unwrap();
        std::fs::create_dir(dir.as_path().join(".network-filesystem-warning")).unwrap();
        assert!(!container.network_filesystem_warning(Some(dir.as_path())));
        std::fs::remove_dir_all(dir).unwrap();
    }
}


#[cfg(test)]
mod command_tests {
    use super::*;
    use crate::commands::*;
    use tauri::{ipc::Channel, Manager};

    #[test]
    fn container_commands_reject_before_database_dialog_network_or_process_work() {
        let app = tauri::test::mock_builder()
            .manage(RuntimeEnvironment::from_value(Some("1")))
            .manage(AppState {
                import_jobs: ImportJobs::default(),
                // No schema or plugins: reaching desktop work must fail this test.
                db: std::sync::Mutex::new(rusqlite::Connection::open_in_memory().unwrap()),
                trash_dir: Default::default(), db_path: Default::default(), sensitive_dirs: vec![],
            })
            .manage(UpdaterState::default())
            .build(tauri::test::mock_context(tauri::test::noop_assets())).unwrap();
        assert!(get_runtime_environment(app.state()).container);
        let expected = |error: CmdError, message: &str| {
            assert!(error.expected);
            assert_eq!(error.message, message);
        };
        expected(list_registered_slicers(app.state(), app.state()).unwrap_err(), "containerSlicerUnavailable");
        expected(scan_installed_slicers(app.state(), app.state()).unwrap_err(), "containerSlicerUnavailable");
        expected(open_in_slicer(app.state(), "1".into(), "1".into(), app.state()).unwrap_err(), "containerSlicerUnavailable");
        expected(open_in_file_manager("/does/not/exist".into(), app.state()).unwrap_err(), "containerFileManagerUnavailable");
        expected(install_app_update(app.handle().clone(), app.state(), app.state(), app.state()).unwrap_err(), "container");
        tauri::async_runtime::block_on(async {
            expected(pick_and_register_slicer(app.handle().clone(), app.state(), app.state()).await.unwrap_err(), "containerSlicerUnavailable");
            expected(reveal_in_file_manager(app.state(), "1".into(), app.state()).await.unwrap_err(), "containerFileManagerUnavailable");
            expected(check_app_update(app.handle().clone(), app.state(), app.state()).await.unwrap_err(), "container");
            expected(download_app_update(app.handle().clone(), app.state(), Channel::new(|_| Ok(())), app.state(), app.state()).await.unwrap_err(), "container");
        });
    }
}
