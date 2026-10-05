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

/// Opens a folder in the system file manager.
pub(crate) fn open_external(target: &str) -> CmdResult<()> {
    #[cfg(target_os = "linux")]
    let mut cmd = std::process::Command::new("xdg-open");
    #[cfg(target_os = "macos")]
    let mut cmd = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut cmd = std::process::Command::new("explorer");
    super::external_env::sanitize_external_command(&mut cmd);
    cmd.arg(target).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

/// Program and arguments that open `url` in the default browser. On Windows,
/// `explorer <url>` is unreliable: a tester's explorer opened "Documents"
/// instead of the browser for our form URL with `?…&…`. The URL protocol
/// handler behind `rundll32 url.dll,FileProtocolHandler` goes through the
/// shell's normal "open this link" path and takes the URL as one argument.
fn url_opener(os: &str, url: &str) -> (&'static str, Vec<String>) {
    match os {
        "windows" => ("rundll32", vec!["url.dll,FileProtocolHandler".to_string(), url.to_string()]),
        "macos" => ("open", vec![url.to_string()]),
        _ => ("xdg-open", vec![url.to_string()]),
    }
}

/// Opens an http(s) URL in the default browser.
pub(crate) fn open_url(url: &str) -> CmdResult<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(CmdError::expected("Nur Web-Adressen können im Browser geöffnet werden"));
    }
    let (program, args) = url_opener(crate::diagnostics::form_url::current_os(), url);
    let mut cmd = std::process::Command::new(program);
    super::external_env::sanitize_external_command(&mut cmd);
    cmd.args(args).spawn().map_err(|e| e.to_string())?;
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

#[tauri::command]
pub fn open_release_url(url: String) -> CmdResult<()> {
    validate_release_url(&url)?;
    open_url(&url)
}

// No URL from the frontend: the Discord link is static.
#[tauri::command]
pub fn open_discord_invite() -> CmdResult<()> {
    open_url(DISCORD_INVITE_URL)
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
mod url_opener_tests {
    use super::*;

    const FORM: &str = "https://3mfkatalog.de/fehler-melden.html?version=0.15.0-10&os=windows&log=1";

    #[test]
    fn windows_uses_the_url_protocol_handler_not_explorer() {
        let (program, args) = url_opener("windows", FORM);
        assert_eq!(program, "rundll32");
        assert_eq!(args, vec!["url.dll,FileProtocolHandler".to_string(), FORM.to_string()]);
    }

    #[test]
    fn macos_and_linux_pass_the_url_unchanged() {
        assert_eq!(url_opener("macos", FORM), ("open", vec![FORM.to_string()]));
        assert_eq!(url_opener("linux", FORM), ("xdg-open", vec![FORM.to_string()]));
    }

    #[test]
    fn only_web_addresses_are_opened_as_urls() {
        assert!(open_url("file:///C:/Windows").is_err());
        assert!(open_url("C:\\Users").is_err());
    }
}

