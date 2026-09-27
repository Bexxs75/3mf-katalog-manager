//! Tauri commands for updating the app from within itself: check the manifest,
//! download the signed package with progress, discard it, or back up the
//! catalog and install it.
use super::*;
use tauri::ipc::Channel;
use tauri::Manager;
use tauri_plugin_updater::UpdaterExt;

use crate::db::printer_link::{get_setting, set_setting};
use crate::updater::{self, backup, LastUpdateInfo};

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
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

// Kept as the plugin's own error type (not stringified) so callers can tell a
// transport hiccup apart from a broken manifest - see `is_transport_error` below.
async fn fetch_update(app: &tauri::AppHandle) -> Result<Option<tauri_plugin_updater::Update>, tauri_plugin_updater::Error> {
    let override_value = std::env::var(updater::ENDPOINT_ENV).ok();
    let url = updater::endpoint(cfg!(feature = "step-preview"), cfg!(debug_assertions), override_value.as_deref());
    let url = url.parse()?;
    let mut builder = app.updater_builder();
    if cfg!(debug_assertions) {
        if let Some(key) = std::env::var(updater::PUBKEY_ENV).ok().filter(|k| !k.trim().is_empty()) {
            builder = builder.pubkey(key);
        }
    }
    builder.endpoints(vec![url])?.build()?.check().await
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

#[tauri::command]
pub async fn check_app_update(app: tauri::AppHandle, state: State<'_, AppState>) -> CmdResult<UpdateInfoDto> {
    let current = env!("CARGO_PKG_VERSION");
    let stored = {
        let conn = lock_db(&state)?;
        get_setting(&conn, updater::LAST_UPDATE_KEY)
            .ok()
            .flatten()
            .and_then(|raw| serde_json::from_str::<LastUpdateInfo>(&raw).ok())
    };
    let available = match fetch_update(&app).await {
        Ok(Some(u)) => {
            log::info!(target: "update", "Update-Check: installiert {current}, verfügbar {}", u.version);
            Some(u.version)
        }
        Ok(None) => {
            log::info!(target: "update", "Update-Check: {current} ist aktuell");
            None
        }
        // No visible error for the check itself: offline users should not be bothered.
        Err(e) if is_transport_error(&e) => {
            log::info!(target: "update", "Update-Check nicht möglich: {e}");
            None
        }
        Err(e) => {
            log::warn!(target: "update", "Update-Check nicht möglich: {e}");
            None
        }
    };
    Ok(UpdateInfoDto {
        current_version: current.to_string(),
        release_url: available.as_deref().map(updater::release_page),
        available_version: available,
        can_install: updater::can_self_install(current_os(), std::env::var("APPIMAGE").ok().as_deref()),
        last_update: updater::visible_last_update(stored, current),
    })
}

#[tauri::command]
pub async fn download_app_update(
    app: tauri::AppHandle,
    pending: State<'_, UpdaterState>,
    on_progress: Channel<DownloadProgress>,
) -> CmdResult<String> {
    let update = fetch_update(&app)
        .await
        .map_err(|e| format!("Update-Prüfung fehlgeschlagen: {e}"))?
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
        backup::create(&conn, &dir, &version).and_then(|path| {
            let info = LastUpdateInfo {
                version: version.clone(),
                date: chrono::Local::now().format("%Y-%m-%d").to_string(),
                backup_file: backup::file_name(&version),
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
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_info_dto_is_camel_case() {
        let dto = UpdateInfoDto {
            current_version: "0.15.0".into(),
            available_version: Some("0.15.1".into()),
            release_url: Some("https://example.com".into()),
            can_install: true,
            last_update: None,
        };
        let json = serde_json::to_value(&dto).unwrap();
        let obj = json.as_object().unwrap();
        assert!(obj.contains_key("currentVersion"));
        assert!(obj.contains_key("availableVersion"));
        assert!(obj.contains_key("releaseUrl"));
        assert!(obj.contains_key("canInstall"));
        assert!(obj.contains_key("lastUpdate"));
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
