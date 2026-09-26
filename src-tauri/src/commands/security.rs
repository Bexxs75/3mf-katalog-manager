use super::*;

const GITHUB_REPO_URL_PREFIX: &str = "https://github.com/Bexxs75/3mf-katalog-manager/";
const GITHUB_API_LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/Bexxs75/3mf-katalog-manager/releases/latest";
const DISCORD_INVITE_URL: &str = "https://discord.gg/abfVNfFqu3";

// Only links to our own GitHub repo are opened, although the URL comes from a
// trusted source (GitHub API) - defense in depth in case the API response is ever
// manipulated/proxied or the field schema changes.
fn validate_release_url(url: &str) -> Result<(), String> {
    if url == GITHUB_REPO_URL_PREFIX.trim_end_matches('/') || url.starts_with(GITHUB_REPO_URL_PREFIX) {
        Ok(())
    } else {
        Err("URL zeigt nicht auf das erwartete GitHub-Repository".to_string())
    }
}
/// Own version immediately and without network, so the UI doesn't wait for `check_for_update` (up to 5 s).
#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
#[tauri::command]
pub async fn check_for_update() -> CmdResult<update_check::UpdateCheckResult> {
    let current = env!("CARGO_PKG_VERSION");

    let client = match reqwest::Client::builder()
        .user_agent("3mf-katalog-manager-update-check")
        .timeout(std::time::Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => return Ok(update_check::compare_versions(current, current, "")),
    };

    let response = match client.get(GITHUB_API_LATEST_RELEASE_URL).send().await {
        Ok(r) => r,
        // Network error: silently treat as "no update", never an error dialog.
        Err(_) => return Ok(update_check::compare_versions(current, current, "")),
    };

    #[derive(serde::Deserialize)]
    struct GithubRelease {
        tag_name: String,
        html_url: String,
    }

    let release: GithubRelease = match response.json().await {
        Ok(r) => r,
        Err(_) => return Ok(update_check::compare_versions(current, current, "")),
    };

    Ok(update_check::compare_versions(current, &release.tag_name, &release.html_url))
}
#[tauri::command]
pub fn open_release_url(url: String) -> CmdResult<()> {
    validate_release_url(&url)?;

    #[cfg(target_os = "linux")]
    let mut cmd = std::process::Command::new("xdg-open");
    #[cfg(target_os = "macos")]
    let mut cmd = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut cmd = std::process::Command::new("explorer");

    cmd.arg(&url).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

// No URL from the frontend: the Discord link is static.
#[tauri::command]
pub fn open_discord_invite() -> CmdResult<()> {
    #[cfg(target_os = "linux")]
    let mut cmd = std::process::Command::new("xdg-open");
    #[cfg(target_os = "macos")]
    let mut cmd = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut cmd = std::process::Command::new("explorer");

    cmd.arg(DISCORD_INVITE_URL).spawn().map_err(|e| e.to_string())?;
    Ok(())
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
