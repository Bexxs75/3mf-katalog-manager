//! Update from within the app: which endpoint to ask, whether this installation
//! can replace itself, and what to show after an update.
pub mod backup;

use serde::{Deserialize, Serialize};

pub const ENDPOINT: &str = "https://github.com/Bexxs75/3mf-katalog-manager/releases/latest/download/latest.json";
pub const ENDPOINT_RC: &str =
    "https://github.com/Bexxs75/3mf-katalog-manager/releases/download/rc/latest-rc.json";
pub const ENDPOINT_STEP: &str =
    "https://github.com/Bexxs75/3mf-katalog-manager/releases/latest/download/latest-step.json";
pub const ENDPOINT_PREVIEW: &str =
    "https://github.com/Bexxs75/3mf-katalog-manager/releases/download/preview/latest-preview.json";
pub const ENDPOINT_PREVIEW_STEP: &str =
    "https://github.com/Bexxs75/3mf-katalog-manager/releases/download/preview/latest-preview-step.json";
pub const PREVIEW_RELEASE_PAGE: &str = "https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/preview";
pub const ENDPOINT_ENV: &str = "MFK_UPDATE_ENDPOINT";
/// Debug builds only: public key for updates signed with a throwaway test key.
pub const PUBKEY_ENV: &str = "MFK_UPDATE_PUBKEY";
pub const LAST_UPDATE_KEY: &str = "last_update_info";

pub const UPDATE_CHANNEL_KEY: &str = "update_channel";

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateChannel {
    Stable,
    Rc,
}

impl UpdateChannel {
    pub fn resolve(stored: Option<&str>, version: &str) -> Self {
        match stored {
            Some("rc") => Self::Rc,
            Some("stable") => Self::Stable,
            _ if version.contains('-') => Self::Rc,
            _ => Self::Stable,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Rc => "rc",
        }
    }
}

pub fn is_newer(current: &str, candidate: &str) -> bool {
    match (
        semver::Version::parse(current),
        semver::Version::parse(candidate),
    ) {
        (Ok(current), Ok(candidate)) => candidate.cmp_precedence(&current).is_gt(),
        _ => false,
    }
}

/// Stable and preview builds retain their variant endpoints. RC manifests carry
/// the single STEP variant. Preview always stays on its isolated channel.
/// The debug override exists for local update tests.
pub fn endpoint(step: bool, preview: bool, channel: UpdateChannel, allow_override: bool, override_value: Option<&str>) -> String {
    if allow_override {
        if let Some(v) = override_value.map(str::trim).filter(|v| !v.is_empty()) {
            return v.to_string();
        }
    }
    if !preview && channel == UpdateChannel::Rc {
        return ENDPOINT_RC.to_string();
    }
    match (preview, step) {
        (false, false) => ENDPOINT,
        (false, true) => ENDPOINT_STEP,
        (true, false) => ENDPOINT_PREVIEW,
        (true, true) => ENDPOINT_PREVIEW_STEP,
    }
    .to_string()
}

pub fn release_page(version: &str, preview: bool, _channel: UpdateChannel) -> String {
    if preview {
        PREVIEW_RELEASE_PAGE.to_string()
    } else {
        format!("https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v{version}")
    }
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
    fn rc_channel_defaults_and_ordering() {
        assert_eq!(UpdateChannel::resolve(None, "0.16.0-2"), UpdateChannel::Rc);
        assert_eq!(
            UpdateChannel::resolve(None, "0.15.3"),
            UpdateChannel::Stable
        );
        assert_eq!(
            UpdateChannel::resolve(Some("stable"), "0.16.0-2"),
            UpdateChannel::Stable
        );
        assert_eq!(
            endpoint(true, false, UpdateChannel::Rc, false, None),
            ENDPOINT_RC
        );
        assert_eq!(
            endpoint(false, false, UpdateChannel::Rc, false, None),
            ENDPOINT_RC
        );
        assert_eq!(
            release_page("0.16.0-2", false, UpdateChannel::Rc),
            "https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v0.16.0-2"
        );
        assert!(!is_newer("0.16.0+one", "0.16.0+two"));
        assert!(!is_newer("0.16.0", "invalid"));
        assert_eq!(
            endpoint(true, true, UpdateChannel::Rc, false, None),
            ENDPOINT_PREVIEW_STEP
        );
        for (old, new) in [
            ("0.15.3", "0.16.0-2"),
            ("0.16.0-2", "0.16.0-3"),
            ("0.16.0-2", "0.16.0"),
        ] {
            assert!(is_newer(old, new));
            assert!(!is_newer(new, old));
            assert!(!is_newer(old, old));
        }
    }

    #[test]
    fn endpoint_follows_variant_and_channel() {
        assert_eq!(endpoint(false, false, UpdateChannel::Stable, false, None), ENDPOINT);
        assert_eq!(endpoint(true, false, UpdateChannel::Stable, false, None), ENDPOINT_STEP);
        assert_eq!(endpoint(false, true, UpdateChannel::Stable, false, None), ENDPOINT_PREVIEW);
        assert_eq!(endpoint(true, true, UpdateChannel::Stable, false, None), ENDPOINT_PREVIEW_STEP);
    }

    #[test]
    fn override_only_when_allowed_and_not_empty() {
        assert_eq!(endpoint(false, true, UpdateChannel::Stable, true, Some("http://127.0.0.1:8765/latest.json")), "http://127.0.0.1:8765/latest.json");
        assert_eq!(endpoint(false, false, UpdateChannel::Stable, false, Some("http://x/latest.json")), ENDPOINT);
        assert_eq!(endpoint(false, false, UpdateChannel::Stable, true, Some("  ")), ENDPOINT);
    }

    #[test]
    fn preview_endpoints_never_point_to_the_normal_channel() {
        for e in [ENDPOINT_PREVIEW, ENDPOINT_PREVIEW_STEP] {
            assert!(e.contains("/releases/download/preview/"));
            assert!(!e.contains("/releases/latest/"));
        }
    }

    #[test]
    fn release_page_points_to_the_tag_or_the_preview_release() {
        assert_eq!(release_page("0.15.1", false, UpdateChannel::Stable), "https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v0.15.1");
        assert_eq!(release_page("0.15.0-2", true, UpdateChannel::Stable), "https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/preview");
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
            backup_file: "Katalog-Sicherung_2026-10-03_0930_vor-Update_0.15.0_auf_0.15.1.db".into(),
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
