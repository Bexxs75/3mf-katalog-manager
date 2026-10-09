//! Tauri commands for updating the app from within itself: check the manifest,
//! download the signed package with progress, discard it, or back up the
//! catalog and install it.
use super::*;
use std::future::Future;
use std::time::Duration;
use tauri::ipc::Channel;
use tauri::Manager;
use tauri_plugin_updater::UpdaterExt;

use crate::db::printer_link::{get_setting, set_setting};
use crate::updater::{self, backup, LastUpdateInfo, UpdateChannel};

#[derive(Default)]
pub struct UpdaterState {
    /// Downloaded and signature-checked update waiting for "restart and install".
    pending: Mutex<Option<(tauri_plugin_updater::Update, Vec<u8>)>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfoDto {
    pub current_version: String,
    pub available_version: Option<String>,
    pub release_url: Option<String>,
    pub can_install: bool,
    pub last_update: Option<LastUpdateInfo>,
    /// The check itself failed (offline, firewall, ...). The start-up toast stays
    /// silent about it, but the Info tab must not claim "up to date".
    pub check_failed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

// Kept as the plugin's own error type (not stringified) so callers can tell a
// transport hiccup apart from a broken manifest - see `is_transport_error` below.
async fn fetch_update(app: &tauri::AppHandle, channel: UpdateChannel) -> Result<Option<tauri_plugin_updater::Update>, tauri_plugin_updater::Error> {
    let override_value = std::env::var(updater::ENDPOINT_ENV).ok();
    let url = updater::endpoint(
        cfg!(feature = "step-preview"),
        cfg!(feature = "preview"),
        channel,
        cfg!(debug_assertions),
        override_value.as_deref(),
    );
    let url = url.parse()?;
    let mut builder = app.updater_builder();
    if cfg!(debug_assertions) {
        if let Some(key) = std::env::var(updater::PUBKEY_ENV).ok().filter(|k| !k.trim().is_empty()) {
            builder = builder.pubkey(key);
        }
    }
    // Use the same SemVer ordering for checks and downloads; a channel switch
    // must never turn a stable manifest into a downgrade.
    builder = builder.version_comparator(|current, release| {
        updater::is_newer(&current.to_string(), &release.version.to_string())
    });
    let updater = builder.endpoints(vec![url])?.build()?;
    retry_once(
        || updater.check(),
        is_transport_error,
        Duration::from_secs(2),
        |e| log::info!(target: "update", "Update-Prüfung fehlgeschlagen, neuer Versuch in 2 s: {}", error_chain(e)),
    )
    .await
}

async fn retry_once<T, E, F, Fut>(
    mut op: F,
    is_retryable: impl Fn(&E) -> bool,
    delay: Duration,
    on_retry: impl FnOnce(&E),
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    match op().await {
        Err(e) if is_retryable(&e) => {
            on_retry(&e);
            // Keep the pause off the async workers; a failed wait must not suppress the retry.
            let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(delay)).await;
            op().await
        }
        result => result,
    }
}

fn error_chain(e: &dyn std::error::Error) -> String {
    let mut previous = e.to_string();
    let mut chain = previous.clone();
    let mut source = e.source();
    while let Some(cause) = source {
        let message = cause.to_string();
        if message != previous {
            chain.push_str(": ");
            chain.push_str(&message);
        }
        previous = message;
        source = cause.source();
    }
    chain
}

/// A plain network/transport hiccup (offline, DNS, TLS, a dropped connection, ...)
/// is routine and not worth more than an info line. Anything else reaching this
/// point - an unparsable body, a missing platform entry, a malformed endpoint URL -
/// means our own release process shipped something broken and deserves a warning.
fn is_transport_error(e: &tauri_plugin_updater::Error) -> bool {
    matches!(e, tauri_plugin_updater::Error::Reqwest(_) | tauri_plugin_updater::Error::Network(_))
}

fn current_os() -> &'static str {
    crate::diagnostics::form_url::current_os()
}

fn read_channel(state: &State<'_, AppState>) -> CmdResult<UpdateChannel> {
    if cfg!(feature = "preview") {
        return Ok(UpdateChannel::Stable);
    }
    let conn = lock_db(state)?;
    let stored = get_setting(&conn, updater::UPDATE_CHANNEL_KEY).map_err(|e| e.to_string())?;
    Ok(UpdateChannel::resolve(
        stored.as_deref(),
        env!("CARGO_PKG_VERSION"),
    ))
}

#[tauri::command]
pub fn get_update_channel(state: State<AppState>) -> CmdResult<UpdateChannel> {
    read_channel(&state)
}

#[tauri::command]
pub fn set_update_channel(state: State<AppState>, channel: UpdateChannel) -> CmdResult<()> {
    let conn = lock_db(&state)?;
    set_setting(&conn, updater::UPDATE_CHANNEL_KEY, channel.as_str()).map_err(|e| e.to_string())?;
    Ok(())
}

fn update_check_result(
    current: &str,
    channel: UpdateChannel,
    result: Result<Option<tauri_plugin_updater::Update>, tauri_plugin_updater::Error>,
) -> (Option<String>, bool) {
    match result {
        Ok(Some(u)) => {
            log::info!(target: "update", "Update-Check: installiert {current}, verfügbar {}", u.version);
            (Some(u.version), false)
        }
        Ok(None) => {
            log::info!(target: "update", "Update-Check: {current} ist aktuell");
            (None, false)
        }
        // The RC pointer may not exist before its first publication. The plugin
        // reports unsuccessful HTTP responses as ReleaseNotFound, but preserves
        // transport and JSON errors; only this typed RC case means "up to date".
        Err(tauri_plugin_updater::Error::ReleaseNotFound) if channel == UpdateChannel::Rc => {
            log::info!(target: "update", "RC-Verweis noch nicht veröffentlicht");
            (None, false)
        }
        // No toast for the check itself: offline users should not be bothered.
        Err(e) if is_transport_error(&e) => {
            log::warn!(target: "update", "Update-Check nicht möglich: {}", error_chain(&e));
            (None, true)
        }
        Err(e) => {
            log::warn!(target: "update", "Update-Check nicht möglich: {}", error_chain(&e));
            (None, true)
        }
    }
}

#[tauri::command]
pub async fn check_app_update(app: tauri::AppHandle, state: State<'_, AppState>) -> CmdResult<UpdateInfoDto> {
    let channel = read_channel(&state)?;
    let current = env!("CARGO_PKG_VERSION");
    let stored = {
        let conn = lock_db(&state)?;
        get_setting(&conn, updater::LAST_UPDATE_KEY)
            .ok()
            .flatten()
            .and_then(|raw| serde_json::from_str::<LastUpdateInfo>(&raw).ok())
    };
    let (available, check_failed) = update_check_result(current, channel, fetch_update(&app, channel).await);
    Ok(UpdateInfoDto {
        current_version: current.to_string(),
        release_url: available.as_deref().map(|v| updater::release_page(v, cfg!(feature = "preview"), channel)),
        available_version: available,
        can_install: updater::can_self_install(current_os(), std::env::var("APPIMAGE").ok().as_deref()),
        last_update: updater::visible_last_update(stored, current),
        check_failed,
    })
}

#[tauri::command]
pub async fn download_app_update(
    app: tauri::AppHandle,
    pending: State<'_, UpdaterState>,
    on_progress: Channel<DownloadProgress>,
    state: State<'_, AppState>,
) -> CmdResult<String> {
    let update = fetch_update(&app, read_channel(&state)?)
        .await
        .map_err(|e| format!("Update-Prüfung fehlgeschlagen: {}", error_chain(&e)))?
        .ok_or_else(|| CmdError::expected("Es ist kein Update mehr verfügbar"))?;
    log::info!(target: "update", "Download von {} gestartet", update.version);
    let mut downloaded = 0u64;
    let bytes = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = on_progress.send(DownloadProgress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(|e| format!("Download fehlgeschlagen: {e}"))?;
    log::info!(target: "update", "Download von {} fertig ({} Bytes, Signatur geprüft)", update.version, bytes.len());
    let version = update.version.clone();
    *pending.pending.lock().map_err(|_| "update lock poisoned".to_string())? = Some((update, bytes));
    Ok(version)
}

#[tauri::command]
pub fn discard_app_update(pending: State<UpdaterState>) -> CmdResult<()> {
    *pending.pending.lock().map_err(|_| "update lock poisoned".to_string())? = None;
    log::info!(target: "update", "Heruntergeladenes Update verworfen");
    Ok(())
}

/// A failed backup must not lose the downloaded update: only a successful backup may
/// consume the pending slot, so a failure leaves the download in place for the next
/// "restart and install" click to retry without fetching it again.
fn take_pending_if_backup_ok<T>(pending: &mut Option<T>, backup_succeeded: bool) -> Option<T> {
    if backup_succeeded { pending.take() } else { None }
}

// `async`: the backup below holds the catalog lock and, however briefly, does file and
// SQLite work that must not run on the main/UI thread, which is where a plain
// `#[tauri::command]` executes.
#[tauri::command(async)]
pub fn install_app_update(app: tauri::AppHandle, state: State<AppState>, pending: State<UpdaterState>) -> CmdResult<()> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join("update-backups");
    let mut guard = pending.pending.lock().map_err(|_| "update lock poisoned".to_string())?;
    let version = guard
        .as_ref()
        .map(|(update, _)| update.version.clone())
        .ok_or_else(|| CmdError::expected("Es liegt kein heruntergeladenes Update vor"))?;

    let backup_result: Result<PathBuf, String> = {
        let conn = lock_db(&state)?;
        backup::create(&conn, &dir, env!("CARGO_PKG_VERSION"), &version).and_then(|path| {
            let info = LastUpdateInfo {
                version: version.clone(),
                date: chrono::Local::now().format("%Y-%m-%d").to_string(),
                backup_file: path.file_name().unwrap().to_string_lossy().into_owned(),
            };
            let json = serde_json::to_string(&info).map_err(|e| e.to_string())?;
            set_setting(&conn, updater::LAST_UPDATE_KEY, &json).map_err(|e| e.to_string())?;
            Ok(path)
        })
    };
    let taken = take_pending_if_backup_ok(&mut guard, backup_result.is_ok());
    drop(guard);
    let path = backup_result.map_err(|e| format!("Sicherung fehlgeschlagen, das Update wurde nicht installiert: {e}"))?;
    let (update, bytes) =
        taken.expect("pending update is still present: the backup succeeded and the lock was held throughout");
    log::info!(target: "update", "Katalog gesichert: {}", path.display());
    log::info!(target: "update", "Installation von {} gestartet", update.version);
    update.install(bytes).map_err(|e| format!("Installation fehlgeschlagen: {e}"))?;
    // Windows exits inside install() and the installer restarts the app; on macOS
    // and Linux the new bundle is in place and we restart into it. Off the main
    // thread (see the `async` attribute above), this still restarts reliably.
    // The restart fallback can skip Exit; release the Unix lock before spawning
    // the replacement process even when the event loop cannot accept the exit.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    if crate::single_instance::single_instance_enabled(
        cfg!(debug_assertions),
        std::env::var(crate::single_instance::ALLOW_MULTIPLE_ENV).ok().as_deref(),
    ) {
        tauri_plugin_single_instance::destroy(&app);
    }
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rc_release_not_found_is_up_to_date() {
        assert_eq!(
            update_check_result("0.16.0-2", UpdateChannel::Rc, Err(tauri_plugin_updater::Error::ReleaseNotFound)),
            (None, false),
        );
    }

    #[test]
    fn rc_transport_error_still_fails() {
        assert_eq!(
            update_check_result("0.16.0-2", UpdateChannel::Rc, Err(tauri_plugin_updater::Error::Network("offline".into()))),
            (None, true),
        );
    }

    #[test]
    fn stable_release_not_found_still_fails() {
        assert_eq!(
            update_check_result("0.16.0", UpdateChannel::Stable, Err(tauri_plugin_updater::Error::ReleaseNotFound)),
            (None, true),
        );
    }

    #[test]
    fn rc_invalid_manifest_still_fails() {
        let error = serde_json::from_str::<serde_json::Value>("invalid JSON").unwrap_err();
        assert_eq!(
            update_check_result("0.16.0-2", UpdateChannel::Rc, Err(error.into())),
            (None, true),
        );
    }

    fn run_retry(
        outcomes: Vec<Result<u8, tauri_plugin_updater::Error>>,
    ) -> (Result<u8, tauri_plugin_updater::Error>, usize, Vec<String>) {
        let mut outcomes = outcomes.into_iter();
        let mut calls = 0;
        let mut notifications = Vec::new();
        let result = tauri::async_runtime::block_on(retry_once(
            || {
                calls += 1;
                std::future::ready(outcomes.next().expect("unexpected extra attempt"))
            },
            is_transport_error,
            std::time::Duration::ZERO,
            |e| notifications.push(e.to_string()),
        ));
        (result, calls, notifications)
    }

    #[test]
    fn retry_once_succeeds_on_first_attempt() {
        let (result, calls, notifications) = run_retry(vec![Ok(42)]);
        assert_eq!(result.unwrap(), 42);
        assert_eq!(calls, 1);
        assert!(notifications.is_empty());
    }

    #[test]
    fn retry_once_recovers_from_transport_error() {
        let error = tauri_plugin_updater::Error::Network("offline".into());
        let message = error.to_string();
        let (result, calls, notifications) = run_retry(vec![Err(error), Ok(42)]);
        assert_eq!(result.unwrap(), 42);
        assert_eq!(calls, 2);
        assert_eq!(notifications, vec![message]);
    }

    #[test]
    fn retry_once_does_not_retry_non_transport_error() {
        let (result, calls, notifications) = run_retry(vec![Err(tauri_plugin_updater::Error::ReleaseNotFound)]);
        assert!(matches!(result, Err(tauri_plugin_updater::Error::ReleaseNotFound)));
        assert_eq!(calls, 1);
        assert!(notifications.is_empty());
    }

    #[test]
    fn retry_once_returns_second_transport_error() {
        let first = tauri_plugin_updater::Error::Network("first".into());
        let message = first.to_string();
        let (result, calls, notifications) = run_retry(vec![
            Err(first),
            Err(tauri_plugin_updater::Error::Network("second".into())),
        ]);
        assert!(matches!(result, Err(tauri_plugin_updater::Error::Network(message)) if message == "second"));
        assert_eq!(calls, 2);
        assert_eq!(notifications, vec![message]);
    }

    #[derive(Debug)]
    struct NestedError {
        message: &'static str,
        source: Option<Box<NestedError>>,
    }

    impl std::fmt::Display for NestedError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.message)
        }
    }

    impl std::error::Error for NestedError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            self.source.as_deref().map(|e| e as &dyn std::error::Error)
        }
    }

    #[test]
    fn error_chain_includes_nested_causes() {
        let error = NestedError {
            message: "request failed",
            source: Some(Box::new(NestedError {
                message: "connection failed",
                source: Some(Box::new(NestedError { message: "DNS failure", source: None })),
            })),
        };
        assert_eq!(error_chain(&error), "request failed: connection failed: DNS failure");
    }

    #[test]
    fn error_chain_skips_adjacent_duplicates_and_keeps_later_causes() {
        let error = NestedError {
            message: "request failed",
            source: Some(Box::new(NestedError {
                message: "request failed",
                source: Some(Box::new(NestedError { message: "DNS failure", source: None })),
            })),
        };
        assert_eq!(error_chain(&error), "request failed: DNS failure");
    }

    #[test]
    fn update_info_dto_is_camel_case() {
        let dto = UpdateInfoDto {
            current_version: "0.15.0".into(),
            available_version: Some("0.15.1".into()),
            release_url: Some("https://example.com".into()),
            can_install: true,
            last_update: None,
            check_failed: false,
        };
        let json = serde_json::to_value(&dto).unwrap();
        let obj = json.as_object().unwrap();
        assert!(obj.contains_key("currentVersion"));
        assert!(obj.contains_key("availableVersion"));
        assert!(obj.contains_key("releaseUrl"));
        assert!(obj.contains_key("canInstall"));
        assert!(obj.contains_key("lastUpdate"));
        assert!(obj.contains_key("checkFailed"));
    }

    #[test]
    fn download_progress_is_camel_case() {
        let progress = DownloadProgress { downloaded: 5, total: Some(10) };
        assert_eq!(serde_json::to_string(&progress).unwrap(), r#"{"downloaded":5,"total":10}"#);
    }

    #[test]
    fn a_failed_backup_leaves_the_pending_update_in_place() {
        let mut pending = Some(42);
        assert_eq!(take_pending_if_backup_ok(&mut pending, false), None);
        assert_eq!(pending, Some(42));
    }

    #[test]
    fn a_successful_backup_takes_the_pending_update() {
        let mut pending = Some(42);
        assert_eq!(take_pending_if_backup_ok(&mut pending, true), Some(42));
        assert_eq!(pending, None);
    }

    #[test]
    fn network_errors_are_transport_errors() {
        assert!(is_transport_error(&tauri_plugin_updater::Error::Network("offline".into())));
    }

    #[test]
    fn a_broken_manifest_is_not_a_transport_error() {
        assert!(!is_transport_error(&tauri_plugin_updater::Error::ReleaseNotFound));
        assert!(!is_transport_error(&tauri_plugin_updater::Error::EmptyEndpoints));
    }
}
