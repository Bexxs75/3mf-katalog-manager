use super::*;

const GITHUB_REPO_URL_PREFIX: &str = "https://github.com/Bexxs75/3mf-katalog-manager/";
const GITHUB_API_LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/Bexxs75/3mf-katalog-manager/releases/latest";
const DISCORD_INVITE_URL: &str = "https://discord.gg/abfVNfFqu3";

// Nur Links auf das eigene GitHub-Repo werden geöffnet, obwohl die URL aus
// einer vertrauten Quelle (GitHub-API) stammt - Defense-in-depth, falls die
// API-Antwort je manipuliert/geproxyt wird oder sich das Feld-Schema ändert.
fn validate_release_url(url: &str) -> Result<(), String> {
    if url == GITHUB_REPO_URL_PREFIX.trim_end_matches('/') || url.starts_with(GITHUB_REPO_URL_PREFIX) {
        Ok(())
    } else {
        Err("URL zeigt nicht auf das erwartete GitHub-Repository".to_string())
    }
}
/// Liefert die eigene App-Version synchron und ohne Netzwerkzugriff, damit
/// die UI die Versionsnummer sofort anzeigen kann, statt auf den (bis zu
/// 5s dauernden) Netzwerk-Roundtrip von `check_for_update` zu warten
/// (Final-Review Finding F2).
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
        // Netzwerkfehler/kein Internet: still als "kein Update" behandeln,
        // niemals einen Fehlerdialog zeigen (siehe Global Constraints).
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

// Nimmt bewusst keine URL vom Frontend entgegen (anders als open_release_url,
// dessen URL je nach Release-Tag variiert) - der Discord-Invite-Link ist
// statisch, es gibt also keinen Grund, ihn ueberhaupt als Parameter
// entgegenzunehmen.
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
