//! Update from within the app: which endpoint to ask, whether this installation
//! can replace itself, and what to show after an update.
pub mod backup;

use serde::{Deserialize, Serialize};

pub const ENDPOINT: &str = "https://github.com/Bexxs75/3mf-katalog-manager/releases/latest/download/latest.json";
pub const ENDPOINT_STEP: &str =
    "https://github.com/Bexxs75/3mf-katalog-manager/releases/latest/download/latest-step.json";
pub const ENDPOINT_ENV: &str = "MFK_UPDATE_ENDPOINT";
/// Debug builds only: public key for updates signed with a throwaway test key.
pub const PUBKEY_ENV: &str = "MFK_UPDATE_PUBKEY";
pub const LAST_UPDATE_KEY: &str = "last_update_info";

/// A STEP build must only ever update to a STEP build and vice versa, so the
/// variant picks the endpoint. The override exists for local update tests.
pub fn endpoint(step: bool, allow_override: bool, override_value: Option<&str>) -> String {
    if allow_override {
        if let Some(v) = override_value.map(str::trim).filter(|v| !v.is_empty()) {
            return v.to_string();
        }
    }
    if step { ENDPOINT_STEP } else { ENDPOINT }.to_string()
}

pub fn release_page(version: &str) -> String {
    format!("https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v{version}")
}

/// On Linux only the AppImage can replace itself; the `APPIMAGE` variable is set
/// by the AppImage runtime. Package-manager installs update through the package manager.
pub fn can_self_install(os: &str, appimage_env: Option<&str>) -> bool {
    os != "linux" || appimage_env.is_some_and(|v| !v.is_empty())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastUpdateInfo {
    pub version: String,
    pub date: String,
    pub backup_file: String,
}

pub fn visible_last_update(stored: Option<LastUpdateInfo>, current_version: &str) -> Option<LastUpdateInfo> {
    stored.filter(|i| i.version == current_version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_follows_the_variant() {
        assert_eq!(endpoint(false, false, None), ENDPOINT);
        assert_eq!(endpoint(true, false, None), ENDPOINT_STEP);
    }

    #[test]
    fn override_only_when_allowed_and_not_empty() {
        assert_eq!(
            endpoint(false, true, Some("http://127.0.0.1:8765/latest.json")),
            "http://127.0.0.1:8765/latest.json"
        );
        assert_eq!(endpoint(false, false, Some("http://127.0.0.1:8765/latest.json")), ENDPOINT);
        assert_eq!(endpoint(true, true, Some("  ")), ENDPOINT_STEP);
    }

    #[test]
    fn release_page_points_to_the_tag() {
        assert_eq!(
            release_page("0.15.1"),
            "https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v0.15.1"
        );
    }

    #[test]
    fn self_install_needs_an_appimage_on_linux() {
        assert!(can_self_install("windows", None));
        assert!(can_self_install("macos", None));
        assert!(can_self_install("linux", Some("/home/u/App.AppImage")));
        assert!(!can_self_install("linux", None));
        assert!(!can_self_install("linux", Some("")));
    }

    #[test]
    fn last_update_is_shown_only_for_the_running_version() {
        let info = LastUpdateInfo {
            version: "0.15.1".into(),
            date: "2026-10-03".into(),
            backup_file: "catalog-vor-0.15.1.db".into(),
        };
        assert_eq!(visible_last_update(Some(info.clone()), "0.15.1"), Some(info.clone()));
        assert_eq!(visible_last_update(Some(info), "0.15.2"), None);
        assert_eq!(visible_last_update(None, "0.15.1"), None);
    }

    #[test]
    fn last_update_info_json_is_camel_case() {
        let info = LastUpdateInfo {
            version: "1".into(),
            date: "d".into(),
            backup_file: "f".into(),
        };
        assert_eq!(
            serde_json::to_string(&info).unwrap(),
            r#"{"version":"1","date":"d","backupFile":"f"}"#
        );
    }
}
