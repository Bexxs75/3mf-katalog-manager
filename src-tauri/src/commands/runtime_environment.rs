use super::{CmdError, CmdResult};
use serde::Serialize;
use std::path::Path;
use tauri::{Manager, State};

#[derive(Clone, Copy, Debug, Serialize)]
pub struct RuntimeEnvironment {
    pub container: bool,
}

#[derive(Clone, Copy)]
pub(super) enum DesktopFeature { Slicer, FileManager, Updater, ExternalAction }

impl DesktopFeature {
    fn message(self) -> &'static str {
        match self {
            Self::Slicer => "containerSlicerUnavailable",
            Self::FileManager => "containerFileManagerUnavailable",
            Self::Updater => "container",
            Self::ExternalAction => "containerExternalUnavailable",
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

const MAX_ORIGIN_BYTES: usize = 1024;

#[derive(Debug, Serialize)]
pub struct CatalogWebLink {
    pub url: Option<String>,
    pub path: String,
}

fn valid_public_origin(value: &str) -> bool {
    let Some(host) = value.strip_prefix("http://").or_else(|| value.strip_prefix("https://")) else {
        return false;
    };
    if host.len() > 253 { return false; }
    static HOST: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"\A(?:[A-Za-z0-9](?:[A-Za-z0-9.-]*[A-Za-z0-9])?|\[[0-9A-Fa-f:]+\])(?::[0-9]{1,5})?\z").unwrap()
    });
    if !HOST.is_match(host) { return false; }
    let port = if let Some(address) = host.strip_prefix('[') {
        let Some((address, suffix)) = address.split_once(']') else { return false; };
        if address.parse::<std::net::Ipv6Addr>().is_err() { return false; }
        suffix.strip_prefix(':')
    } else {
        host.split_once(':').map(|(_, port)| port)
    };
    port.is_none_or(|port| port.parse::<u16>().is_ok_and(|port| port > 0))
}

fn catalog_web_link(file_id: i64, configured: Option<&str>, remembered: Option<&[u8]>) -> CatalogWebLink {
    let origin = configured.filter(|value| valid_public_origin(value)).or_else(|| {
        remembered.filter(|bytes| bytes.len() <= MAX_ORIGIN_BYTES)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .filter(|value| valid_public_origin(value))
    });
    let path = format!("/katalog/?file={file_id}");
    CatalogWebLink { url: origin.map(|origin| format!("{origin}{path}")), path }
}

#[tauri::command]
pub fn get_catalog_web_link(file_id: i64, environment: State<'_, RuntimeEnvironment>) -> CmdResult<CatalogWebLink> {
    use std::io::Read;
    if !environment.container { return Err(CmdError::expected("catalogWebContainerOnly")); }
    let configured = std::env::var("KATALOG_PUBLIC_URL").ok();
    if configured.as_deref().is_some_and(|value| !valid_public_origin(value)) {
        log::warn!("Ignoring invalid KATALOG_PUBLIC_URL");
    }
    // Bound the read even if the file grows after it is opened.
    let remembered = std::fs::File::open("/run/3mf/public-origin").ok().and_then(|file| {
        if !file.metadata().ok()?.is_file() { return None; }
        let mut bytes = Vec::new();
        file.take((MAX_ORIGIN_BYTES + 1) as u64).read_to_end(&mut bytes).ok()?;
        Some(bytes)
    });
    Ok(catalog_web_link(file_id, configured.as_deref(), remembered.as_deref()))
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
        for feature in [DesktopFeature::Slicer, DesktopFeature::FileManager, DesktopFeature::Updater, DesktopFeature::ExternalAction] {
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
        expected(open_release_url("invalid".into(), app.state()).unwrap_err(), "containerExternalUnavailable");
        expected(open_discord_invite(app.state()).unwrap_err(), "containerExternalUnavailable");
        expected(open_data_folder(app.handle().clone(), app.state()).unwrap_err(), "containerExternalUnavailable");
        expected(open_log_folder(app.handle().clone(), app.state()).unwrap_err(), "containerExternalUnavailable");
        expected(open_bug_report_form("de".into(), true, app.state()).unwrap_err(), "containerExternalUnavailable");
        expected(super::super::security::open_url("invalid", *app.state::<RuntimeEnvironment>()).unwrap_err(), "containerExternalUnavailable");
        expected(super::super::security::open_external("/does/not/exist", *app.state::<RuntimeEnvironment>()).unwrap_err(), "containerExternalUnavailable");
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

#[cfg(test)]
mod catalog_link_tests {
    use super::*;

    #[test]
    fn origins_are_strict_and_environment_has_priority() {
        let fallback = b"https://nas.lan:3001";
        for valid in ["http://nas.lan:3000", "https://example.org", "http://127.0.0.1", "http://[::1]:3000"] {
            let link = catalog_web_link(42, Some(valid), Some(fallback));
            assert_eq!(link.path, "/katalog/?file=42");
            assert_eq!(link.url, Some(format!("{valid}/katalog/?file=42")));
        }
        for invalid in ["", " http://nas", "http://nas ", "http://nas/", "http://nas/path", "http://nas?q=1", "http://nas#x", "http://user@nas", "ftp://nas", "http://nas\n", "http://nas\t", "http://nas:0", "http://nas:65536", "http://[bad]", "http://a_b", "http://nas\\evil"] {
            assert_eq!(catalog_web_link(1, Some(invalid), None).url, None, "{invalid:?}");
            assert_eq!(catalog_web_link(1, Some(invalid), Some(fallback)).url, Some("https://nas.lan:3001/katalog/?file=1".into()));
            assert_eq!(catalog_web_link(1, None, Some(invalid.as_bytes())).url, None);
        }
        assert_eq!(catalog_web_link(1, None, None).url, None);
        assert_eq!(catalog_web_link(1, None, Some(&vec![b'a'; 1025])).url, None);
        assert_eq!(catalog_web_link(1, None, Some(&[255])).url, None);
    }

    #[test]
    fn catalog_command_rejects_desktop_before_reading_origin() {
        let app = tauri::test::mock_builder()
            .manage(RuntimeEnvironment { container: false })
            .build(tauri::test::mock_context(tauri::test::noop_assets())).unwrap();
        let error = get_catalog_web_link(42, app.state()).unwrap_err();
        assert!(error.expected);
        assert_eq!(error.message, "catalogWebContainerOnly");
    }
}
