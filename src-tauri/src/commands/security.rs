use super::*;

const GITHUB_REPO_URL_PREFIX: &str = "https://github.com/Bexxs75/3mf-katalog-manager/";
const DISCORD_INVITE_URL: &str = "https://discord.gg/abfVNfFqu3";

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
