use super::*;

const GITHUB_REPO_URL_PREFIX: &str = "https://github.com/Bexxs75/3mf-katalog-manager/";
const DISCORD_INVITE_URL: &str = "https://discord.gg/abfVNfFqu3";
// The website reads these two query/hash parts to preselect the STEP variant
// in its download section (`?step=1#download`); only the German page and the
// English one exist, so every non-German language falls back to English.
const STEP_DOWNLOAD_URL_DE: &str = "https://3mfkatalog.de/?step=1#download";
const STEP_DOWNLOAD_URL_EN: &str = "https://3mfkatalog.de/en/?step=1#download";

fn step_download_url(lang: &str) -> &'static str {
    if lang == "de" {
        STEP_DOWNLOAD_URL_DE
    } else {
        STEP_DOWNLOAD_URL_EN
    }
}

// The frontend passes this URL in (release notes link, Discord/report links go
// through their own dedicated commands), so it's untrusted input as far as this
// command is concerned. Restricting it to our own repo keeps "open external URL"
// from becoming an arbitrary-URL opener for whatever the renderer ends up holding.
fn validate_release_url(url: &str) -> Result<(), String> {
    if url == GITHUB_REPO_URL_PREFIX.trim_end_matches('/') || url.starts_with(GITHUB_REPO_URL_PREFIX) {
        Ok(())
    } else {
        Err("URL zeigt nicht auf das erwartete GitHub-Repository".to_string())
    }
}

/// Opens a URL or folder with the system default handler.
pub(crate) fn open_external(target: &str) -> CmdResult<()> {
    #[cfg(target_os = "linux")]
    let mut cmd = std::process::Command::new("xdg-open");
    #[cfg(target_os = "macos")]
    let mut cmd = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut cmd = std::process::Command::new("explorer");
    cmd.arg(target).spawn().map_err(|e| e.to_string())?;
    Ok(())
}
/// Own version immediately and without network, so the UI doesn't wait on `check_app_update`.
#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Lets the UI say that this is a test version with its own catalog.
#[tauri::command]
pub fn is_preview_build() -> bool {
    cfg!(feature = "preview")
}

/// Lets the UI show an explanation instead of a generic error when a STEP
/// file has no 3D preview in this build.
#[tauri::command]
pub fn has_step_preview() -> bool {
    cfg!(feature = "step-preview")
}

// No URL from the frontend: only the UI's current language selects between
// the two fixed download URLs.
#[tauri::command]
pub fn open_step_download(lang: String) -> CmdResult<()> {
    open_external(step_download_url(&lang))
}
#[tauri::command]
pub fn open_release_url(url: String) -> CmdResult<()> {
    validate_release_url(&url)?;
    open_external(&url)
}

// No URL from the frontend: the Discord link is static.
#[tauri::command]
pub fn open_discord_invite() -> CmdResult<()> {
    open_external(DISCORD_INVITE_URL)
}

#[cfg(test)]
mod update_command_tests {
    use super::*;

    #[test]
    fn accepts_the_projects_own_github_release_url() {
        assert!(validate_release_url(
            "https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v0.7.9"
        )
        .is_ok());
    }

    #[test]
    fn rejects_a_non_https_scheme() {
        assert!(validate_release_url(
            "javascript:alert(1)"
        )
        .is_err());
    }

    #[test]
    fn rejects_a_different_host() {
        assert!(validate_release_url("https://evil.example.com/releases/tag/v9.9.9").is_err());
    }

    #[test]
    fn rejects_a_different_github_repo() {
        assert!(validate_release_url("https://github.com/someone-else/other-repo").is_err());
    }

    #[test]
    fn accepts_the_repo_root_url_with_and_without_trailing_slash() {
        assert!(validate_release_url("https://github.com/Bexxs75/3mf-katalog-manager").is_ok());
        assert!(validate_release_url("https://github.com/Bexxs75/3mf-katalog-manager/").is_ok());
    }
}

#[cfg(test)]
mod step_download_tests {
    use super::*;

    #[test]
    fn only_german_gets_the_german_download_page() {
        assert_eq!(step_download_url("de"), STEP_DOWNLOAD_URL_DE);
    }

    #[test]
    fn every_other_language_falls_back_to_english() {
        for lang in ["en", "es", "fr", "", "de-DE", "pt"] {
            assert_eq!(step_download_url(lang), STEP_DOWNLOAD_URL_EN);
        }
    }

    // The frontend must not be able to steer this at all - unlike
    // `open_release_url`, there is no `url` parameter to validate.
    #[test]
    fn the_command_takes_a_language_and_nothing_else() {
        fn assert_signature(_f: fn(String) -> CmdResult<()>) {}
        assert_signature(open_step_download);
    }
}
